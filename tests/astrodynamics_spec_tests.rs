//! Checks of the refined math spec against independent oracles (closed-form geometry,
//! Vallado, and the sgp4 crate's own Brouwer recovery).
use astrea_sda_api::services::astrodynamics::*;
use chrono::{TimeZone, Utc};

const MU: f64 = 398_600.441_8;

#[test]
fn tt_minus_utc_is_69_184_seconds_and_ut1_equals_utc_without_eop() {
    let t = Utc.with_ymd_and_hms(2024, 3, 24, 0, 0, 0).unwrap();
    let dt_s = (julian_date_tt(t) - julian_date(t)) * 86_400.0;
    assert!((dt_s - 69.184).abs() < 1e-3, "{dt_s}");
    assert_eq!(julian_date_ut1(t), julian_date(t));
}

// Vallado Example 5-1: Sun at 2006-04-02 00:00 UT1 = [146186212, 28788976, 12481064] km (MOD).
#[test]
fn sun_direction_matches_vallado_example_5_1() {
    let t = Utc.with_ymd_and_hms(2006, 4, 2, 0, 0, 0).unwrap();
    let got = calculate_sun_position_eci(t);
    let exp = [146_186_212.0, 28_788_976.0, 12_481_064.0];
    let dot: f64 = got.iter().zip(exp).map(|(a, b)| a * b).sum();
    let n = |v: &[f64]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
    let ang = (dot / (n(&got) * n(&exp)))
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees();
    assert!(ang < 0.02, "direction error {ang} deg");
}

#[test]
fn local_radius_matches_wgs84_closed_forms() {
    assert!((local_earth_radius_km(0.0) - 6378.137).abs() < 1e-6);
    assert!((local_earth_radius_km(90.0) - 6_356.752_314).abs() < 1e-3);
    assert!((local_earth_radius_km(-90.0) - 6_356.752_314).abs() < 1e-3);
    // Geodetic 45 deg: R = 6367.489 km (standard WGS-84 value)
    assert!((local_earth_radius_km(45.0) - 6367.489).abs() < 0.01);
}

// Independent oracle: R * acos(R / (R + h)) with WGS-84 radii evaluated outside the crate
// (equator a, geodetic 45 deg 6367.4895 km, pole b), h = 500 km.
#[test]
fn footprint_radius_uses_local_radius() {
    for (lat, expected) in [(0.0, 2446.9500), (45.0, 2444.7818), (90.0, 2442.5933)] {
        let got = calculate_footprint_radius(500.0, lat);
        assert!(
            (got - expected).abs() < 1e-3,
            "lat {lat}: {got} vs {expected}"
        );
    }
}

#[test]
fn polar_cap_is_never_simple_even_at_the_pole() {
    // cos(90 deg) -> 0 would divide by ~0; the guard must return before it
    assert!(!footprint_ring_is_simple(90.0, 0.0, 100.0));
    assert!(!footprint_ring_is_simple(-90.0, 10.0, 2000.0));
    assert!(footprint_ring_is_simple(0.0, 0.0, 2000.0));
}

#[test]
fn phase_function_and_magnitude_closed_forms() {
    use std::f64::consts::PI;
    assert!((lambert_sphere_phase_function(0.0) - 1.0).abs() < 1e-12);
    assert!((lambert_sphere_phase_function(PI / 2.0) - 1.0 / PI).abs() < 1e-12);
    assert!(lambert_sphere_phase_function(PI).abs() < 1e-12);
    // M0 = 2.5 at 1000 km, zero phase
    assert!((visual_magnitude(1000.0, 0.0) - 2.5).abs() < 1e-12);
    // quarter phase: +2.5 log10(pi)
    let dm = visual_magnitude(1000.0, PI / 2.0) - 2.5;
    assert!((dm - 2.5 * PI.log10()).abs() < 1e-12);
    // backlit floor keeps the result finite: +2.5*3 = 7.5 mag at most
    assert!((visual_magnitude(1000.0, PI) - 10.0).abs() < 1e-12);
    // 0.1 range floor
    assert!((visual_magnitude(1.0, 0.0) - (2.5 - 5.0)).abs() < 1e-12);
}

#[test]
fn phase_angle_is_sun_satellite_observer_angle() {
    let sat = [7000.0, 0.0, 0.0];
    let sun = [1.5e8, 0.0, 0.0]; // along +x from the satellite
    let obs = [7000.0, 0.0, 500.0]; // along +z from the satellite
    let g = phase_angle_rad(sat, obs, sun);
    assert!((g - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    // observer between satellite and sun gives zero phase angle
    assert!(phase_angle_rad(sat, [7100.0, 0.0, 0.0], sun).abs() < 1e-9);
}

/// Appends the TLE modulo-10 checksum (digits count, '-' counts 1).
fn with_checksum(body: &str) -> String {
    let sum: u32 = body
        .chars()
        .map(|c| c.to_digit(10).unwrap_or(u32::from(c == '-')))
        .sum();
    format!("{body}{}", sum % 10)
}

#[test]
fn brouwer_sma_matches_sgp4_crate_recovery_and_differs_from_kepler() {
    let l1 = with_checksum("1 25544U 98067A   24084.00000000  .00016717  00000-0  10270-3 0  999");
    let l2 = with_checksum("2 25544  98.0000 208.9163 0006317  69.9862  25.2906 15.50000000    1");
    let e = sgp4::Elements::from_tle(None, l1.as_bytes(), l2.as_bytes()).unwrap();
    let got = brouwer_mean_semi_major_axis_km(&e);

    let g = &sgp4::WGS72;
    let n_o = e.mean_motion * 2.0 * std::f64::consts::PI / 1440.0;
    let orbit = sgp4::Orbit::from_kozai_elements(
        g,
        e.inclination.to_radians(),
        e.right_ascension.to_radians(),
        e.eccentricity,
        e.argument_of_perigee.to_radians(),
        e.mean_anomaly.to_radians(),
        n_o,
    )
    .unwrap();
    let oracle = (g.ke / orbit.mean_motion).powf(2.0 / 3.0) * g.ae;
    assert!((got - oracle).abs() < 0.05, "{got} vs {oracle}");

    let n = e.mean_motion * 2.0 * std::f64::consts::PI / 86_400.0;
    let kepler = (MU / (n * n)).cbrt();
    assert!(
        (got - kepler).abs() > 1.0,
        "Kepler differs by {}",
        got - kepler
    );
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / n, v[1] / n, v[2] / n]
}

#[test]
fn circular_inclined_orbit_reports_argument_of_latitude_as_true_anomaly() {
    let (i, raan, u) = (51.6f64.to_radians(), 40f64.to_radians(), 70f64.to_radians());
    let r = 6778.0;
    let pos = [
        r * (raan.cos() * u.cos() - raan.sin() * u.sin() * i.cos()),
        r * (raan.sin() * u.cos() + raan.cos() * u.sin() * i.cos()),
        r * u.sin() * i.sin(),
    ];
    let v = (MU / r).sqrt();
    let vel = [
        v * (-raan.cos() * u.sin() - raan.sin() * u.cos() * i.cos()),
        v * (-raan.sin() * u.sin() + raan.cos() * u.cos() * i.cos()),
        v * u.cos() * i.sin(),
    ];
    let el = osculating_elements(pos, vel);
    assert_eq!(el.arg_of_perigee_deg, 0.0);
    assert!(
        (el.true_anomaly_deg - 70.0).abs() < 1e-6,
        "{}",
        el.true_anomaly_deg
    );
    assert!((el.raan_deg - 40.0).abs() < 1e-6);
    assert!((el.mean_anomaly_deg - 70.0).abs() < 1e-4);
}

#[test]
fn circular_equatorial_orbit_reports_true_longitude() {
    let r = 42_164.0;
    let l = 123f64.to_radians();
    let v = (MU / r).sqrt();
    let pos = [r * l.cos(), r * l.sin(), 0.0];
    let vel = [-v * l.sin(), v * l.cos(), 0.0];
    let el = osculating_elements(pos, vel);
    assert_eq!((el.raan_deg, el.arg_of_perigee_deg), (0.0, 0.0));
    assert!(
        (el.true_anomaly_deg - 123.0).abs() < 1e-6,
        "{}",
        el.true_anomaly_deg
    );

    // retrograde: motion reversed, angle measured against rotation sense
    let el = osculating_elements(pos, [v * l.sin(), -v * l.cos(), 0.0]);
    assert!(
        (el.true_anomaly_deg - 237.0).abs() < 1e-6,
        "{}",
        el.true_anomaly_deg
    );
    let _ = unit(pos);
}

#[test]
fn equatorial_elliptical_orbit_reports_longitude_of_periapsis() {
    // periapsis at longitude 200 deg, satellite at periapsis (nu = 0)
    let (a, e) = (20_000.0f64, 0.3f64);
    let rp = a * (1.0 - e);
    let vp = (MU * (1.0 + e) / rp).sqrt();
    let w = 200f64.to_radians();
    let pos = [rp * w.cos(), rp * w.sin(), 0.0];
    let vel = [-vp * w.sin(), vp * w.cos(), 0.0];
    let el = osculating_elements(pos, vel);
    assert_eq!(el.raan_deg, 0.0);
    assert!(
        (el.arg_of_perigee_deg - 200.0).abs() < 1e-6,
        "{}",
        el.arg_of_perigee_deg
    );
    assert!(el.true_anomaly_deg < 1e-4 || el.true_anomaly_deg > 360.0 - 1e-4);
}

// Oracle: purely radial motion with no gravitational term reduces to sqrt((1-b)/(1+b)).
#[test]
fn relativistic_doppler_reduces_to_radial_closed_form() {
    let c = 299_792.458;
    let (f0, v) = (8.4e9, 7.6);
    for rr in [v, -v] {
        let got = relativistic_received_freq_hz(f0, rr, v, 0.0, 7000.0, 7000.0);
        let b = rr / c;
        let exact = f0 * ((1.0 - b) / (1.0 + b)).sqrt();
        assert!((got - exact).abs() < 1e-6, "{got} vs {exact}");
    }
}

// The gravitational blue shift for LEO is ~4e-11 (spec 8.2).
#[test]
fn gravitational_shift_is_about_4e_minus_11_for_leo() {
    let f0 = 1.0e10;
    let with = relativistic_received_freq_hz(f0, 0.0, 0.0, 0.0, 6378.137, 6778.0);
    let rel = with / f0 - 1.0;
    assert!((rel - 4.1e-11).abs() < 0.3e-11, "{rel}");
}

// Time dilation uses inertial speeds of both ends: sqrt((1 - bs^2) / (1 - bo^2)).
#[test]
fn relativistic_doppler_uses_inertial_speeds_of_transmitter_and_receiver() {
    let c = 299_792.458;
    let (f0, vs, vo) = (1.0e10, 7.6, 0.4651);
    let got = relativistic_received_freq_hz(f0, 0.0, vs, vo, 7000.0, 7000.0);
    let exact = f0 * ((1.0 - (vs / c).powi(2)) / (1.0 - (vo / c).powi(2))).sqrt();
    assert!((got - exact).abs() < 1e-6, "{got} vs {exact}");
}

#[test]
fn moon_uses_the_spec_obliquity() {
    let t = Utc.with_ymd_and_hms(2024, 3, 24, 0, 0, 0).unwrap();
    let d = julian_date_tt(t) - 2_451_545.0;
    let [x, y, z] = calculate_lunar_position_eci(t);
    let eps = mean_obliquity_rad(d);
    assert!((mean_obliquity_rad(0.0).to_degrees() - 23.439).abs() < 1e-12);
    // Rotating back to the ecliptic must give beta = 5.128 sin F exactly
    let r = (x * x + y * y + z * z).sqrt();
    let z_ecl = -y * eps.sin() + z * eps.cos();
    let f = (93.272 + 13.229350 * d) % 360.0;
    let beta_deg = (z_ecl / r).asin().to_degrees();
    assert!(
        (beta_deg - 5.128 * f.to_radians().sin()).abs() < 1e-9,
        "{beta_deg}"
    );
}

// A polar orbit with w = 90 deg has its perigee over the pole, so the reference radius is b.
#[test]
fn apsis_altitudes_use_local_radius() {
    let (hp, ha) = apsis_altitudes_km(7000.0, 0.01, 90.0, 90.0);
    assert!((hp - (7000.0 * 0.99 - 6_356.752_314)).abs() < 1e-3, "{hp}");
    assert!((ha - (7000.0 * 1.01 - 6_356.752_314)).abs() < 1e-3, "{ha}");
    // Equatorial orbit: apsides on the equator, radius a
    let (hp, _) = apsis_altitudes_km(7000.0, 0.01, 0.0, 45.0);
    assert!((hp - (7000.0 * 0.99 - 6378.137)).abs() < 1e-9);
}

// Transit edges and centre are computed, not synthesized: ordered, consistent, and not the old
// fixed +/-1 s window.
#[test]
fn transit_times_are_refined_from_the_geometry() {
    use astrea_sda_api::models::{Satellite, Tle, TransitTarget};
    let sat = Satellite {
        id: uuid::Uuid::new_v4(),
        name: "ISS".into(),
        tle: Tle {
            line_one: "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996"
                .into(),
            line_two: "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012"
                .into(),
        },
        created_date: Utc::now(),
        last_modified_date: Utc::now(),
    };
    let start = Utc.with_ymd_and_hms(2024, 3, 24, 0, 0, 0).unwrap();
    let res = find_transits(TransitTarget::Sun, &[sat], 30.0, -97.0, 0.1, start, 7, 60.0).unwrap();
    assert!(res.transits_found > 0);
    for m in &res.results {
        assert!(m.transit_start_utc <= m.transit_center_utc);
        assert!(m.transit_center_utc <= m.transit_end_utc);
        let span = (m.transit_end_utc - m.transit_start_utc).num_milliseconds() as f64 / 1000.0;
        assert!((m.transit_duration_seconds - span).abs() < 1e-9);
        assert!(m.min_angular_separation_deg <= 60.0);
    }
    assert!(res.results.iter().any(|m| m.transit_duration_seconds > 2.0));
}
