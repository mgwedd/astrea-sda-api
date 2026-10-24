//! `/viewer` shell: security headers on every response, and Cesium served only when configured.
use astrea_sda_api::viewer;
use axum::{body::Body, http::Request};
use tower::ServiceExt;

async fn get(app: axum::Router, path: &str) -> axum::response::Response {
    app.oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn shell_pages_carry_strict_security_headers() {
    for path in ["/viewer", "/viewer/app.js", "/viewer/app.css"] {
        let res = get(viewer::router(None), path).await;
        assert_eq!(res.status(), 200, "{path}");
        let h = res.headers();
        let csp = h["content-security-policy"].to_str().unwrap();
        assert!(csp.contains("default-src 'self'"), "{path}: {csp}");
        assert!(csp.contains("frame-ancestors 'none'"), "{path}: {csp}");
        let script_src = csp
            .split(';')
            .find(|d| d.trim_start().starts_with("script-src"))
            .unwrap();
        assert!(!script_src.contains("unsafe-inline"), "{path}: {csp}");
        assert!(
            !csp.contains("http"),
            "{path}: no third-party origin: {csp}"
        );
        assert_eq!(h["referrer-policy"], "no-referrer", "{path}");
        assert_eq!(h["cache-control"], "no-store", "{path}");
        assert_eq!(h["x-content-type-options"], "nosniff", "{path}");
    }
}

#[tokio::test]
async fn cesium_is_served_only_from_the_configured_dir() {
    assert_eq!(
        get(viewer::router(None), "/viewer/cesium/Cesium.js")
            .await
            .status(),
        404
    );

    let dir = std::env::temp_dir().join(format!("astrea-viewer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Cesium.js"), "var Cesium={};").unwrap();
    let res = get(
        viewer::router(Some(dir.clone())),
        "/viewer/cesium/Cesium.js",
    )
    .await;
    assert_eq!(res.status(), 200);
    assert!(res.headers().contains_key("content-security-policy"));
    // Traversal must not escape the assets dir.
    let res = get(
        viewer::router(Some(dir.clone())),
        "/viewer/cesium/..%2F..%2Fetc%2Fpasswd",
    )
    .await;
    assert_ne!(res.status(), 200);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn token_is_never_persisted_by_the_client() {
    let js = include_str!("../viewer/app.js");
    for banned in [
        "localStorage",
        "sessionStorage",
        "document.cookie",
        "location.search",
    ] {
        assert!(!js.contains(banned), "app.js must not use {banned}");
    }
}

#[test]
fn deep_link_hash_never_encodes_auth_tokens_or_observer_coordinates() {
    let js = include_str!("../viewer/app.js");

    // location.hash is supported exclusively for #scene= deep links
    assert!(
        js.contains("location.hash"),
        "app.js must support #scene= hash deep linking"
    );
    assert!(
        js.contains("#scene="),
        "app.js must use #scene= prefix for deep linking"
    );

    // Strict security rule: tokens must NEVER be placed in the hash or storage
    assert!(
        !js.contains("location.hash = token"),
        "token must never be assigned to location.hash"
    );
    assert!(
        !js.contains("location.hash = `Bearer"),
        "Bearer token must never be in location.hash"
    );
    assert!(
        !js.contains("location.hash = 'Bearer"),
        "Bearer token must never be in location.hash"
    );

    // Sanitization must explicitly strip token/auth keys and observer coordinates
    assert!(
        js.contains("sanitizeSceneConfig"),
        "app.js must sanitize scene configurations"
    );
    for banned_key in [
        "token", "auth", "jwt", "password", "observer", "lat", "lon", "altm",
    ] {
        assert!(
            js.contains(banned_key),
            "sanitizer in app.js must handle {banned_key}"
        );
    }
}

#[test]
fn compare_mode_ui_and_color_differentiation_are_present() {
    let html = include_str!("../viewer/index.html");
    let css = include_str!("../viewer/app.css");
    let js = include_str!("../viewer/app.js");

    // UI elements in index.html
    assert!(
        html.contains("id=\"compare-mode\""),
        "index.html must have compare-mode checkbox"
    );
    assert!(
        html.contains("id=\"sat-b\""),
        "index.html must have sat-b comparison selector"
    );
    assert!(
        html.contains("id=\"layer-type\""),
        "index.html must have layer-type selector"
    );
    assert!(
        html.contains("id=\"share-link\""),
        "index.html must have share-link button"
    );
    assert!(
        html.contains("id=\"compare-legend\""),
        "index.html must have compare legend"
    );

    // Differentiated color schemes in css and js
    // Scene A: Cyan (#00e5ff / [0, 229, 255, 255])
    // Scene B: Amber / Neon Orange (#ff9100 / [255, 145, 0, 255])
    assert!(css.contains("#00e5ff"), "css must style Scene A in Cyan");
    assert!(
        css.contains("#ff9100"),
        "css must style Scene B in Amber/Orange"
    );
    assert!(
        js.contains("255, 145, 0, 255"),
        "js must color Scene B entities in Orange"
    );
    assert!(
        js.contains("0, 229, 255, 255"),
        "js must color Scene A entities in Cyan"
    );

    // Entity collision prevention
    assert!(
        js.contains("compare_"),
        "js must namespace comparison packet IDs to avoid entity collision"
    );
    assert!(js.contains("[A]"), "js must badge Scene A entities");
    assert!(js.contains("[B]"), "js must badge Scene B entities");
}
