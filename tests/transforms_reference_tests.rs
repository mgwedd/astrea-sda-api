//! Accuracy tests for the element and frame transforms against independently generated
//! reference values (`tests/reference/gen_transform_reference.py`, numpy + ERFA).
//! The generator shares no code with the implementation; regenerate the JSON with
//! `python3 tests/reference/gen_transform_reference.py > tests/reference/transform_reference.json`.

use astrea_sda_api::models::{
    CartesianState, CoordinateFrame, ElementTransformRequest, EquinoctialInput,
    FrameTransformRequest, KeplerianInput,
};
use astrea_sda_api::services::astrodynamics::{
    solve_kepler_equation, transform_coordinate_frame, transform_orbital_elements,
};
use chrono::{DateTime, Utc};
use serde_json::Value;

fn reference() -> Value {
    serde_json::from_str(include_str!("reference/transform_reference.json")).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

fn vec3(v: &Value) -> [f64; 3] {
    [f(&v[0]), f(&v[1]), f(&v[2])]
}

fn ang_diff_deg(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn assert_vec_close(name: &str, got: [f64; 3], want: [f64; 3], tol: f64) {
    for j in 0..3 {
        assert!(
            (got[j] - want[j]).abs() <= tol,
            "{name}[{j}]: got {} want {} (tol {tol})",
            got[j],
            want[j]
        );
    }
}

/// Check one transform response against the reference MEE set, Cartesian state and derived a/e/i.
fn assert_matches_reference(case: &Value, got: &astrea_sda_api::models::ElementTransformResponse) {
    let name = case["name"].as_str().unwrap();
    let eq = &got.equinoctial;
    assert!((eq.p_km - f(&case["p"])).abs() < 1e-6, "{name} p");
    for (label, got_v, key) in [
        ("f", eq.f, "f"),
        ("g", eq.g, "g"),
        ("h", eq.h, "h"),
        ("k", eq.k, "k"),
    ] {
        assert!(
            (got_v - f(&case[key])).abs() < 1e-9,
            "{name} {label}: {got_v} vs {}",
            case[key]
        );
    }
    assert!(
        ang_diff_deg(eq.true_longitude_deg, f(&case["L"])) < 1e-6,
        "{name} L"
    );
    assert!(
        ang_diff_deg(eq.mean_longitude_deg, f(&case["lam"])) < 1e-6,
        "{name} mean lon"
    );
    assert_eq!(
        eq.retrograde_factor as i64,
        case["I"].as_i64().unwrap(),
        "{name} I"
    );
    assert!(
        (got.keplerian.semi_major_axis_km - f(&case["a"])).abs() < 1e-6,
        "{name} a"
    );
    assert!(
        (got.keplerian.eccentricity - f(&case["e"])).abs() < 1e-9,
        "{name} e"
    );
    assert!(
        (got.keplerian.inclination_deg - f(&case["inc"])).abs() < 1e-6,
        "{name} inc"
    );
    assert_vec_close(
        &format!("{name} r"),
        got.cartesian.position_km,
        vec3(&case["r"]),
        1e-6,
    );
    assert_vec_close(
        &format!("{name} v"),
        got.cartesian.velocity_kms,
        vec3(&case["v"]),
        1e-9,
    );
}

#[test]
fn cartesian_input_matches_independent_equinoctial_reference() {
    for case in reference()["elements"].as_array().unwrap() {
        let req = ElementTransformRequest {
            cartesian: Some(CartesianState {
                position_km: vec3(&case["r"]),
                velocity_kms: vec3(&case["v"]),
            }),
            keplerian: None,
            equinoctial: None,
        };
        assert_matches_reference(case, &transform_orbital_elements(&req).unwrap());
    }
}

#[test]
fn keplerian_input_matches_independent_reference() {
    for case in reference()["elements"].as_array().unwrap() {
        let k = &case["kep"];
        let req = ElementTransformRequest {
            cartesian: None,
            keplerian: Some(KeplerianInput {
                semi_major_axis_km: f(&k[0]),
                eccentricity: f(&k[1]),
                inclination_deg: f(&k[2]),
                raan_deg: f(&k[3]),
                arg_of_perigee_deg: f(&k[4]),
                true_anomaly_deg: Some(f(&k[5])),
                mean_anomaly_deg: None,
            }),
            equinoctial: None,
        };
        assert_matches_reference(case, &transform_orbital_elements(&req).unwrap());
    }
}

#[test]
fn equinoctial_input_true_and_mean_longitude_reproduce_reference_state() {
    for case in reference()["elements"].as_array().unwrap() {
        for use_mean in [false, true] {
            let req = ElementTransformRequest {
                cartesian: None,
                keplerian: None,
                equinoctial: Some(EquinoctialInput {
                    p_km: f(&case["p"]),
                    f: f(&case["f"]),
                    g: f(&case["g"]),
                    h: f(&case["h"]),
                    k: f(&case["k"]),
                    true_longitude_deg: (!use_mean).then(|| f(&case["L"])),
                    mean_longitude_deg: use_mean.then(|| f(&case["lam"])),
                    retrograde_factor: Some(case["I"].as_i64().unwrap() as i32),
                }),
            };
            let got = transform_orbital_elements(&req).unwrap();
            let name = case["name"].as_str().unwrap();
            assert_vec_close(
                &format!("{name} r mean={use_mean}"),
                got.cartesian.position_km,
                vec3(&case["r"]),
                1e-5,
            );
            assert_vec_close(
                &format!("{name} v mean={use_mean}"),
                got.cartesian.velocity_kms,
                vec3(&case["v"]),
                1e-8,
            );
            assert!(
                (got.keplerian.inclination_deg - f(&case["inc"])).abs() < 1e-6,
                "{name} inc"
            );
        }
    }
}

#[test]
fn kepler_solver_matches_bisection_and_vallado_example_2_1() {
    // Vallado 4th ed. Example 2-1: M = 235.4 deg, e = 0.4 -> E = 220.512074767522 deg (first row).
    for case in reference()["kepler"].as_array().unwrap() {
        let m = f(&case["M_deg"]);
        let e = f(&case["e"]);
        let nu = solve_kepler_equation(m.to_radians(), e).to_degrees();
        assert!(
            ang_diff_deg(nu, f(&case["nu_deg"])) < 1e-9,
            "M={m} e={e}: nu {nu}"
        );
    }
}

fn frame_req(
    case: &Value,
    source: CoordinateFrame,
    target: CoordinateFrame,
    pos: [f64; 3],
    vel: Option<[f64; 3]>,
) -> FrameTransformRequest {
    let obs = &case["observer"];
    FrameTransformRequest {
        source_frame: source,
        target_frame: target,
        epoch: case["epoch"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap(),
        position: pos,
        velocity: vel,
        observer_lat_deg: Some(f(&obs["lat"])),
        observer_lon_deg: Some(f(&obs["lon"])),
        observer_alt_m: Some(f(&obs["alt_m"])),
    }
}

#[test]
fn teme_to_ecef_position_and_velocity_match_erfa() {
    for case in reference()["frames"].as_array().unwrap() {
        let epoch = case["epoch"].as_str().unwrap();
        let out = transform_coordinate_frame(&frame_req(
            case,
            CoordinateFrame::EciTeme,
            CoordinateFrame::EcefWgs84,
            vec3(&case["teme_pos"]),
            Some(vec3(&case["teme_vel"])),
        ))
        .unwrap();
        // 1 m on position, 1 mm/s on velocity (includes the omega x r Earth-rotation term).
        assert_vec_close(
            &format!("{epoch} ecef pos"),
            out.position,
            vec3(&case["ecef_pos"]),
            1e-3,
        );
        assert_vec_close(
            &format!("{epoch} ecef vel"),
            out.velocity.unwrap(),
            vec3(&case["ecef_vel"]),
            1e-6,
        );
        let geo = out.geodetic.unwrap();
        assert!(
            (geo.latitude_deg - f(&case["geodetic"]["lat"])).abs() < 1e-4,
            "{epoch} lat"
        );
        assert!(
            ang_diff_deg(geo.longitude_deg, f(&case["geodetic"]["lon"])) < 1e-4,
            "{epoch} lon"
        );
        assert!(
            (geo.altitude_km - f(&case["geodetic"]["alt_km"])).abs() < 0.01,
            "{epoch} alt"
        );
    }
}

#[test]
fn ecef_to_teme_inverts_reference_rotation() {
    for case in reference()["frames"].as_array().unwrap() {
        let epoch = case["epoch"].as_str().unwrap();
        let out = transform_coordinate_frame(&frame_req(
            case,
            CoordinateFrame::EcefWgs84,
            CoordinateFrame::EciTeme,
            vec3(&case["ecef_pos"]),
            Some(vec3(&case["ecef_vel"])),
        ))
        .unwrap();
        assert_vec_close(
            &format!("{epoch} teme pos"),
            out.position,
            vec3(&case["teme_pos"]),
            1e-3,
        );
        assert_vec_close(
            &format!("{epoch} teme vel"),
            out.velocity.unwrap(),
            vec3(&case["teme_vel"]),
            1e-6,
        );
    }
}

#[test]
fn topocentric_sez_ned_and_look_angles_match_erfa_geometry() {
    for case in reference()["frames"].as_array().unwrap() {
        let epoch = case["epoch"].as_str().unwrap();
        for (frame, key) in [
            (CoordinateFrame::TopocentricSez, "sez"),
            (CoordinateFrame::TopocentricNed, "ned"),
        ] {
            let out = transform_coordinate_frame(&frame_req(
                case,
                CoordinateFrame::EcefWgs84,
                frame,
                vec3(&case["ecef_pos"]),
                None,
            ))
            .unwrap();
            assert_vec_close(
                &format!("{epoch} {key}"),
                out.position,
                vec3(&case[key]),
                1e-3,
            );
            let look = out.look_angles.unwrap();
            assert!(
                ang_diff_deg(look.azimuth_deg, f(&case["az"])) < 1e-3,
                "{epoch} az"
            );
            assert!(
                (look.elevation_deg - f(&case["el"])).abs() < 1e-3,
                "{epoch} el"
            );
            assert!(
                (look.slant_range_km - f(&case["range_km"])).abs() < 0.01,
                "{epoch} range"
            );
        }
    }
}
