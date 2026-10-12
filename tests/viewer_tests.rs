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
        "location.hash",
    ] {
        assert!(!js.contains(banned), "app.js must not use {banned}");
    }
}
