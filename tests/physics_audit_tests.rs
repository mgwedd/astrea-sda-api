//! Physics audit proof tests. Reference values come from Skyfield 1.x (SGP4 + WGS-84 + DE421)
//! or from closed-form identities, never from this crate's own code.
use astrea_sda_api::{
    models::{LightingState, Satellite, Tle},
    services::astrodynamics::*,
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn sat(name: &str, l1: &str, l2: &str) -> Satellite {
    Satellite {
        id: Uuid::new_v4(),
        name: name.into(),
        tle: Tle {
            line_one: l1.into(),
            line_two: l2.into(),
        },
        created_date: Utc::now(),
        last_modified_date: Utc::now(),
    }
}

// Epoch 2021-08-27 15:52:51 UTC; all checks run within minutes of epoch.
fn atlas() -> Satellite {
    sat(
        "ATLAS CENTAUR 2",
        "1 00694U 63047A   21239.66170074  .00000250  00000-0  20987-4 0  9994",
        "2 00694  30.3579   8.5616 0584817  14.9507 346.7615 14.02868132898397",
    )
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2021, 8, 27, 16, 0, 0).unwrap()
}

// Skyfield: sub-satellite point at t0 = (13.92330, 177.31523), 467.250 km.
const SUB_LAT: f64 = 13.923295486819791;
const SUB_LON: f64 = 177.31523434723331;

// Crossing orbits that share an ascending node at epoch 2021-08-27 12:00:00 UTC.
fn crossing_pair() -> (Satellite, Satellite) {
    (
        sat(
            "A",
            "1 90001U 21001A   21239.50000000  .00000000  00000-0  00000-0 0  9996",
            "2 90001  51.6000 100.0000 0001000   0.0000   0.0000 15.50000000    18",
        ),
        sat(
            "B",
            "1 90002U 21001A   21239.50000000  .00000000  00000-0  00000-0 0  9997",
            "2 90002  60.0000 100.0000 0001000   0.0000   0.0000 15.50000000    13",
        ),
    )
}

fn ang_diff(a: f64, b: f64) -> f64 {
    ((a - b + 540.0) % 360.0 - 180.0).abs()
}

#[test]
fn a01_gmst_matches_vallado_example_3_5() {
    // Vallado Ex 3-5: 1992-08-20 12:14:00 UT1, GMST = 152.578787810 deg
    let t = Utc.with_ymd_and_hms(1992, 8, 20, 12, 14, 0).unwrap();
    let gmst = calculate_local_sidereal_time(t, 0.0).to_degrees();
    assert!(
        ang_diff(gmst, 152.578787810) < 0.01,
        "GMST {gmst} vs 152.5788"
    );
}

#[test]
fn a02_gmst_matches_skyfield_at_t0() {
    let gmst = calculate_local_sidereal_time(t0(), 0.0).to_degrees();
    let expected = 14.407293610572868 * 15.0;
    assert!(ang_diff(gmst, expected) < 0.01, "GMST {gmst} vs {expected}");
}

#[test]
fn a03_sun_position_matches_equinox_geometry() {
    // 2021 March equinox 2021-03-20 09:37 UTC: sun RA ~0, Dec ~0
    let t = Utc.with_ymd_and_hms(2021, 3, 20, 9, 37, 0).unwrap();
    let s = calculate_sun_position_eci(t);
    let ra = s[1].atan2(s[0]).to_degrees();
    assert!(ang_diff(ra, 0.0) < 0.1, "sun RA at equinox {ra} deg");
}

#[test]
fn a04_ground_track_subpoint_matches_skyfield() {
    let gt = generate_ground_track(
        &atlas(),
        t0(),
        1,
        60,
        false,
        false,
        &CancellationToken::new(),
    )
    .unwrap();
    let p = &gt.trajectory[0];
    assert!((p.lat - SUB_LAT).abs() < 0.05, "lat {} vs {SUB_LAT}", p.lat);
    assert!(
        ang_diff(p.lon, SUB_LON) < 0.05,
        "lon {} vs {SUB_LON}",
        p.lon
    );
    assert!((p.alt_km - 467.2497).abs() < 1.0, "alt {}", p.alt_km);
}

#[test]
fn a05_look_angles_observer_due_north_of_subpoint() {
    // Skyfield: az 180.000, el 22.9365, range 1028.955 km
    let la = calculate_look_angles(&atlas(), SUB_LAT + 8.0, SUB_LON, 0.0, t0()).unwrap();
    assert!(
        (la.range_km - 1028.955).abs() < 2.0,
        "range {}",
        la.range_km
    );
    assert!((la.elevation - 22.9365).abs() < 0.2, "el {}", la.elevation);
    assert!(ang_diff(la.azimuth, 180.0) < 0.5, "az {}", la.azimuth);
}

#[test]
fn a06_look_angles_observer_due_east_of_subpoint() {
    // Skyfield: az 271.206, el 17.740, range 1211.855 km
    let la = calculate_look_angles(&atlas(), SUB_LAT, SUB_LON + 10.0, 0.0, t0()).unwrap();
    assert!(
        (la.range_km - 1211.855).abs() < 2.0,
        "range {}",
        la.range_km
    );
    assert!(ang_diff(la.azimuth, 271.206) < 0.5, "az {}", la.azimuth);
}

#[test]
fn a07_look_angles_pure_geometry_azimuth_convention() {
    // Observer at (0,0) on equator; target 100 km due north, 100 km up. Az must be 0 (north).
    let la = ecf_to_look_angles(0.0, 0.0, 0.0, [6378.137 + 100.0, 0.0, 100.0]);
    assert!(
        ang_diff(la.azimuth, 0.0) < 1e-6,
        "az north expected 0, got {}",
        la.azimuth
    );
    // Target due east: az must be 90.
    let la = ecf_to_look_angles(0.0, 0.0, 0.0, [6378.137 + 100.0, 100.0, 0.0]);
    assert!(
        ang_diff(la.azimuth, 90.0) < 1e-6,
        "az east expected 90, got {}",
        la.azimuth
    );
}

#[test]
fn a08_observer_altitude_is_metres_per_api_docs() {
    // API docs: `alt` is metres. 100 m vs 0 m must change range by < 0.2 km, not ~100 km.
    let lat = SUB_LAT + 8.0;
    let r0 = find_next_visible_pass(&atlas(), lat, SUB_LON, 0.0, t0(), -90.0, 1).unwrap();
    let r1 = find_next_visible_pass(&atlas(), lat, SUB_LON, 100.0, t0(), -90.0, 1).unwrap();
    let dr = (r1.range_km - r0.range_km).abs();
    assert!(dr < 0.2, "100 m observer altitude moved range by {dr} km");
}

#[test]
fn a09_doppler_range_rate_matches_skyfield() {
    // Skyfield range rate for observer 8 deg north of subpoint: -3.0364 km/s
    let r = calculate_doppler_shift(&atlas(), 437.5e6, SUB_LAT + 8.0, SUB_LON, 0.0, t0(), false)
        .unwrap();
    assert!(
        (r.range_rate_kms + 3.0364).abs() < 0.05,
        "range rate {}",
        r.range_rate_kms
    );
}

#[test]
fn a10_doppler_range_rate_far_observer_matches_skyfield() {
    // Skyfield range rate for observer (20, -100): -5.3677 km/s
    let r = calculate_doppler_shift(&atlas(), 437.5e6, 20.0, -100.0, 0.0, t0(), false).unwrap();
    assert!(
        (r.range_rate_kms + 5.3677).abs() < 0.05,
        "range rate {}",
        r.range_rate_kms
    );
}

#[test]
fn a11_shadow_state_matches_skyfield() {
    // Skyfield: satellite NOT sunlit at t0
    let r = calculate_illumination(&atlas(), 0.0, 0.0, 0.0, t0()).unwrap();
    assert_eq!(r.lighting_state, LightingState::Umbra);
}

#[test]
fn a12_observer_sun_elevation_matches_skyfield() {
    // Skyfield sun elevation at (51.5, 0.0): +26.32 deg
    let r = calculate_illumination(&atlas(), 51.5, 0.0, 0.0, t0()).unwrap();
    assert!(
        (r.observer_sun_elevation_deg - 26.32).abs() < 0.2,
        "{}",
        r.observer_sun_elevation_deg
    );
}

#[test]
fn a13_overhead_never_returns_satellite_below_horizon() {
    // Observer at the antipode of the sub-satellite point: satellite elevation ~ -90 deg.
    let r = find_overhead_satellite(&[atlas()], -SUB_LAT, SUB_LON - 180.0, 0.0, t0());
    assert!(
        r.is_none(),
        "below-horizon satellite reported overhead: {:?}",
        r.map(|o| o.elevation)
    );
}

#[test]
fn a14_not_observable_when_below_horizon() {
    // Skyfield 2021-08-27 16:10 UTC: sat sunlit, observer at antipode, night, sat el = -89.8
    let t = Utc.with_ymd_and_hms(2021, 8, 27, 16, 10, 0).unwrap();
    let r =
        calculate_illumination(&atlas(), -27.875321159327317, 33.78782841168473, 0.0, t).unwrap();
    assert!(
        !r.is_visibly_observable,
        "satellite 90 deg below horizon reported observable"
    );
}

#[test]
fn a15_conjunction_not_missed_by_coarse_sampling() {
    // Crossing orbits at the shared ascending node. Skyfield 1 s scan: true miss 1.58 km at
    // epoch + 2 s. Start 30 s before epoch so the 1-minute grid straddles the TCA.
    let start = Utc.with_ymd_and_hms(2021, 8, 27, 11, 59, 30).unwrap();
    let (a, b) = crossing_pair();
    let res = find_conjunctions(&[a, b], start, 5.0, 1);
    assert_eq!(
        res.conjunctions_found, 1,
        "1.58 km miss not reported at 5 km threshold"
    );
    assert!(
        res.results[0].min_distance_km < 3.0,
        "{}",
        res.results[0].min_distance_km
    );
}

#[test]
fn a16_czml_time_tags_match_trajectory_timestamps() {
    let gt = generate_ground_track(
        &atlas(),
        t0(),
        10,
        60,
        false,
        true,
        &CancellationToken::new(),
    )
    .unwrap();
    let czml = gt.czml.unwrap();
    let cart = czml[1]["position"]["cartesian"].as_array().unwrap();
    for (i, p) in gt.trajectory.iter().enumerate() {
        let off = (p.timestamp - gt.trajectory[0].timestamp).num_seconds() as f64;
        assert_eq!(cart[i * 4].as_f64().unwrap(), off);
    }
}

// Skyfield find_events, observer (35, -120), 2021-08-27: rises above 10 deg at
// 16:11:57.168 (peak 52.802 deg at 16:16:36.852).
#[test]
fn a17_next_pass_aos_matches_skyfield() {
    let r = find_next_visible_pass(&atlas(), 35.0, -120.0, 0.0, t0(), 10.0, 1440).unwrap();
    let aos = Utc.with_ymd_and_hms(2021, 8, 27, 16, 11, 57).unwrap() + Duration::milliseconds(168);
    let err = (r.pass_time - aos).num_milliseconds().abs();
    assert!(err < 2000, "AOS {} vs {aos} ({err} ms)", r.pass_time);
}

// Same pass with a 52.7 deg threshold is above it for only 13.6 s (16:16:30.176 - 16:16:43.749).
// A 60 s grid without peak refinement steps straight over it.
#[test]
fn a18_next_pass_finds_pass_shorter_than_sample_step() {
    let r = find_next_visible_pass(&atlas(), 35.0, -120.0, 0.0, t0(), 52.7, 1440).unwrap();
    let aos = Utc.with_ymd_and_hms(2021, 8, 27, 16, 16, 30).unwrap() + Duration::milliseconds(176);
    let err = (r.pass_time - aos).num_milliseconds().abs();
    assert!(err < 2000, "AOS {} vs {aos} ({err} ms)", r.pass_time);
}

#[test]
fn a19_footprint_is_instantaneous_and_omitted_when_ring_is_not_simple() {
    let gt = generate_ground_track(
        &atlas(),
        t0(),
        90,
        60,
        true,
        false,
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(
        gt.footprint_radius_km,
        calculate_footprint_radius(gt.trajectory[0].alt_km, gt.trajectory[0].lat)
    );
    assert!(footprint_ring_is_simple(0.0, 0.0, 2000.0));
    assert!(
        !footprint_ring_is_simple(80.0, 0.0, 2000.0),
        "ring encloses the pole"
    );
    assert!(
        !footprint_ring_is_simple(0.0, 175.0, 2000.0),
        "ring crosses the anti-meridian"
    );
    // Sub-point at t0 is lon 177.3 with a ~2,400 km radius: must not emit a wrapped polygon.
    assert!(gt.footprint_polygon.is_none());
}

#[test]
fn a20_same_catalogue_object_is_not_a_conjunction() {
    let (a, _) = crossing_pair();
    let mut dup = a.clone();
    dup.id = Uuid::new_v4();
    let res = find_conjunctions(&[a, dup], t0(), 10.0, 1);
    assert_eq!(res.conjunctions_found, 0);
}
