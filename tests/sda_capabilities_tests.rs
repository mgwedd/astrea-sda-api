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
    let watch: DecayWatchResponse = scan_decay_watch(&catalog, 300.0, 10);
    assert_eq!(watch.decaying_satellites_found, 1);
    assert_eq!(watch.objects[0].satellite_name, "DECAYING DEBRIS");
}

#[test]
fn test_foster_collision_probability() {
    // 50 m miss, isotropic 50 m sigma, 10 m HBR
    let res: CollisionProbabilityResponse =
        calculate_foster_collision_probability(0.05, 14.5, 10.0, 50.0, 50.0, 0.0);
    assert_eq!(res.hard_body_radius_m, 10.0);
    assert_eq!((res.sigma_1_m, res.sigma_2_m), (50.0, 50.0));
    assert_eq!(res.risk_category, "Critical (Pc >= 1e-4)");

    // Far miss: 10 km -> negligible
    let far = calculate_foster_collision_probability(10.0, 10.0, 10.0, 50.0, 50.0, 0.0);
    assert!(far.collision_probability < 1e-10);
    assert_eq!(far.risk_category, "Negligible (Pc < 1e-7)");
}

// Oracle: isotropic direct hit has the exact closed form 1 - exp(-R^2 / 2 sigma^2).
#[test]
fn foster_pc_isotropic_direct_hit_matches_closed_form() {
    let r = calculate_foster_collision_probability(0.0, 7.0, 10.0, 50.0, 50.0, 0.0);
    let exact = 1.0 - (-(10.0f64 * 10.0) / (2.0 * 50.0 * 50.0)).exp();
    assert!((r.collision_probability - exact).abs() < 1e-9 * exact.max(1.0));
}

// Oracle: for R << sigma_min the constant-density Akella-Alfriend form (spec 10.5) holds.
#[test]
fn foster_pc_anisotropic_matches_constant_density_form() {
    let (r, s1, s2) = (1.0f64, 500.0f64, 50.0f64);
    let (d, alpha) = (80.0f64, 35.0f64.to_radians());
    let (m1, m2) = (d * alpha.cos(), d * alpha.sin());
    let expected =
        r * r / (2.0 * s1 * s2) * (-0.5 * (m1 * m1 / (s1 * s1) + m2 * m2 / (s2 * s2))).exp();
    let got = calculate_foster_collision_probability(d / 1000.0, 7.0, r, s1, s2, 35.0)
        .collision_probability;
    assert!(
        ((got - expected) / expected).abs() < 5e-3,
        "{got} vs {expected}"
    );
}

// Circularizing the covariance must change the answer materially for anisotropic input.
#[test]
fn foster_pc_is_not_circularized() {
    let aniso = calculate_foster_collision_probability(0.2, 7.0, 10.0, 500.0, 20.0, 90.0);
    let sigma_iso = ((500.0f64 * 500.0 + 20.0 * 20.0) / 2.0).sqrt();
    let iso = calculate_foster_collision_probability(0.2, 7.0, 10.0, sigma_iso, sigma_iso, 90.0);
    let ratio = aniso.collision_probability / iso.collision_probability;
    assert!(ratio > 2.0 || ratio < 0.5, "ratio {ratio}");
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

    // 7. Test GET /v1/decay-watch
    let req = Request::builder()
        .method("GET")
        .uri("/v1/decay-watch?max_perigee_km=500.0&limit=10")
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
        "sigma1M": 40.0,
        "sigma2M": 40.0
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

/// Independent WGS-84 radius at geocentric latitude (literals, not crate helpers).
fn wgs84_radius_km(phi_gc_rad: f64) -> f64 {
    let (a, b) = (6378.137_f64, 6_356.752_314_245_f64);
    let (s, c) = phi_gc_rad.sin_cos();
    a * b / (b * b * c * c + a * a * s * s).sqrt()
}

// ISS: i = 51.64, w ~ 98.8 -> apsides near 50.8 deg geocentric latitude, ~10 km below R_E.
#[test]
fn state_apsis_altitudes_use_radius_at_apsis_latitude() {
    let iss = mock_sat("ISS (ZARYA)", ISS_LINE1, ISS_LINE2);
    let t = Utc.with_ymd_and_hms(2024, 3, 23, 21, 31, 0).unwrap();
    let st = calculate_satellite_state(&iss, t).unwrap();
    let k = &st.keplerian_elements;
    let phi =
        (k.inclination_deg.to_radians().sin() * k.arg_of_perigee_deg.to_radians().sin()).asin();
    let r = wgs84_radius_km(phi);
    assert!(6378.137 - r > 5.0, "test must discriminate from R_E: {r}");
    let hp = k.semi_major_axis_km * (1.0 - k.eccentricity) - r;
    let ha = k.semi_major_axis_km * (1.0 + k.eccentricity) - r;
    // outputs are rounded to 0.01 km and inputs to ~1e-4, so allow 0.05 km
    assert!(
        (k.perigee_altitude_km - hp).abs() < 0.05,
        "{} vs {hp}",
        k.perigee_altitude_km
    );
    assert!(
        (k.apogee_altitude_km - ha).abs() < 0.05,
        "{} vs {ha}",
        k.apogee_altitude_km
    );
}

#[test]
fn decay_risk_perigee_uses_radius_at_apsis_latitude() {
    use astrea_sda_api::services::astrodynamics::{
        brouwer_mean_semi_major_axis_km, normalize_tle_line,
    };
    use sgp4::Elements;
    let iss = mock_sat("ISS (ZARYA)", ISS_LINE1, ISS_LINE2);
    let l1 = normalize_tle_line(&iss.tle.line_one, '1');
    let l2 = normalize_tle_line(&iss.tle.line_two, '2');
    let el = Elements::from_tle(None, l1.as_bytes(), l2.as_bytes()).unwrap();
    let a = brouwer_mean_semi_major_axis_km(&el);
    let phi =
        (el.inclination.to_radians().sin() * el.argument_of_perigee.to_radians().sin()).asin();
    let r = wgs84_radius_km(phi);
    let res = calculate_decay_risk(&iss).unwrap();
    assert!((res.perigee_altitude_km - (a * (1.0 - el.eccentricity) - r)).abs() < 0.06);
    assert!((res.apogee_altitude_km - (a * (1.0 + el.eccentricity) - r)).abs() < 0.06);
}
