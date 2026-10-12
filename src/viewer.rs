//! `/viewer`: static CesiumJS client shell. Holds no state and calls the JSON API from the browser.
//! Cesium itself is not compiled in; it is served from `VIEWER_ASSETS_DIR` (see docs/VIEWER.md).
use axum::{
    http::{header, HeaderName, HeaderValue},
    response::Html,
    routing::get,
    Router,
};
use std::path::PathBuf;
use tower_http::{services::ServeDir, set_header::SetResponseHeaderLayer};

/// Relaxations beyond same-origin are the ones Cesium 1.146 measurably needs (docs/CZML_VIEWER_LOG.md R10):
/// `unsafe-eval` (Knockout, run when Cesium.js loads), `wasm-unsafe-eval`, blob: workers, inline styles.
const CSP: &str = "default-src 'self'; script-src 'self' blob: 'unsafe-eval' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data: blob:; worker-src 'self' blob:; connect-src 'self' data: blob:; \
object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

fn header_layer(name: HeaderName, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(name, HeaderValue::from_static(value))
}

pub fn router(assets_dir: Option<PathBuf>) -> Router {
    let mut r = Router::new()
        .route(
            "/viewer",
            get(|| async { Html(include_str!("../viewer/index.html")) }),
        )
        .route(
            "/viewer/app.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("../viewer/app.js"),
                )
            }),
        )
        .route(
            "/viewer/app.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css")],
                    include_str!("../viewer/app.css"),
                )
            }),
        );
    if let Some(dir) = assets_dir {
        r = r.nest_service("/viewer/cesium", ServeDir::new(dir));
    }
    r.layer(header_layer(header::CONTENT_SECURITY_POLICY, CSP))
        .layer(header_layer(header::REFERRER_POLICY, "no-referrer"))
        .layer(header_layer(header::CACHE_CONTROL, "no-store"))
        .layer(header_layer(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
}
