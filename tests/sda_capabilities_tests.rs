use astrea_sda_api::{
    create_router,
    models::{
        CollisionProbabilityResponse, DecayWatchResponse, PassScheduleResponse,
        RelativeMotionResponse, Satellite, SatelliteDecayRiskResponse, SatelliteStateResponse, Tle,
    },
    repository::SatelliteRepository,
    services::astrodynamics::{
        calculate_decay_risk, calculate_foster_collision_probability, calculate_relative_motion,
        calculate_satellite_state, find_pass_schedule, scan_decay_watch,
    },
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

fn mock_sat(name: &str, line1: &str, line2: &str) -> Satellite {
    Satellite {
        id: Uuid::new_v4(),
        name: name.to_string(),
        tle: Tle {
            line_one: line1.to_string(),
            line_two: line2.to_string(),
        },
        created_date: Utc::now(),
        last_modified_date: Utc::now(),
    }
}

// ISS TLE (epoch 2024 day 83 ~ March 23 2024)
const ISS_LINE1: &str = "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996";
const ISS_LINE2: &str = "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012";

// Tiangong / CSS TLE
const TIANGONG_LINE1: &str =
    "1 48274U 21035A   24083.91428787  .00021345  00000+0  22143-3 0  9998";
const TIANGONG_LINE2: &str =
    "2 48274  41.4728 112.5843 0004928  64.2185 295.9537 15.60271542164426";

// Debris decaying near re-entry (high drag B*, low altitude)
const DEBRIS_DECAY_LINE1: &str =
    "1 99999U 21239A   21239.50000000  .01500000  00000-0  10500-1 0  9997";
const DEBRIS_DECAY_LINE2: &str =
    "2 99999  51.6000 180.0000 0015000  90.0000 270.0000 16.30000000    13";

#[test]
fn test_pass_schedule_calculation() {
    let iss = mock_sat("ISS (ZARYA)", ISS_LINE1, ISS_LINE2);
    // Start at epoch: 2024-03-23 21:31:00 UTC
    let start_time = Utc.with_ymd_and_hms(2024, 3, 23, 21, 31, 0).unwrap();

    // Ground station: Austin, TX (30.2672° N, -97.7431° E)
    let res: PassScheduleResponse = find_pass_schedule(
        &iss, 30.2672, -97.7431, 150.0, start_time, 5.0,   // 5 deg threshold
        7,     // 7 days
        false, // all passes
    )
    .expect("Pass schedule calculation failed");

    assert_eq!(res.satellite_name, "ISS (ZARYA)");
    assert_eq!(res.forecast_days, 7);
    assert!(
        res.passes_found > 0,
        "Expected passes over Austin in 7 days"
    );

    let pass = &res.passes[0];
    assert_eq!(pass.pass_id, 1);
    assert!(pass.max_elevation_deg >= 5.0);
    assert!(pass.duration_seconds > 0.0);
    assert!(pass.tca_range_km > 0.0);
    assert!(pass.aos_time <= pass.tca_time);
    assert!(pass.tca_time <= pass.los_time);
}

#[test]
fn test_relative_motion_lvlh_frame() {
    let iss = mock_sat("ISS (ZARYA)", ISS_LINE1, ISS_LINE2);
    let tiangong = mock_sat("CSS (TIANGONG)", TIANGONG_LINE1, TIANGONG_LINE2);
    let epoch = Utc.with_ymd_and_hms(2024, 3, 24, 0, 0, 0).unwrap();

    let res: RelativeMotionResponse =
        calculate_relative_motion(&iss, &tiangong, epoch, 10, 60).expect("Relative motion failed");

    assert_eq!(res.primary_satellite.name, "ISS (ZARYA)");
    assert_eq!(res.target_satellite.name, "CSS (TIANGONG)");
    assert!(res.relative_distance_km > 0.0);

    // Verify Hill's frame components satisfy Pythagorean theorem
    let reconstructed_dist = (res.radial_distance_km.powi(2)
        + res.in_track_distance_km.powi(2)
        + res.cross_track_distance_km.powi(2))
    .sqrt();
    let diff = (reconstructed_dist - res.relative_distance_km).abs();
    assert!(
        diff < 0.1,
        "Hill components must match relative distance within tolerance"
    );

    assert!(res.trajectory.is_some());
    let traj = res.trajectory.unwrap();
    assert_eq!(traj.len(), 11); // 0 to 10 mins with 1 min step = 11 pts
}

#[test]
fn test_satellite_state_vector_and_keplerian() {
    let iss = mock_sat("ISS (ZARYA)", ISS_LINE1, ISS_LINE2);
    let epoch = Utc.with_ymd_and_hms(2024, 3, 24, 0, 0, 0).unwrap();

    let res: SatelliteStateResponse =
        calculate_satellite_state(&iss, epoch).expect("State calculation failed");

    assert_eq!(res.satellite_name, "ISS (ZARYA)");
    // Semi-major axis for ISS is approximately 6790 - 6800 km
    assert!(
        res.keplerian_elements.semi_major_axis_km > 6700.0
            && res.keplerian_elements.semi_major_axis_km < 6900.0,
        "ISS SMA expected ~6790 km, got {}",
        res.keplerian_elements.semi_major_axis_km
    );

    // Altitude ~ 400 - 430 km
    assert!(
        res.altitude_km > 380.0 && res.altitude_km < 450.0,
        "ISS altitude expected ~415 km, got {}",
        res.altitude_km
    );

    // Inclination ~ 51.6 deg
    assert!(
        (res.keplerian_elements.inclination_deg - 51.64).abs() < 1.0,
        "ISS inclination expected ~51.6 deg, got {}",
        res.keplerian_elements.inclination_deg
    );

    // Orbital period ~ 92 - 93 minutes
    assert!(
        res.keplerian_elements.orbital_period_minutes > 90.0
            && res.keplerian_elements.orbital_period_minutes < 95.0
    );
}

#[test]
fn test_decay_risk_and_decay_watch() {
    let iss = mock_sat("ISS (ZARYA)", ISS_LINE1, ISS_LINE2);
    let debris = mock_sat("DECAYING DEBRIS", DEBRIS_DECAY_LINE1, DEBRIS_DECAY_LINE2);

    let iss_risk: SatelliteDecayRiskResponse =
        calculate_decay_risk(&iss).expect("ISS decay calculation failed");
    assert!(iss_risk.perigee_altitude_km > 380.0);
    assert_eq!(iss_risk.decay_status, "Low Risk / Long-Term LEO Decay");

    let debris_risk: SatelliteDecayRiskResponse =
        calculate_decay_risk(&debris).expect("Debris decay calculation failed");
    // Mean motion 16.3 rev/day -> SMA ~6550 km -> perigee < 180 km
    assert!(debris_risk.perigee_altitude_km < 200.0);
    assert!(debris_risk.reentry_risk_score > 80.0);

    let catalog = vec![iss, debris];
    let watch: DecayWatchResponse = scan_decay_watch(&catalog, 300.0, 0.001, 10);
    assert_eq!(watch.decaying_satellites_found, 1);
    assert_eq!(watch.objects[0].satellite_name, "DECAYING DEBRIS");
}

#[test]
fn test_foster_collision_probability() {
    // Direct head-on close miss: 50 meters miss distance, 50m uncertainty
    let res: CollisionProbabilityResponse =
        calculate_foster_collision_probability(0.05, 14.5, 10.0, 50.0);

    assert!(res.collision_probability > 0.0);
    assert!(res.collision_probability < 1.0);
    assert_eq!(res.hard_body_radius_m, 10.0);
    assert_eq!(res.combined_uncertainty_m, 50.0);
    assert_eq!(res.risk_category, "Critical (Pc >= 1e-4)");

    // Far miss: 10 km miss distance -> negligible collision probability
    let far_res = calculate_foster_collision_probability(10.0, 10.0, 10.0, 50.0);
    assert!(far_res.collision_probability < 1e-10);
    assert_eq!(far_res.risk_category, "Negligible (Pc < 1e-7)");
}

#[tokio::test]
async fn test_sda_api_endpoints_integration() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    // 1. Obtain JWT Token
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
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let auth: Value = serde_json::from_slice(&body).unwrap();
    let token = auth["token"].as_str().unwrap();

    // 2. Create ISS and Tiangong satellites
    let iss_payload = json!({
        "name": "ISS (ZARYA)",
        "tleLineOne": ISS_LINE1,
        "tleLineTwo": ISS_LINE2
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(&iss_payload).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let iss_sat: Value = serde_json::from_slice(&body).unwrap();
    let iss_id = iss_sat["id"].as_str().unwrap();

    let tiangong_payload = json!({
        "name": "CSS (TIANGONG)",
        "tleLineOne": TIANGONG_LINE1,
        "tleLineTwo": TIANGONG_LINE2
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(&tiangong_payload).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let tiangong_sat: Value = serde_json::from_slice(&body).unwrap();
    let tiangong_id = tiangong_sat["id"].as_str().unwrap();

    // 3. Test GET /v1/satellites/{id}/passes
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/satellites/{}/passes?lat=30.2672&lon=-97.7431&threshold_deg=5.0&duration_days=7&start_time=2024-03-23T21:31:00Z",
            iss_id
        ))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let passes_res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(passes_res["satelliteName"], "ISS (ZARYA)");
    assert!(passes_res["passesFound"].as_u64().unwrap() > 0);

    // 4. Test GET /v1/satellites/{id}/relative-motion
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/satellites/{}/relative-motion?target_id={}&time=2024-03-24T00:00:00Z&duration_minutes=5&step_seconds=60",
            iss_id, tiangong_id
        ))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let rel_res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(rel_res["primarySatellite"]["name"], "ISS (ZARYA)");
    assert_eq!(rel_res["targetSatellite"]["name"], "CSS (TIANGONG)");
    assert!(rel_res["relativeDistanceKm"].as_f64().unwrap() > 0.0);

    // 5. Test GET /v1/satellites/{id}/state
    let req = Request::builder()
        .method("GET")
        .uri(format!(
            "/v1/satellites/{}/state?epoch=2024-03-24T00:00:00Z",
            iss_id
        ))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let state_res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(state_res["satelliteName"], "ISS (ZARYA)");
    assert!(state_res["altitudeKm"].as_f64().unwrap() > 300.0);
    assert!(
        state_res["keplerianElements"]["semiMajorAxisKm"]
            .as_f64()
            .unwrap()
            > 6000.0
    );

    // 6. Test GET /v1/satellites/{id}/decay-risk
    let req = Request::builder()
        .method("GET")
        .uri(format!("/v1/satellites/{}/decay-risk", iss_id))
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let decay_res: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(decay_res["satelliteName"], "ISS (ZARYA)");
    assert!(decay_res["perigeeAltitudeKm"].as_f64().unwrap() > 350.0);

    // 7. Test GET /v1/satellites/decay-watch
    let req = Request::builder()
        .method("GET")
        .uri("/v1/satellites/decay-watch?max_perigee_km=500.0&limit=10")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let watch_res: Value = serde_json::from_slice(&body).unwrap();
    assert!(watch_res["scannedSatellitesCount"].as_u64().unwrap() >= 2);

    // 8. Test POST /v1/conjunctions/collision-probability
    let pc_req_body = json!({
        "missDistanceKm": 0.04,
        "relativeVelocityKms": 12.0,
        "hardBodyRadiusM": 15.0,
        "combinedPositionUncertaintyM": 40.0
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/conjunctions/collision-probability")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(&pc_req_body).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let pc_res: Value = serde_json::from_slice(&body).unwrap();
    assert!(pc_res["collisionProbability"].as_f64().unwrap() > 0.0);
    assert_eq!(pc_res["riskCategory"], "Critical (Pc >= 1e-4)");
}
