//! Cache keys must not collapse distinct requests: times 1 s apart (same old 10 s / 1 min bucket)
//! and coordinates 0.001 deg apart (same old 2-decimal rounding) must return distinct results.
use astrea_sda_api::{create_router, repository::SatelliteRepository};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;

const ISS: (&str, &str) = (
    "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996",
    "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012",
);
const CSS: (&str, &str) = (
    "1 48274U 21035A   24083.91428787  .00021345  00000+0  22143-3 0  9998",
    "2 48274  41.4728 112.5843 0004928  64.2185 295.9537 15.60271542164426",
);

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Value {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let req = match body {
        Some(v) => b
            .header("content-type", "application/json")
            .body(Body::from(v.to_string())),
        None => b.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(
        status.is_success(),
        "{uri} -> {status}: {}",
        String::from_utf8_lossy(&bytes)
    );
    assert_ne!(status, StatusCode::NO_CONTENT);
    serde_json::from_slice(&bytes).unwrap()
}

async fn setup() -> (Router, String, String, String) {
    let app = create_router(SatelliteRepository::new(None).await);
    let auth = call(
        &app,
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({"email": "admin@astrea.local", "password": "password123"})),
    )
    .await;
    let token = auth["token"].as_str().unwrap().to_string();
    let mut ids = vec![];
    for (name, (l1, l2)) in [("ISS", ISS), ("CSS", CSS)] {
        let sat = call(
            &app,
            "POST",
            "/v1/satellites",
            Some(&token),
            Some(json!({"name": name, "tleLineOne": l1, "tleLineTwo": l2})),
        )
        .await;
        ids.push(sat["id"].as_str().unwrap().to_string());
    }
    (app, token, ids.remove(0), ids.remove(0))
}

/// Same endpoint, two URIs that old bucketing/rounding mapped to one key: results must differ.
async fn assert_distinct(app: &Router, token: &str, a: &str, b: &str) {
    let ra = call(app, "GET", a, Some(token), None).await;
    let rb = call(app, "GET", b, Some(token), None).await;
    assert_ne!(ra, rb, "cache collapsed distinct requests:\n{a}\n{b}");
}

/// Sub-satellite point of the ISS at T1, so /overhead finds a satellite (it 404s otherwise).
async fn ground_point(app: &Router, tok: &str, iss: &str) -> (f64, f64) {
    let st = call(
        app,
        "GET",
        &format!("/v1/satellites/{iss}/state?epoch={T1}"),
        Some(tok),
        None,
    )
    .await;
    let r2 = |v: &Value| (v.as_f64().unwrap() * 100.0).round() / 100.0;
    (r2(&st["latitudeDeg"]), r2(&st["longitudeDeg"]))
}

// 12:00:01 and 12:00:02 share both the 10 s bucket and the 1 min bucket.
const T1: &str = "2024-03-24T12:00:01Z";
const T2: &str = "2024-03-24T12:00:02Z";

#[tokio::test]
async fn time_keyed_endpoints_do_not_bucket_time() {
    let (app, tok, iss, css) = setup().await;
    let (la, lo) = ground_point(&app, &tok, &iss).await;
    let loc = format!("lat={la}&lon={lo}");
    for (path, q) in [
        (
            "/v1/satellites/overhead".to_string(),
            format!("{loc}&time="),
        ),
        (
            format!("/v1/satellites/{iss}/illumination"),
            format!("{loc}&time="),
        ),
        (
            format!("/v1/satellites/{iss}/doppler"),
            format!("{loc}&center_freq_hz=437500000&time="),
        ),
        (
            format!("/v1/satellites/{iss}/relative-motion"),
            format!("target_id={css}&time="),
        ),
        (format!("/v1/satellites/{iss}/state"), "epoch=".to_string()),
        (
            format!("/v1/satellites/{iss}/groundtrack"),
            "duration_minutes=5&start_time=".to_string(),
        ),
    ] {
        assert_distinct(
            &app,
            &tok,
            &format!("{path}?{q}{T1}"),
            &format!("{path}?{q}{T2}"),
        )
        .await;
    }
}

#[tokio::test]
async fn identical_requests_still_hit_the_cache() {
    let (app, tok, iss, _) = setup().await;
    let uri = format!("/v1/satellites/{iss}/state?epoch={T1}");
    let a = call(&app, "GET", &uri, Some(&tok), None).await;
    let b = call(&app, "GET", &uri, Some(&tok), None).await;
    assert_eq!(a, b);
}

#[tokio::test]
async fn coordinates_are_not_rounded_in_keys() {
    let (app, tok, iss, _) = setup().await;
    // 0.001 deg is the same key under the old {:.2} rounding
    let (la, lo) = ground_point(&app, &tok, &iss).await;
    for (path, q) in [
        (
            "/v1/satellites/overhead".to_string(),
            format!("lon={lo}&time={T1}&lat="),
        ),
        (
            format!("/v1/satellites/{iss}/illumination"),
            format!("lon={lo}&time={T1}&lat="),
        ),
        (
            format!("/v1/satellites/{iss}/doppler"),
            format!("lon={lo}&center_freq_hz=437500000&time={T1}&lat="),
        ),
        (
            format!("/v1/satellites/{iss}/passes"),
            format!("lon={lo}&start_time={T1}&lat="),
        ),
        (
            format!("/v1/satellites/{iss}/next-visible"),
            format!("lon={lo}&lat="),
        ),
    ] {
        assert_distinct(
            &app,
            &tok,
            &format!("{path}?{q}{la}"),
            &format!("{path}?{q}{}", la + 0.001),
        )
        .await;
    }
}
