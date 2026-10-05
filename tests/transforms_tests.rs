use astrea_sda_api::models::{
    CartesianState, CoordinateFrame, ElementTransformRequest, EquinoctialInput,
    FrameTransformRequest, KeplerianInput,
};
use astrea_sda_api::services::astrodynamics::{
    cartesian_to_keplerian, keplerian_to_cartesian, keplerian_to_equinoctial,
    transform_coordinate_frame, transform_orbital_elements,
};
use chrono::{TimeZone, Utc};

const MU: f64 = 398_600.441_8;

fn ang_diff_deg(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn norm(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[test]
fn circular_equatorial_speed_and_period() {
    let a = 7000.0;
    let s = keplerian_to_cartesian(a, 0.0, 0.0, 0.0, 0.0, 0.0).unwrap();
    assert!((norm(s.position_km) - a).abs() < 1e-9);
    assert!((norm(s.velocity_kms) - (MU / a).sqrt()).abs() < 1e-9);
    let k = cartesian_to_keplerian(s.position_km, s.velocity_kms);
    let period_min = 2.0 * std::f64::consts::PI * (a.powi(3) / MU).sqrt() / 60.0;
    assert!((k.orbital_period_minutes - period_min).abs() < 1e-6);
}

#[test]
fn keplerian_cartesian_round_trip_iss_molniya_geo_polar() {
    // (a, e, i, raan, argp, nu)
    let cases = [
        (6790.0, 0.0005, 51.64, 195.9, 98.8, 261.4), // ISS-like
        (26560.0, 0.74, 63.4, 120.0, 270.0, 40.0),   // Molniya-like
        (42164.0, 0.0002, 0.05, 80.0, 10.0, 200.0),  // GEO-like
        (7200.0, 0.01, 90.0, 10.0, 45.0, 120.0),     // polar
        (7000.0, 0.02, 120.0, 300.0, 200.0, 330.0),  // retrograde
    ];
    for (a, e, i, raan, argp, nu) in cases {
        let s = keplerian_to_cartesian(a, e, i, raan, argp, nu).unwrap();
        let k = cartesian_to_keplerian(s.position_km, s.velocity_kms);
        assert!((k.semi_major_axis_km - a).abs() < 1e-5, "a for {a}");
        assert!((k.eccentricity - e).abs() < 1e-9, "e for {a}");
        assert!((k.inclination_deg - i).abs() < 1e-7, "i for {a}");
        // RAAN, argp, nu are ill-conditioned for tiny e / i, so compare only when well conditioned.
        if e > 1e-3 && i > 1e-2 {
            assert!(ang_diff_deg(k.raan_deg, raan) < 1e-6, "raan for {a}");
            assert!(
                ang_diff_deg(k.arg_of_perigee_deg, argp) < 1e-6,
                "argp for {a}"
            );
            assert!(ang_diff_deg(k.true_anomaly_deg, nu) < 1e-6, "nu for {a}");
        }
        // Whatever the angle conditioning, the state must reproduce exactly.
        let s2 = keplerian_to_cartesian(
            k.semi_major_axis_km,
            k.eccentricity,
            k.inclination_deg,
            k.raan_deg,
            k.arg_of_perigee_deg,
            k.true_anomaly_deg,
        )
        .unwrap();
        for j in 0..3 {
            assert!((s.position_km[j] - s2.position_km[j]).abs() < 1e-5);
            assert!((s.velocity_kms[j] - s2.velocity_kms[j]).abs() < 1e-8);
        }
    }
}

#[test]
fn equinoctial_values_for_known_orbit() {
    let (a, e, i, raan, argp, nu) = (26560.0, 0.74, 63.4_f64, 120.0_f64, 270.0_f64, 40.0_f64);
    let s = keplerian_to_cartesian(a, e, i, raan, argp, nu).unwrap();
    let k = cartesian_to_keplerian(s.position_km, s.velocity_kms);
    let m = keplerian_to_equinoctial(&k, None);
    assert_eq!(m.retrograde_factor, 1);
    assert!((m.p_km - a * (1.0 - e * e)).abs() < 1e-4);
    let varpi = (argp + raan).to_radians();
    assert!((m.f - e * varpi.cos()).abs() < 1e-8);
    assert!((m.g - e * varpi.sin()).abs() < 1e-8);
    let t = (i.to_radians() / 2.0).tan();
    assert!((m.h - t * raan.to_radians().cos()).abs() < 1e-8);
    assert!((m.k - t * raan.to_radians().sin()).abs() < 1e-8);
    assert!(ang_diff_deg(m.true_longitude_deg, raan + argp + nu) < 1e-6);
}

#[test]
fn equinoctial_is_nonsingular_for_circular_equatorial() {
    let s = keplerian_to_cartesian(7000.0, 0.0, 0.0, 0.0, 0.0, 30.0).unwrap();
    let k = cartesian_to_keplerian(s.position_km, s.velocity_kms);
    let m = keplerian_to_equinoctial(&k, None);
    assert!(m.f.abs() < 1e-9 && m.g.abs() < 1e-9);
    assert!(m.h.abs() < 1e-9 && m.k.abs() < 1e-9);
    assert!(ang_diff_deg(m.true_longitude_deg, 30.0) < 1e-6);
}

#[test]
fn unified_transform_all_inputs_agree() {
    let (a, e, i, raan, argp, nu) = (7200.0, 0.05, 98.0, 40.0, 60.0, 100.0);
    let from_kep = transform_orbital_elements(&ElementTransformRequest {
        cartesian: None,
        keplerian: Some(KeplerianInput {
            semi_major_axis_km: a,
            eccentricity: e,
            inclination_deg: i,
            raan_deg: raan,
            arg_of_perigee_deg: argp,
            true_anomaly_deg: Some(nu),
            mean_anomaly_deg: None,
        }),
        equinoctial: None,
    })
    .unwrap();

    let from_cart = transform_orbital_elements(&ElementTransformRequest {
        cartesian: Some(CartesianState {
            position_km: from_kep.cartesian.position_km,
            velocity_kms: from_kep.cartesian.velocity_kms,
        }),
        keplerian: None,
        equinoctial: None,
    })
    .unwrap();

    let eq = &from_kep.equinoctial;
    let from_eq = transform_orbital_elements(&ElementTransformRequest {
        cartesian: None,
        keplerian: None,
        equinoctial: Some(EquinoctialInput {
            p_km: eq.p_km,
            f: eq.f,
            g: eq.g,
            h: eq.h,
            k: eq.k,
            true_longitude_deg: Some(eq.true_longitude_deg),
            mean_longitude_deg: None,
            retrograde_factor: Some(eq.retrograde_factor),
        }),
    })
    .unwrap();

    for other in [&from_cart, &from_eq] {
        for j in 0..3 {
            assert!(
                (from_kep.cartesian.position_km[j] - other.cartesian.position_km[j]).abs() < 1e-4
            );
            assert!(
                (from_kep.cartesian.velocity_kms[j] - other.cartesian.velocity_kms[j]).abs() < 1e-7
            );
        }
        assert!((other.keplerian.semi_major_axis_km - a).abs() < 1e-4);
        assert!((other.keplerian.inclination_deg - i).abs() < 1e-6);
    }
}

#[test]
fn mean_anomaly_input_matches_true_anomaly_input() {
    let (a, e) = (26560.0, 0.7);
    let base = transform_orbital_elements(&ElementTransformRequest {
        cartesian: None,
        keplerian: Some(KeplerianInput {
            semi_major_axis_km: a,
            eccentricity: e,
            inclination_deg: 63.4,
            raan_deg: 10.0,
            arg_of_perigee_deg: 270.0,
            true_anomaly_deg: Some(75.0),
            mean_anomaly_deg: None,
        }),
        equinoctial: None,
    })
    .unwrap();
    let m = base.keplerian.mean_anomaly_deg;
    let via_m = transform_orbital_elements(&ElementTransformRequest {
        cartesian: None,
        keplerian: Some(KeplerianInput {
            semi_major_axis_km: a,
            eccentricity: e,
            inclination_deg: 63.4,
            raan_deg: 10.0,
            arg_of_perigee_deg: 270.0,
            true_anomaly_deg: None,
            mean_anomaly_deg: Some(m),
        }),
        equinoctial: None,
    })
    .unwrap();
    assert!(ang_diff_deg(via_m.keplerian.true_anomaly_deg, 75.0) < 1e-6);
}

#[test]
fn invalid_element_inputs_are_rejected() {
    assert!(keplerian_to_cartesian(-7000.0, 0.0, 0.0, 0.0, 0.0, 0.0).is_err());
    assert!(keplerian_to_cartesian(7000.0, 1.2, 0.0, 0.0, 0.0, 0.0).is_err());
    let empty = ElementTransformRequest {
        cartesian: None,
        keplerian: None,
        equinoctial: None,
    };
    assert!(transform_orbital_elements(&empty).is_err());
}

fn frame_req(
    source: CoordinateFrame,
    target: CoordinateFrame,
    pos: [f64; 3],
    vel: Option<[f64; 3]>,
    observer: bool,
) -> FrameTransformRequest {
    FrameTransformRequest {
        source_frame: source,
        target_frame: target,
        epoch: Utc.with_ymd_and_hms(2024, 3, 23, 21, 31, 0).unwrap(),
        position: pos,
        velocity: vel,
        observer_lat_deg: observer.then_some(30.2672),
        observer_lon_deg: observer.then_some(-97.7431),
        observer_alt_m: observer.then_some(150.0),
    }
}

#[test]
fn eci_ecef_round_trip_with_velocity() {
    let pos = [4000.0, 5000.0, 3000.0];
    let vel = [-3.0, 4.0, 5.0];
    let there = transform_coordinate_frame(&frame_req(
        CoordinateFrame::EciTeme,
        CoordinateFrame::EcefWgs84,
        pos,
        Some(vel),
        false,
    ))
    .unwrap();
    // Rotation preserves position magnitude and z.
    assert!((norm(there.position) - norm(pos)).abs() < 1e-9);
    assert!((there.position[2] - pos[2]).abs() < 1e-12);
    let back = transform_coordinate_frame(&frame_req(
        CoordinateFrame::EcefWgs84,
        CoordinateFrame::EciTeme,
        there.position,
        there.velocity,
        false,
    ))
    .unwrap();
    for j in 0..3 {
        assert!((back.position[j] - pos[j]).abs() < 1e-8);
        assert!((back.velocity.unwrap()[j] - vel[j]).abs() < 1e-10);
    }
}

#[test]
fn ecef_sez_ned_round_trip_and_sign_conventions() {
    // A point roughly overhead the observer: ~500 km above Austin in ECEF.
    let obs =
        astrea_sda_api::services::astrodynamics::observer_geodetic_to_ecf(30.2672, -97.7431, 0.15);
    let up = [
        30.2672_f64.to_radians().cos() * (-97.7431_f64).to_radians().cos(),
        30.2672_f64.to_radians().cos() * (-97.7431_f64).to_radians().sin(),
        30.2672_f64.to_radians().sin(),
    ];
    let pos = [
        obs[0] + 500.0 * up[0],
        obs[1] + 500.0 * up[1],
        obs[2] + 500.0 * up[2],
    ];
    let sez = transform_coordinate_frame(&frame_req(
        CoordinateFrame::EcefWgs84,
        CoordinateFrame::TopocentricSez,
        pos,
        None,
        true,
    ))
    .unwrap();
    // Zenith component ~500 km, horizontals ~0 (geodetic up differs slightly from geocentric up).
    assert!((sez.position[2] - 500.0).abs() < 1.0);
    let ned = transform_coordinate_frame(&frame_req(
        CoordinateFrame::EcefWgs84,
        CoordinateFrame::TopocentricNed,
        pos,
        None,
        true,
    ))
    .unwrap();
    assert!((ned.position[0] + sez.position[0]).abs() < 1e-9);
    assert!((ned.position[1] - sez.position[1]).abs() < 1e-9);
    assert!((ned.position[2] + sez.position[2]).abs() < 1e-9);
    let la = ned.look_angles.unwrap();
    assert!(la.elevation_deg > 89.0);
    assert!((la.slant_range_km - 500.0).abs() < 1.0);

    let back = transform_coordinate_frame(&frame_req(
        CoordinateFrame::TopocentricNed,
        CoordinateFrame::EcefWgs84,
        ned.position,
        None,
        true,
    ))
    .unwrap();
    for (b, p) in back.position.iter().zip(pos.iter()) {
        assert!((b - p).abs() < 1e-7);
    }
}

#[test]
fn topocentric_requires_observer() {
    let r = transform_coordinate_frame(&frame_req(
        CoordinateFrame::EcefWgs84,
        CoordinateFrame::TopocentricSez,
        [7000.0, 0.0, 0.0],
        None,
        false,
    ));
    assert!(r.is_err());
}

/// Independent oracle: Vallado, *Fundamentals of Astrodynamics and Applications*,
/// Example 2-5 (RV -> COE). Expected values are the published textbook results.
#[test]
fn vallado_example_2_5_rv_to_coe() {
    let k = cartesian_to_keplerian(
        [6524.834, 6862.875, 6448.296],
        [4.901327, 5.533756, -1.976341],
    );
    assert!(
        (k.semi_major_axis_km - 36127.343).abs() < 0.5,
        "a = {}",
        k.semi_major_axis_km
    );
    assert!(
        (k.eccentricity - 0.832853).abs() < 1e-5,
        "e = {}",
        k.eccentricity
    );
    assert!(
        (k.inclination_deg - 87.870).abs() < 1e-3,
        "i = {}",
        k.inclination_deg
    );
    assert!(
        ang_diff_deg(k.raan_deg, 227.898) < 1e-3,
        "raan = {}",
        k.raan_deg
    );
    assert!(
        ang_diff_deg(k.arg_of_perigee_deg, 53.38) < 1e-2,
        "argp = {}",
        k.arg_of_perigee_deg
    );
    assert!(
        ang_diff_deg(k.true_anomaly_deg, 92.335) < 1e-3,
        "nu = {}",
        k.true_anomaly_deg
    );
}
