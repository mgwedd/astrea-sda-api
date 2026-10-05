//! CZML encoding checks. Physics truth (TEME→ECEF) is covered by
//! transforms_reference_tests against an ERFA oracle; these tests cover what the
//! CZML layer adds: units/epoch encoding, and the error of the degree-5 Lagrange
//! interpolation Cesium is told to use, vs 1 s samples of the same ephemeris.

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
    let truth = generate_ground_track(&sat, t0, 30, 1, false, false)
        .unwrap()
        .trajectory;
    let coarse = generate_ground_track(&sat, t0, 30, step, false, true).unwrap();
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
    let r = generate_ground_track(&iss(), t0, 10, 30, false, true).unwrap();
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
