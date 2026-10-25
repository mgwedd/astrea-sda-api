use astrea_sda_api::{
    create_router,
    models::{HorizonMaskPoint, Satellite, SensorDefinition, Tle},
    repository::SatelliteRepository,
    services::sensor::{
        calculate_tracking_rates, find_sensor_passes, interpolate_horizon_elevation,
        schedule_sensor_network,
    },
};
use axum::{body::Body, http::Request};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use uuid::Uuid;

fn sample_iss() -> Satellite {
    Satellite {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        name: "ISS (ZARYA)".to_string(),
        tle: Tle {
            line_one: "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996"
                .to_string(),
            line_two: "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012"
                .to_string(),
        },
        created_date: Utc::now(),
        last_modified_date: Utc::now(),
    }
}

async fn call_router(app: &axum::Router, req: Request<Body>) -> (u16, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let body = axum::body::to_bytes(res.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap();
    let json_val = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, json_val)
}

#[test]
fn test_interpolate_horizon_elevation() {
    // 1. Empty mask returns 0.0
    assert_eq!(interpolate_horizon_elevation(&[], 120.0), 0.0);

    // 2. Single-point mask returns constant elevation
    let single = vec![HorizonMaskPoint {
        azimuth_deg: 90.0,
        elevation_deg: 22.5,
    }];
    assert_eq!(interpolate_horizon_elevation(&single, 0.0), 22.5);
    assert_eq!(interpolate_horizon_elevation(&single, 270.0), 22.5);

    // 3. Multi-point profile: cardinal directions
    let mask = vec![
        HorizonMaskPoint {
            azimuth_deg: 0.0,
            elevation_deg: 10.0,
        },
        HorizonMaskPoint {
            azimuth_deg: 90.0,
            elevation_deg: 20.0,
        },
        HorizonMaskPoint {
            azimuth_deg: 180.0,
            elevation_deg: 30.0,
        },
        HorizonMaskPoint {
            azimuth_deg: 270.0,
            elevation_deg: 15.0,
        },
    ];

    // Exact matches
    assert!((interpolate_horizon_elevation(&mask, 0.0) - 10.0).abs() < 1e-6);
    assert!((interpolate_horizon_elevation(&mask, 90.0) - 20.0).abs() < 1e-6);
    assert!((interpolate_horizon_elevation(&mask, 180.0) - 30.0).abs() < 1e-6);
    assert!((interpolate_horizon_elevation(&mask, 270.0) - 15.0).abs() < 1e-6);

    // Midpoints
    assert!((interpolate_horizon_elevation(&mask, 45.0) - 15.0).abs() < 1e-6);
    assert!((interpolate_horizon_elevation(&mask, 135.0) - 25.0).abs() < 1e-6);
    assert!((interpolate_horizon_elevation(&mask, 225.0) - 22.5).abs() < 1e-6);

    // Circular wrap-around across 360/0 deg (between 270 deg: 15.0 and 0/360 deg: 10.0)
    // At 315 deg, midpoint of [270, 360] -> (15.0 + 10.0) / 2 = 12.5
    assert!((interpolate_horizon_elevation(&mask, 315.0) - 12.5).abs() < 1e-6);

    // Azimuth modulo normalization
    assert!((interpolate_horizon_elevation(&mask, 405.0) - 15.0).abs() < 1e-6);
    assert!((interpolate_horizon_elevation(&mask, -45.0) - 12.5).abs() < 1e-6);
}

#[test]
fn test_analytical_tracking_rates_against_numerical_oracle() {
    // Topocentric SEZ position (km) and velocity (km/s)
    let pos_sez = [1200.0, -850.0, 450.0];
    let vel_sez = [-3.5, 4.2, 1.8];

    let rates = calculate_tracking_rates(pos_sez, vel_sez);

    // Finite difference numerical oracle with small dt
    let dt = 1e-4_f64;
    let pos_plus = [
        pos_sez[0] + vel_sez[0] * dt,
        pos_sez[1] + vel_sez[1] * dt,
        pos_sez[2] + vel_sez[2] * dt,
    ];
    let pos_minus = [
        pos_sez[0] - vel_sez[0] * dt,
        pos_sez[1] - vel_sez[1] * dt,
        pos_sez[2] - vel_sez[2] * dt,
    ];

    let range_plus = (pos_plus[0].powi(2) + pos_plus[1].powi(2) + pos_plus[2].powi(2)).sqrt();
    let range_minus = (pos_minus[0].powi(2) + pos_minus[1].powi(2) + pos_minus[2].powi(2)).sqrt();
    let num_range_rate = (range_plus - range_minus) / (2.0 * dt);

    let az_plus = pos_plus[1]
        .atan2(-pos_plus[0])
        .to_degrees()
        .rem_euclid(360.0);
    let az_minus = pos_minus[1]
        .atan2(-pos_minus[0])
        .to_degrees()
        .rem_euclid(360.0);
    let mut d_az = az_plus - az_minus;
    if d_az > 180.0 {
        d_az -= 360.0;
    } else if d_az < -180.0 {
        d_az += 360.0;
    }
    let num_az_rate = d_az / (2.0 * dt);

    let el_plus = (pos_plus[2] / range_plus).asin().to_degrees();
    let el_minus = (pos_minus[2] / range_minus).asin().to_degrees();
    let num_el_rate = (el_plus - el_minus) / (2.0 * dt);

    // Verify analytical tracking rates match numerical differentiation oracle
    assert!(
        (rates.range_rate_kms - num_range_rate).abs() < 1e-5,
        "Range rate mismatch: analytical={}, numerical={}",
        rates.range_rate_kms,
        num_range_rate
    );
    assert!(
        (rates.azimuth_rate_deg_s - num_az_rate).abs() < 1e-3,
        "Azimuth rate mismatch: analytical={}, numerical={}",
        rates.azimuth_rate_deg_s,
        num_az_rate
    );
    assert!(
        (rates.elevation_rate_deg_s - num_el_rate).abs() < 1e-3,
        "Elevation rate mismatch: analytical={}, numerical={}",
        rates.elevation_rate_deg_s,
        num_el_rate
    );
}

#[test]
fn test_zenith_keyhole_handling() {
    // Near-zenith keyhole: horizontal distance ~ 0
    let pos_zenith = [1e-9, 1e-9, 600.0];
    let vel_horizontal = [7.5, 0.0, 0.0];

    let rates = calculate_tracking_rates(pos_zenith, vel_horizontal);

    // Azimuth rate should be safely handled (0.0 guard) without NaN or panic
    assert!(rates.azimuth_rate_deg_s.is_finite());
    assert!(rates.elevation_rate_deg_s.is_finite());
    assert!(rates.range_rate_kms.is_finite());
}

#[test]
fn test_single_sensor_pass_evaluation() {
    let sat = sample_iss();
    let start_time = Utc.with_ymd_and_hms(2024, 3, 24, 0, 0, 0).unwrap();
    let cancel = CancellationToken::new();

    let sensor = SensorDefinition {
        id: Some(Uuid::new_v4()),
        name: "Austin Tracking Station".to_string(),
        lat: 30.2672,
        lon: -97.7431,
        alt_m: 150.0,
        min_elevation_deg: Some(15.0),
        max_elevation_deg: Some(85.0),
        max_azimuth_rate_deg_s: Some(20.0),
        max_elevation_rate_deg_s: Some(10.0),
        horizon_mask: None,
        optical_only: None,
    };

    let passes = find_sensor_passes(&sat, &sensor, start_time, 2.0, &cancel).unwrap();

    assert!(
        !passes.is_empty(),
        "Expected passes for ISS over Austin across 2 days"
    );
    for p in &passes {
        assert_eq!(p.sensor_name, "Austin Tracking Station");
        assert!(p.duration_seconds >= 10.0);
        assert!(p.max_elevation_deg >= 15.0);
        assert!(p.peak_azimuth_rate_deg_s >= 0.0);
        assert!(p.peak_elevation_rate_deg_s >= 0.0);
        assert!(p.is_mount_feasible);
    }

    // Now test with strict mount slew limits (e.g. 0.01 deg/s) to verify infeasibility detection
    let mut restricted_sensor = sensor.clone();
    restricted_sensor.max_azimuth_rate_deg_s = Some(0.01);

    let restricted_passes =
        find_sensor_passes(&sat, &restricted_sensor, start_time, 2.0, &cancel).unwrap();
    assert!(!restricted_passes.is_empty());
    for p in &restricted_passes {
        assert!(
            !p.is_mount_feasible,
            "Pass should violate mount azimuth slew limits"
        );
    }
}

#[test]
fn test_terrain_horizon_masking_clips_passes() {
    let sat = sample_iss();
    let start_time = Utc.with_ymd_and_hms(2024, 3, 24, 0, 0, 0).unwrap();
    let cancel = CancellationToken::new();

    // Sensor A: Flat 10-degree horizon
    let sensor_flat = SensorDefinition {
        id: Some(Uuid::new_v4()),
        name: "Flat Horizon".to_string(),
        lat: 30.2672,
        lon: -97.7431,
        alt_m: 150.0,
        min_elevation_deg: Some(10.0),
        max_elevation_deg: None,
        max_azimuth_rate_deg_s: None,
        max_elevation_rate_deg_s: None,
        horizon_mask: None,
        optical_only: None,
    };

    // Sensor B: Mountain obstacle reaching 50 deg elevation between East and South
    let sensor_mountain = SensorDefinition {
        id: Some(Uuid::new_v4()),
        name: "Mountain Horizon".to_string(),
        lat: 30.2672,
        lon: -97.7431,
        alt_m: 150.0,
        min_elevation_deg: Some(10.0),
        max_elevation_deg: None,
        max_azimuth_rate_deg_s: None,
        max_elevation_rate_deg_s: None,
        horizon_mask: Some(vec![
            HorizonMaskPoint {
                azimuth_deg: 0.0,
                elevation_deg: 10.0,
            },
            HorizonMaskPoint {
                azimuth_deg: 60.0,
                elevation_deg: 50.0,
            },
            HorizonMaskPoint {
                azimuth_deg: 150.0,
                elevation_deg: 50.0,
            },
            HorizonMaskPoint {
                azimuth_deg: 210.0,
                elevation_deg: 10.0,
            },
            HorizonMaskPoint {
                azimuth_deg: 360.0,
                elevation_deg: 10.0,
            },
        ]),
        optical_only: None,
    };

    let passes_flat = find_sensor_passes(&sat, &sensor_flat, start_time, 2.0, &cancel).unwrap();
    let passes_mountain =
        find_sensor_passes(&sat, &sensor_mountain, start_time, 2.0, &cancel).unwrap();

    let total_duration_flat: f64 = passes_flat.iter().map(|p| p.duration_seconds).sum();
    let total_duration_mountain: f64 = passes_mountain.iter().map(|p| p.duration_seconds).sum();

    // Mountain mask must strictly reduce tracking time compared to flat horizon
    assert!(
        total_duration_mountain < total_duration_flat,
        "Terrain mask should clip visible tracking duration: mountain={}s, flat={}s",
        total_duration_mountain,
        total_duration_flat
    );
}

#[test]
fn test_multi_station_network_schedule_and_handovers() {
    let sat = sample_iss();
    let start_time = Utc.with_ymd_and_hms(2024, 3, 24, 0, 0, 0).unwrap();
    let cancel = CancellationToken::new();

    let sensors = vec![
        SensorDefinition {
            id: Some(Uuid::new_v4()),
            name: "Austin Station".to_string(),
            lat: 30.2672,
            lon: -97.7431,
            alt_m: 150.0,
            min_elevation_deg: Some(10.0),
            max_elevation_deg: None,
            max_azimuth_rate_deg_s: None,
            max_elevation_rate_deg_s: None,
            horizon_mask: None,
            optical_only: None,
        },
        SensorDefinition {
            id: Some(Uuid::new_v4()),
            name: "Madrid Station".to_string(),
            lat: 40.4168,
            lon: -3.7038,
            alt_m: 650.0,
            min_elevation_deg: Some(10.0),
            max_elevation_deg: None,
            max_azimuth_rate_deg_s: None,
            max_elevation_rate_deg_s: None,
            horizon_mask: None,
            optical_only: None,
        },
    ];

    let schedule = schedule_sensor_network(&[sat], &sensors, start_time, 3.0, &cancel).unwrap();

    assert!(schedule.total_passes > 0);
    assert_eq!(schedule.passes.len(), schedule.total_passes);

    // Verify chronological order of passes
    for i in 0..schedule.passes.len().saturating_sub(1) {
        assert!(schedule.passes[i].aos_time <= schedule.passes[i + 1].aos_time);
        assert_eq!(schedule.passes[i].pass_id, i + 1);
    }

    // Verify handovers between Austin and Madrid stations
    assert!(
        !schedule.handovers.is_empty(),
        "Expected inter-station handovers across 3 days"
    );
    for h in &schedule.handovers {
        assert_ne!(h.from_sensor_id, h.to_sensor_id);
        assert_eq!(h.satellite_name, "ISS (ZARYA)");
    }
}

#[tokio::test]
async fn test_api_sensor_endpoints_integration() {
    let repo = SatelliteRepository::new(None).await;
    let app = create_router(repo);

    // 1. Login to obtain viewer JWT
    let login_req = Request::builder()
        .method("POST")
        .uri("/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email": "admin@astrea.local", "password": "password123"}).to_string(),
        ))
        .unwrap();
    let (status, login_res) = call_router(&app, login_req).await;
    assert_eq!(status, 200);
    let token = login_res["token"].as_str().unwrap().to_string();

    // 2. Create Satellite for testing
    let sat_req = Request::builder()
        .method("POST")
        .uri("/v1/satellites")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "name": "TEST-ISS",
                "tleLineOne": "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996",
                "tleLineTwo": "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012"
            })
            .to_string(),
        ))
        .unwrap();
    let (status, sat_res) = call_router(&app, sat_req).await;
    assert_eq!(status, 201);
    let sat_id = sat_res["id"].as_str().unwrap();

    // 3. Test POST /v1/sensors/schedule
    let sched_req = Request::builder()
        .method("POST")
        .uri("/v1/sensors/schedule")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "sensors": [
                    {
                        "name": "Austin Node",
                        "lat": 30.2672,
                        "lon": -97.7431,
                        "altM": 150.0,
                        "minElevationDeg": 10.0
                    },
                    {
                        "name": "Madrid Node",
                        "lat": 40.4168,
                        "lon": -3.7038,
                        "altM": 650.0,
                        "minElevationDeg": 10.0
                    }
                ],
                "satelliteIds": [sat_id],
                "durationDays": 1.0
            })
            .to_string(),
        ))
        .unwrap();

    let (status, sched_res) = call_router(&app, sched_req).await;
    assert_eq!(status, 200, "Schedule response failed: {:?}", sched_res);
    assert!(sched_res["totalPasses"].as_u64().unwrap() > 0);
    assert!(!sched_res["passes"].as_array().unwrap().is_empty());

    // 4. Test POST /v1/sensors/passes
    let single_pass_req = Request::builder()
        .method("POST")
        .uri("/v1/sensors/passes")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "sensor": {
                    "name": "Austin Single",
                    "lat": 30.2672,
                    "lon": -97.7431,
                    "altM": 150.0,
                    "minElevationDeg": 15.0
                },
                "satelliteId": sat_id,
                "durationDays": 1.0
            })
            .to_string(),
        ))
        .unwrap();

    let (status, passes_res) = call_router(&app, single_pass_req).await;
    assert_eq!(status, 200, "Single pass response failed: {:?}", passes_res);
    assert!(passes_res["totalPasses"].as_u64().unwrap() > 0);

    // 5. Test validation error: empty sensors list
    let bad_req = Request::builder()
        .method("POST")
        .uri("/v1/sensors/schedule")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "sensors": [],
                "satelliteIds": [sat_id],
                "durationDays": 1.0
            })
            .to_string(),
        ))
        .unwrap();
    let (status, _) = call_router(&app, bad_req).await;
    assert_eq!(status, 400);

    // 6. Test validation error: invalid sensor latitude
    let bad_lat_req = Request::builder()
        .method("POST")
        .uri("/v1/sensors/schedule")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(
            json!({
                "sensors": [
                    {
                        "name": "Invalid Lat",
                        "lat": 95.0,
                        "lon": 0.0,
                        "altM": 0.0
                    }
                ],
                "satelliteIds": [sat_id],
                "durationDays": 1.0
            })
            .to_string(),
        ))
        .unwrap();
    let (status, _) = call_router(&app, bad_lat_req).await;
    assert_eq!(status, 400);
}
