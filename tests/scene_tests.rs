//! Tests for `POST /v1/scenes` (Phase 3 of Issue #65).
//! Validates composable multi-layer CZML scene generation, clock synchronization,
//! fixed ECEF frame output, degree-5 Lagrange interpolation, input caps, and format handling.

use astrea_sda_api::{
    create_router,
    models::{SceneLayer, SceneRequest},
    repository::SatelliteRepository,
    services::czml::{check_scene_request, MAX_SCENE_LAYERS},
};
use axum::{body::Body, http::Request};
use chrono::{Duration, TimeZone, Utc};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

#[test]
fn unit_check_scene_request_validation() {
    // Empty layers rejected
    let req = SceneRequest {
        start: None,
        duration_hours: Some(6.0),
        step_seconds: Some(30),
        layers: vec![],
    };
    assert!(check_scene_request(&req).is_err());

    // Exceeding 20 layers rejected
    let too_many_layers: Vec<SceneLayer> = (0..MAX_SCENE_LAYERS + 1)
        .map(|_| SceneLayer::Groundtrack {
            satellite_id: Uuid::new_v4(),
            label: None,
            rgba: None,
        })
        .collect();
    let req = SceneRequest {
        start: None,
        duration_hours: Some(6.0),
        step_seconds: Some(30),
        layers: too_many_layers,
    };
    assert!(check_scene_request(&req).is_err());

    // Duration <= 0 or > 24 h rejected
    let req = SceneRequest {
        start: None,
        duration_hours: Some(0.0),
        step_seconds: Some(30),
        layers: vec![SceneLayer::Groundtrack {
            satellite_id: Uuid::new_v4(),
            label: None,
            rgba: None,
        }],
    };
    assert!(check_scene_request(&req).is_err());

    let req = SceneRequest {
        start: None,
        duration_hours: Some(25.0),
        step_seconds: Some(30),
        layers: vec![SceneLayer::Groundtrack {
            satellite_id: Uuid::new_v4(),
            label: None,
            rgba: None,
        }],
    };
    assert!(check_scene_request(&req).is_err());

    // Step > 120 s rejected
    let req = SceneRequest {
        start: None,
        duration_hours: Some(6.0),
        step_seconds: Some(150),
        layers: vec![SceneLayer::Groundtrack {
            satellite_id: Uuid::new_v4(),
            label: None,
            rgba: None,
        }],
    };
    assert!(check_scene_request(&req).is_err());

    // Samples > 2000 per track rejected (e.g. 24 h @ 10 s = 8640 samples)
    let req = SceneRequest {
        start: None,
        duration_hours: Some(24.0),
        step_seconds: Some(10),
        layers: vec![SceneLayer::Groundtrack {
            satellite_id: Uuid::new_v4(),
            label: None,
            rgba: None,
        }],
    };
    assert!(check_scene_request(&req).is_err());

    // Valid request accepted
    let req = SceneRequest {
        start: None,
        duration_hours: Some(6.0),
        step_seconds: Some(30),
        layers: vec![SceneLayer::Groundtrack {
            satellite_id: Uuid::new_v4(),
            label: None,
            rgba: None,
        }],
    };
    assert!(check_scene_request(&req).is_ok());
}

async fn call(app: &axum::Router, req: Request<Body>) -> (u16, axum::http::HeaderMap, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&bytes).to_string() }));
    (status, headers, body)
}

async fn setup_app() -> (axum::Router, String, String, String) {
    let app = create_router(SatelliteRepository::new(None).await);

    // Login as admin
    let login_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email": "admin@astrea.local", "password": "password123"}).to_string(),
        ))
        .unwrap();
    let (_, _, login_body) = call(&app, login_req).await;
    let token = login_body["token"].as_str().unwrap().to_string();

    // Create Satellite 1: ISS
    let iss_req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "name": "ISS (ZARYA)",
                "tleLineOne": "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996",
                "tleLineTwo": "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012"
            })
            .to_string(),
        ))
        .unwrap();
    let (_, _, iss_body) = call(&app, iss_req).await;
    let iss_id = iss_body["id"].as_str().unwrap().to_string();

    // Create Satellite 2: Tiangong
    let tiangong_req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "name": "CSS (TIANGONG)",
                "tleLineOne": "1 48274U 21035A   24083.91428787  .00021345  00000+0  22143-3 0  9998",
                "tleLineTwo": "2 48274  41.4728 112.5843 0004928  64.2185 295.9537 15.60271542164426"
            })
            .to_string(),
        ))
        .unwrap();
    let (_, _, tiangong_body) = call(&app, tiangong_req).await;
    let tiangong_id = tiangong_body["id"].as_str().unwrap().to_string();

    (app, token, iss_id, tiangong_id)
}

#[tokio::test]
async fn test_scene_composition_multi_layer() {
    let (app, token, iss_id, tiangong_id) = setup_app().await;
    let t0 = Utc.with_ymd_and_hms(2024, 3, 25, 0, 0, 0).unwrap();

    let scene_req = json!({
        "start": t0.to_rfc3339(),
        "durationHours": 4.0,
        "stepSeconds": 60,
        "layers": [
            {
                "type": "groundtrack",
                "satelliteId": iss_id,
                "label": "ISS Orbit",
                "rgba": [0, 200, 255, 255]
            },
            {
                "type": "passes",
                "satelliteId": tiangong_id,
                "label": "Tiangong Passes",
                "observer": {
                    "lat": 28.5,
                    "lon": -80.5,
                    "altM": 10.0
                },
                "minElevation": 10.0
            }
        ]
    });

    let post_req = Request::builder()
        .method("POST")
        .uri("/v1/scenes")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(scene_req.to_string()))
        .unwrap();

    let (status, _, body) = call(&app, post_req).await;
    assert_eq!(status, 200, "Scene generation response: {body}");

    assert_eq!(body["layerCount"], 2);
    assert_eq!(body["droppedSamples"], 0);

    let czml = body["czml"].as_array().expect("czml must be a JSON array");
    assert!(!czml.is_empty(), "CZML document must contain packets");

    // Packet 0: Root document with synchronized clock
    let doc_packet = &czml[0];
    assert_eq!(doc_packet["id"], "document");
    assert_eq!(doc_packet["version"], "1.0");
    assert_eq!(
        doc_packet["clock"]["currentTime"],
        "2024-03-25T00:00:00.000Z"
    );
    let expected_interval = format!(
        "2024-03-25T00:00:00.000Z/{}",
        (t0 + Duration::hours(4)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    );
    assert_eq!(doc_packet["clock"]["interval"], expected_interval);

    // Layer 0 Groundtrack entity verification
    let iss_track = czml
        .iter()
        .find(|p| p["id"] == format!("{iss_id}-layer-0"))
        .expect("ISS groundtrack entity packet must exist");
    assert_eq!(iss_track["name"], "ISS Orbit");
    assert_eq!(iss_track["position"]["referenceFrame"], "FIXED");
    assert_eq!(iss_track["position"]["interpolationAlgorithm"], "LAGRANGE");
    assert_eq!(iss_track["position"]["interpolationDegree"], 5);
    assert_eq!(
        iss_track["path"]["material"]["solidColor"]["color"]["rgba"],
        json!([0, 200, 255, 255])
    );
}

#[tokio::test]
async fn test_scene_raw_czml_format() {
    let (app, token, iss_id, _) = setup_app().await;
    let t0 = Utc.with_ymd_and_hms(2024, 3, 25, 12, 0, 0).unwrap();

    let scene_req = json!({
        "start": t0.to_rfc3339(),
        "durationHours": 2.0,
        "stepSeconds": 60,
        "layers": [
            {
                "type": "groundtrack",
                "satelliteId": iss_id
            }
        ]
    });

    let post_req = Request::builder()
        .method("POST")
        .uri("/v1/scenes?format=czml")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(scene_req.to_string()))
        .unwrap();

    let (status, headers, body) = call(&app, post_req).await;
    assert_eq!(status, 200, "Raw CZML response: {body}");

    // Content-Type must be application/vnd.cesium+json
    let content_type = headers["content-type"].to_str().unwrap();
    assert!(
        content_type.contains("application/vnd.cesium+json"),
        "Content-Type: {content_type}"
    );

    // Response body is the raw CZML array directly
    let czml = body.as_array().expect("Body must be raw CZML JSON array");
    assert_eq!(czml[0]["id"], "document");
    assert_eq!(czml[1]["id"], format!("{iss_id}-layer-0"));
}

#[tokio::test]
async fn test_scene_input_validation_and_caps() {
    let (app, token, iss_id, _) = setup_app().await;

    // Case 1: Empty layers
    let req = Request::builder()
        .method("POST")
        .uri("/v1/scenes")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(json!({ "layers": [] }).to_string()))
        .unwrap();
    let (status, _, body) = call(&app, req).await;
    assert_eq!(status, 400, "Empty layers: {body}");

    // Case 2: Duration > 24 hours
    let req = Request::builder()
        .method("POST")
        .uri("/v1/scenes")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "durationHours": 25.0,
                "layers": [{ "type": "groundtrack", "satelliteId": iss_id }]
            })
            .to_string(),
        ))
        .unwrap();
    let (status, _, body) = call(&app, req).await;
    assert_eq!(status, 400, "Duration > 24h: {body}");

    // Case 3: Step > 120 seconds
    let req = Request::builder()
        .method("POST")
        .uri("/v1/scenes")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "stepSeconds": 180,
                "layers": [{ "type": "groundtrack", "satelliteId": iss_id }]
            })
            .to_string(),
        ))
        .unwrap();
    let (status, _, body) = call(&app, req).await;
    assert_eq!(status, 400, "Step > 120s: {body}");

    // Case 4: Samples > 2000 per track (24h @ 10s = 8640 samples)
    let req = Request::builder()
        .method("POST")
        .uri("/v1/scenes")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "durationHours": 24.0,
                "stepSeconds": 10,
                "layers": [{ "type": "groundtrack", "satelliteId": iss_id }]
            })
            .to_string(),
        ))
        .unwrap();
    let (status, _, body) = call(&app, req).await;
    assert_eq!(status, 400, "Samples > 2000: {body}");
}

#[tokio::test]
async fn test_scene_missing_satellite_returns_404() {
    let (app, token, _, _) = setup_app().await;
    let missing_id = Uuid::new_v4();

    let req = Request::builder()
        .method("POST")
        .uri("/v1/scenes")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "layers": [{ "type": "groundtrack", "satelliteId": missing_id }]
            })
            .to_string(),
        ))
        .unwrap();
    let (status, _, _) = call(&app, req).await;
    assert_eq!(status, 404);
}
