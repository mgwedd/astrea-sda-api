use astrea_sda_api::{
    create_router,
    models::{Satellite, Tle},
    repository::SatelliteRepository,
    services::astrodynamics,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use uuid::Uuid;

#[test]
fn test_generate_ground_track_direct() {
    let sat = Satellite {
        id: Uuid::new_v4(),
        name: "ATLAS CENTAUR 2".to_string(),
        tle: Tle {
            line_one: "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994"
                .to_string(),
            line_two: "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
                .to_string(),
        },
        created_date: chrono::Utc::now(),
        last_modified_date: chrono::Utc::now(),
    };

    let start_time = chrono::DateTime::parse_from_rfc3339("2021-08-27T12:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let res = astrodynamics::generate_ground_track(
        &sat,
        start_time,
        30,
        60,
        true,
        true,
        &CancellationToken::new(),
    );
    assert!(
        res.is_ok(),
        "Expected groundtrack generation to succeed: {:?}",
        res.err()
    );
    let track = res.unwrap();
    assert_eq!(track.trajectory.len(), 31);
    assert!(track.orbital_period_minutes > 80.0);
    assert!(track.footprint_radius_km > 1000.0);
    assert!(track.geojson.is_some());
    assert!(track.footprint_polygon.is_some());
    assert!(track.czml.is_some());
}

#[tokio::test]
async fn test_groundtrack_endpoint_and_geojson() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    // 0. Login to obtain JWT Bearer Token
    let login_payload = json!({
        "email": "admin@astrea.local",
        "password": "password123"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&login_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let token = auth_json["token"].as_str().unwrap();

    // 1. Create satellite with valid TLE and Authorization header
    let create_payload = json!({
        "name": "ATLAS CENTAUR 2",
        "tleLineOne": "00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "tleLineTwo": "00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(&create_payload).unwrap()))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let sat_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let sat_id = sat_json["id"].as_str().unwrap();

    // 2a. Query Ground Track without authorization -> Expect 401 Unauthorized
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/satellites/{}/groundtrack?duration_minutes=30&step_seconds=60&format=all&start_time=2021-08-27T12:00:00Z",
            sat_id
        ))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // 2b. Query Ground Track endpoint with valid JWT Bearer header (format=all)
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/satellites/{}/groundtrack?duration_minutes=30&step_seconds=60&format=all&start_time=2021-08-27T12:00:00Z",
            sat_id
        ))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = response.into_parts();
    let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    assert_eq!(parts.status, StatusCode::OK);

    let track_json: Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(track_json["satelliteId"], sat_id);
    assert_eq!(track_json["satelliteName"], "ATLAS CENTAUR 2");
    assert!(track_json["orbitalPeriodMinutes"].as_f64().unwrap() > 80.0);
    assert!(track_json["footprintRadiusKm"].as_f64().unwrap() > 1000.0);
    assert_eq!(track_json["durationMinutes"], 30);
    assert_eq!(track_json["stepSeconds"], 60);

    // Verify trajectory points array
    let trajectory = track_json["trajectory"].as_array().unwrap();
    assert_eq!(trajectory.len(), 31);

    let first_pt = &trajectory[0];
    assert!(first_pt["timestamp"].is_string());
    assert!(first_pt["lat"].is_number());
    assert!(first_pt["lon"].is_number());
    assert!(first_pt["altKm"].is_number());
    assert_eq!(first_pt["positionEcfKm"].as_array().unwrap().len(), 3);
    assert_eq!(first_pt["velocityEcfKms"].as_array().unwrap().len(), 3);

    // Verify GeoJSON trajectory feature
    let geojson = &track_json["geojson"];
    assert_eq!(geojson["type"], "Feature");
    assert!(
        geojson["geometry"]["type"] == "LineString"
            || geojson["geometry"]["type"] == "MultiLineString"
    );

    // Verify Footprint Polygon feature
    let footprint_poly = &track_json["footprintPolygon"];
    assert_eq!(footprint_poly["type"], "Feature");
    assert_eq!(footprint_poly["geometry"]["type"], "Polygon");

    // Verify CZML array
    let czml = &track_json["czml"];
    assert!(czml.is_array());
    let czml_arr = czml.as_array().unwrap();
    assert_eq!(czml_arr[0]["id"], "document");
    assert_eq!(czml_arr[1]["id"], sat_id);
    assert!(czml_arr[1]["position"]["cartesian"].is_array());
}
