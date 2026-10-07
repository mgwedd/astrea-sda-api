//! CZML encoding checks. Physics truth (TEME→ECEF) is covered by
//! transforms_reference_tests against an ERFA oracle; these tests cover what the
//! CZML layer adds: units/epoch encoding, and the error of the degree-5 Lagrange
//! interpolation Cesium is told to use, vs 1 s samples of the same ephemeris.
use tokio_util::sync::CancellationToken;

use astrea_sda_api::models::{Satellite, Tle};
use astrea_sda_api::services::astrodynamics::generate_ground_track;
use chrono::{TimeZone, Utc};

fn iss() -> Satellite {
    Satellite {
        id: uuid::Uuid::nil(),
        name: "ISS".into(),
        tle: Tle {
            line_one: "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996"
                .into(),
            line_two: "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012"
                .into(),
        },
        created_date: Utc::now(),
        last_modified_date: Utc::now(),
    }
}

/// Textbook Lagrange interpolation over `nodes` (t, xyz), independent of the service.
fn lagrange(nodes: &[(f64, [f64; 3])], t: f64) -> [f64; 3] {
    let mut out = [0.0; 3];
    for (j, (tj, pj)) in nodes.iter().enumerate() {
        let w: f64 = nodes
            .iter()
            .enumerate()
            .filter(|(m, _)| *m != j)
            .map(|(_, (tm, _))| (t - tm) / (tj - tm))
            .product();
        for k in 0..3 {
            out[k] += w * pj[k];
        }
    }
    out
}

fn max_err_m(step: usize) -> f64 {
    let t0 = Utc.with_ymd_and_hms(2024, 3, 25, 12, 0, 0).unwrap();
    let sat = iss();
    let truth = generate_ground_track(&sat, t0, 30, 1, false, false, &CancellationToken::new())
        .unwrap()
        .trajectory;
    let coarse =
        generate_ground_track(&sat, t0, 30, step, false, true, &CancellationToken::new()).unwrap();
    let cart = coarse.czml.unwrap()[1]["position"]["cartesian"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect::<Vec<_>>();
    let nodes: Vec<(f64, [f64; 3])> = cart.chunks(4).map(|c| (c[0], [c[1], c[2], c[3]])).collect();
    let mut worst: f64 = 0.0;
    // Interior only: skip the first/last 3 nodes, where a centred window does not exist.
    for (i, p) in truth.iter().enumerate() {
        let t = i as f64;
        let k = (t / step as f64) as usize;
        if k < 3 || k + 3 >= nodes.len() {
            continue;
        }
        let est = lagrange(&nodes[k - 2..=k + 3], t);
        let e = (0..3)
            .map(|a| (est[a] - p.position_ecf_km[a] * 1000.0).powi(2))
            .sum::<f64>()
            .sqrt();
        worst = worst.max(e);
    }
    worst
}

#[test]
fn encodes_metres_and_second_offsets_from_epoch() {
    let t0 = Utc.with_ymd_and_hms(2024, 3, 25, 12, 0, 0).unwrap();
    let r =
        generate_ground_track(&iss(), t0, 10, 30, false, true, &CancellationToken::new()).unwrap();
    let doc = r.czml.unwrap();
    assert_eq!(doc[0]["clock"]["currentTime"], "2024-03-25T12:00:00.000Z");
    let pos = &doc[1]["position"];
    assert_eq!(pos["epoch"], "2024-03-25T12:00:00.000Z");
    let cart = pos["cartesian"].as_array().unwrap();
    for (i, p) in r.trajectory.iter().enumerate() {
        assert_eq!(cart[4 * i].as_f64().unwrap(), (i * 30) as f64);
        for a in 0..3 {
            assert_eq!(
                cart[4 * i + 1 + a].as_f64().unwrap(),
                p.position_ecf_km[a] * 1000.0
            );
        }
    }
}

#[test]
fn lagrange5_stays_under_one_metre_up_to_120s_for_leo() {
    for step in [30, 60, 120] {
        let e = max_err_m(step);
        assert!(e < 1.0, "step {step}s: {e} m");
    }
}

mod http {
    use astrea_sda_api::{create_router, repository::SatelliteRepository};
    use axum::{body::Body, http::Request};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    async fn call(app: &axum::Router, req: Request<Body>) -> (u16, Value) {
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status().as_u16();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn setup() -> (axum::Router, String, String) {
        let app = create_router(SatelliteRepository::new(None).await);
        let login = Request::builder()
            .method("POST")
            .uri("/v1/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"email": "admin@astrea.local", "password": "password123"}).to_string(),
            ))
            .unwrap();
        let token = call(&app, login).await.1["token"]
            .as_str()
            .unwrap()
            .to_string();
        let create = Request::builder()
            .method("POST")
            .uri("/v1/satellites")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(
                json!({
                    "name": "ISS",
                    "tleLineOne": "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996",
                    "tleLineTwo": "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012"
                })
                .to_string(),
            ))
            .unwrap();
        let id = call(&app, create).await.1["id"]
            .as_str()
            .unwrap()
            .to_string();
        (app, token, id)
    }

    async fn get(app: &axum::Router, token: &str, uri: String) -> (u16, Value) {
        let req = Request::builder()
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        call(app, req).await
    }

    #[tokio::test]
    async fn over_cap_requests_are_400_not_500() {
        let (app, t, id) = setup().await;
        let base = format!("/v1/satellites/{id}/groundtrack?start_time=2024-03-25T12:00:00Z");
        for q in [
            "duration_minutes=1500&step_seconds=60",            // > 24 h
            "duration_minutes=60&step_seconds=1",               // 3601 samples
            "duration_minutes=30&step_seconds=300&format=czml", // step > 120 s for czml
        ] {
            let (status, body) = get(&app, &t, format!("{base}&{q}")).await;
            assert_eq!(status, 400, "{q}: {body}");
        }
    }

    #[tokio::test]
    async fn groundtrack_and_passes_czml_through_engine() {
        let (app, t, id) = setup().await;
        let (status, body) = get(
            &app,
            &t,
            format!("/v1/satellites/{id}/groundtrack?duration_minutes=30&step_seconds=60&format=czml&start_time=2024-03-25T12:00:00Z"),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["droppedSamples"], 0);
        assert_eq!(body["czml"][1]["position"]["referenceFrame"], "FIXED");

        let (status, body) = get(
            &app,
            &t,
            format!("/v1/satellites/{id}/passes?lat=40&lon=-75&duration_days=1&format=czml&start_time=2024-03-25T00:00:00Z"),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        let n = body["passes"].as_array().unwrap().len();
        assert!(n > 0);
        assert_eq!(body["czml"].as_array().unwrap().len(), n + 1);
        assert_eq!(body["czmlDroppedSamples"], 0);
    }
}
