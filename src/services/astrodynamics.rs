use crate::error::AppError;
use crate::models::{
    CartesianState, ClassicalKeplerianElements, CollisionProbabilityResponse, ConjunctionMatch,
    ConjunctionSearchResponse, CoordinateFrame, DecayWatchResponse, DopplerResponse,
    ElementTransformRequest, ElementTransformResponse, FrameTransformRequest,
    FrameTransformResponse, GeoJsonFeature, GeoJsonGeometry, GeodeticCoordinates,
    GroundTrackResponse, IlluminationResponse, KeplerianElements, LightingState, LookAnglesDetail,
    ModifiedEquinoctialElements, NextVisiblePassResponse, ObserverTwilightState, OverheadResponse,
    PassScheduleResponse, RelativeMotionPoint, RelativeMotionResponse, Satellite,
    SatelliteDecayRiskResponse, SatellitePass, SatelliteStateResponse, SatelliteSummary,
    TransitMatch, TransitPredictionResponse, TransitTarget,
};
use crate::services::czml;
use crate::services::ephemeris::{Ephemeris, Samples};
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use sgp4::{Constants, Elements, Prediction};
use tokio_util::sync::CancellationToken;

pub struct LookAngles {
    pub azimuth: f64,
    pub elevation: f64,
    pub range_km: f64,
}

/// Normalizes a TLE line: ensures correct line number prefix ('1' or '2') and 69-char width
pub fn normalize_tle_line(line: &str, expected_line_num: char) -> String {
    let mut trimmed = line.trim().to_string();
    if !trimmed.starts_with(expected_line_num) {
        trimmed = format!("{} {}", expected_line_num, trimmed);
    }
    if trimmed.len() < 69 {
        format!("{:<69}", trimmed)
    } else {
        trimmed
    }
}

/// Julian Date (UTC) from the Unix epoch, including sub-second precision
pub fn julian_date(time: DateTime<Utc>) -> f64 {
    time.timestamp_millis() as f64 / 86_400_000.0 + 2_440_587.5
}

/// TAI-UTC in seconds (valid since 2017-01-01, no leap second scheduled)
pub const DELTA_AT_S: f64 = 37.0;
/// TT-TAI in seconds (exact)
pub const TT_MINUS_TAI_S: f64 = 32.184;
/// UT1-UTC in seconds. Taken as 0 (|DUT1| < 0.9 s, <= 0.0038 deg GMST error, ~0.4 km); no IERS feed.
pub const DUT1_S: f64 = 0.0;

/// Julian Date (TT) for ephemeris evaluation: JD_UTC + (dAT + 32.184 s)/86400
pub fn julian_date_tt(time: DateTime<Utc>) -> f64 {
    julian_date(time) + (DELTA_AT_S + TT_MINUS_TAI_S) / 86_400.0
}

/// Julian Date (UT1) for Earth rotation: JD_UTC + DUT1/86400
pub fn julian_date_ut1(time: DateTime<Utc>) -> f64 {
    julian_date(time) + DUT1_S / 86_400.0
}

/// Computes Local Sidereal Time (Greenwich Mean Sidereal Time + East Longitude) in radians
pub fn calculate_local_sidereal_time(time: DateTime<Utc>, lon_deg: f64) -> f64 {
    let d = julian_date_ut1(time) - 2451545.0;
    // GMST in degrees, IAU 1982 (Vallado eq. 3-47) including the T^2 and T^3 terms: dropping
    // them costs ~0.09 arcsec (~3 m) in 2026 and grows quadratically.
    let t = d / 36_525.0;
    let gmst_deg = (280.46061837 + 360.98564736629 * d + 0.000387933 * t * t
        - t * t * t / 38_710_000.0)
        % 360.0;
    let gmst = if gmst_deg < 0.0 {
        gmst_deg + 360.0
    } else {
        gmst_deg
    };

    let lst_deg = (gmst + lon_deg) % 360.0;
    if lst_deg < 0.0 {
        (lst_deg + 360.0).to_radians()
    } else {
        lst_deg.to_radians()
    }
}

/// Converts ECI position [x, y, z] to ECF position
pub fn eci_to_ecf(eci: [f64; 3], lst_rad: f64) -> [f64; 3] {
    let x = eci[0] * lst_rad.cos() + eci[1] * lst_rad.sin();
    let y = -eci[0] * lst_rad.sin() + eci[1] * lst_rad.cos();
    let z = eci[2];
    [x, y, z]
}

/// Calculates Topocentric Horizon look angles (Azimuth, Elevation, Slant Range)
pub fn ecf_to_look_angles(
    lat_deg: f64,
    lon_deg: f64,
    alt_km: f64,
    sat_ecf: [f64; 3],
) -> LookAngles {
    let lat_rad = lat_deg.to_radians();
    let lon_rad = lon_deg.to_radians();

    let re = 6378.137; // WGS84 Equatorial radius
    let f = 1.0 / 298.257223563;
    let c = 1.0 / (1.0 - (2.0 * f - f * f) * lat_rad.sin() * lat_rad.sin()).sqrt();

    // Observer ECF coordinates
    let obs_x = (re * c + alt_km) * lat_rad.cos() * lon_rad.cos();
    let obs_y = (re * c + alt_km) * lat_rad.cos() * lon_rad.sin();
    let obs_z = (re * (c * (1.0 - f) * (1.0 - f)) + alt_km) * lat_rad.sin();

    // Range vector in ECF
    let rx = sat_ecf[0] - obs_x;
    let ry = sat_ecf[1] - obs_y;
    let rz = sat_ecf[2] - obs_z;

    // Rotate ECF vector to Topocentric Horizon frame (South, East, Up)
    let top_s = lat_rad.sin() * lon_rad.cos() * rx + lat_rad.sin() * lon_rad.sin() * ry
        - lat_rad.cos() * rz;
    let top_e = -lon_rad.sin() * rx + lon_rad.cos() * ry;
    let top_u = lat_rad.cos() * lon_rad.cos() * rx
        + lat_rad.cos() * lon_rad.sin() * ry
        + lat_rad.sin() * rz;

    let range_km = (rx * rx + ry * ry + rz * rz).sqrt();
    let elevation = (top_u / range_km).asin().to_degrees();

    let mut azimuth = top_e.atan2(-top_s).to_degrees();
    if azimuth < 0.0 {
        azimuth += 360.0;
    }

    LookAngles {
        azimuth,
        elevation,
        range_km,
    }
}

/// SGP4 propagation helper returning LookAngles at a specific timestamp
pub fn calculate_look_angles(
    satellite: &Satellite,
    lat: f64,
    lon: f64,
    alt: f64,
    time: DateTime<Utc>,
) -> Result<LookAngles, AppError> {
    let line1 = normalize_tle_line(&satellite.tle.line_one, '1');
    let line2 = normalize_tle_line(&satellite.tle.line_two, '2');

    let elements = Elements::from_tle(
        Some(satellite.name.clone()),
        line1.as_bytes(),
        line2.as_bytes(),
    )
    .map_err(|e| AppError::Sgp4Error(format!("Failed to parse TLE: {:?}", e)))?;

    let constants = Constants::from_elements(&elements)
        .map_err(|e| AppError::Sgp4Error(format!("Constants error: {:?}", e)))?;

    // Convert elements.datetime (NaiveDateTime) to DateTime<Utc>
    let epoch_dt = DateTime::<Utc>::from_naive_utc_and_offset(elements.datetime, Utc);
    let minutes_since_epoch = (time - epoch_dt).num_milliseconds() as f64 / 60000.0;

    let prediction: Prediction = constants
        .propagate(minutes_since_epoch)
        .map_err(|e| AppError::Sgp4Error(format!("SGP4 propagation error: {:?}", e)))?;

    let lst = calculate_local_sidereal_time(time, 0.0);
    let sat_ecf = eci_to_ecf(prediction.position, lst);
    Ok(ecf_to_look_angles(lat, lon, alt / 1000.0, sat_ecf))
}

/// SGP4 TEME position (km) at a timestamp
pub fn satellite_position_eci(
    satellite: &Satellite,
    time: DateTime<Utc>,
) -> Result<[f64; 3], AppError> {
    let line1 = normalize_tle_line(&satellite.tle.line_one, '1');
    let line2 = normalize_tle_line(&satellite.tle.line_two, '2');
    let elements = Elements::from_tle(
        Some(satellite.name.clone()),
        line1.as_bytes(),
        line2.as_bytes(),
    )
    .map_err(|e| AppError::Sgp4Error(format!("Failed to parse TLE: {:?}", e)))?;
    let constants = Constants::from_elements(&elements)
        .map_err(|e| AppError::Sgp4Error(format!("Constants error: {:?}", e)))?;
    let epoch_dt = DateTime::<Utc>::from_naive_utc_and_offset(elements.datetime, Utc);
    let minutes = (time - epoch_dt).num_milliseconds() as f64 / 60000.0;
    constants
        .propagate(minutes)
        .map(|p| p.position)
        .map_err(|e| AppError::Sgp4Error(format!("SGP4 propagation error: {:?}", e)))
}

/// Lambertian-sphere phase function Phi(gamma) = (sin g + (pi - g) cos g) / pi
pub fn lambert_sphere_phase_function(gamma_rad: f64) -> f64 {
    (gamma_rad.sin() + (std::f64::consts::PI - gamma_rad) * gamma_rad.cos()) / std::f64::consts::PI
}

/// Apparent visual magnitude: M0 + 5 log10(max(rho/1000, 0.1)) - 2.5 log10(max(Phi(gamma), 1e-3)),
/// with M0 = 2.5 at rho = 1000 km and zero phase angle
pub fn visual_magnitude(range_km: f64, phase_angle_rad: f64) -> f64 {
    2.5 + 5.0 * (range_km / 1000.0).max(0.1).log10()
        - 2.5
            * lambert_sphere_phase_function(phase_angle_rad)
                .max(1e-3)
                .log10()
}

/// Phase angle (Sun-satellite-observer) in radians, all vectors in TEME
pub fn phase_angle_rad(sat_eci: [f64; 3], obs_eci: [f64; 3], sun_eci: [f64; 3]) -> f64 {
    let a = [
        sun_eci[0] - sat_eci[0],
        sun_eci[1] - sat_eci[1],
        sun_eci[2] - sat_eci[2],
    ];
    let b = [
        obs_eci[0] - sat_eci[0],
        obs_eci[1] - sat_eci[1],
        obs_eci[2] - sat_eci[2],
    ];
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let na = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let nb = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    (dot / (na * nb)).clamp(-1.0, 1.0).acos()
}

/// Observer position in TEME (km): WGS-84 geodetic to ECEF, rotated by -GMST
pub fn observer_position_eci(
    lat_deg: f64,
    lon_deg: f64,
    alt_km: f64,
    time: DateTime<Utc>,
) -> [f64; 3] {
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();
    let n = WGS84_A_KM / (1.0 - WGS84_E2 * lat.sin() * lat.sin()).sqrt();
    let ecf = [
        (n + alt_km) * lat.cos() * lon.cos(),
        (n + alt_km) * lat.cos() * lon.sin(),
        (n * (1.0 - WGS84_E2) + alt_km) * lat.sin(),
    ];
    let gmst = calculate_local_sidereal_time(time, 0.0);
    // Inverse of eci_to_ecf: rotate by -GMST
    [
        ecf[0] * gmst.cos() - ecf[1] * gmst.sin(),
        ecf[0] * gmst.sin() + ecf[1] * gmst.cos(),
        ecf[2],
    ]
}

/// Multi-threaded batch evaluation of all satellites using **Rayon** (`par_iter()`)
pub fn find_overhead_satellite(
    satellites: &[Satellite],
    lat: f64,
    lon: f64,
    alt: f64,
    time: DateTime<Utc>,
) -> Option<OverheadResponse> {
    satellites
        .par_iter()
        .filter_map(|sat| {
            calculate_look_angles(sat, lat, lon, alt, time)
                .ok()
                .map(|look| (sat, look.elevation))
                .filter(|(_, elevation)| *elevation > 0.0)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(sat, elevation)| OverheadResponse {
            satellite: sat.clone(),
            elevation,
        })
}

/// Predicts the next pass of a satellite above an elevation threshold.
///
/// Samples elevation every 60 s, refines any elevation peak between samples with a
/// golden-section search (so passes shorter than one step are not skipped), then
/// bisects the threshold crossing (AOS) to 1 s. Assumes elevation is unimodal within
/// +/-60 s of a sampled local maximum, which holds for orbits with period >> 2 min.
pub fn find_next_visible_pass(
    satellite: &Satellite,
    lat: f64,
    lon: f64,
    alt: f64,
    start_time: DateTime<Utc>,
    elevation_threshold_deg: f64,
    search_duration_minutes: i64,
) -> Result<NextVisiblePassResponse, AppError> {
    let elev = |t: DateTime<Utc>| {
        calculate_look_angles(satellite, lat, lon, alt, t)
            .map(|l| l.elevation)
            .unwrap_or(f64::NEG_INFINITY)
    };
    let at =
        |t0: DateTime<Utc>, secs: f64| t0 + chrono::Duration::milliseconds((secs * 1000.0) as i64);

    let step = 60.0;
    let mut prev_t = start_time;
    let mut prev_e = elev(start_time);
    let mut aos_bracket = if prev_e >= elevation_threshold_deg {
        Some((start_time, start_time))
    } else {
        None
    };

    let mut k = 1;
    while aos_bracket.is_none() && k <= search_duration_minutes {
        let t = start_time + chrono::Duration::minutes(k);
        let e = elev(t);
        if e >= elevation_threshold_deg {
            aos_bracket = Some((prev_t, t));
        } else {
            // A short pass can rise above the threshold and set again between samples.
            let next_e = elev(t + chrono::Duration::minutes(1));
            if e >= prev_e && e >= next_e {
                let (mut lo, mut hi) = (-step, step);
                while hi - lo > 1.0 {
                    let m1 = hi - 0.618_033_988_75 * (hi - lo);
                    let m2 = lo + 0.618_033_988_75 * (hi - lo);
                    if elev(at(t, m1)) > elev(at(t, m2)) {
                        hi = m2
                    } else {
                        lo = m1
                    }
                }
                let peak_t = at(t, (lo + hi) / 2.0);
                if elev(peak_t) >= elevation_threshold_deg {
                    aos_bracket = Some((prev_t.max(at(t, -step)), peak_t));
                }
            }
        }
        prev_t = t;
        prev_e = e;
        k += 1;
    }

    let (mut below, mut above) = aos_bracket.ok_or(AppError::NotFound)?;
    while (above - below).num_milliseconds() > 1000 {
        let mid = below + (above - below) / 2;
        if elev(mid) >= elevation_threshold_deg {
            above = mid
        } else {
            below = mid
        }
    }

    let look = calculate_look_angles(satellite, lat, lon, alt, above)?;
    Ok(NextVisiblePassResponse {
        satellite_id: satellite.id,
        satellite_name: satellite.name.clone(),
        pass_time: above,
        elevation_deg: look.elevation,
        azimuth_deg: look.azimuth,
        range_km: look.range_km,
    })
}

/// Converts ECF position [X, Y, Z] in km to WGS-84 Geodetic Latitude (deg), Longitude (deg), and Altitude (km)
pub fn ecf_to_geodetic(ecf: [f64; 3]) -> (f64, f64, f64) {
    let a = 6378.137; // WGS-84 equatorial radius in km
    let f = 1.0 / 298.257223563;
    let b = a * (1.0 - f);
    let e2 = (a * a - b * b) / (a * a);
    let ep2 = (a * a - b * b) / (b * b);

    let x = ecf[0];
    let y = ecf[1];
    let z = ecf[2];
    let p = (x * x + y * y).sqrt();

    let lon = y.atan2(x).to_degrees();

    if p < 1e-6 {
        let lat = if z >= 0.0 { 90.0 } else { -90.0 };
        let alt_km = z.abs() - b;
        return (lat, lon, alt_km);
    }

    let theta = (z * a).atan2(p * b);
    let lat_rad = (z + ep2 * b * theta.sin().powi(3)).atan2(p - e2 * a * theta.cos().powi(3));
    let lat = lat_rad.to_degrees();

    let n = a / (1.0 - e2 * lat_rad.sin().powi(2)).sqrt();
    let alt_km = p / lat_rad.cos() - n;

    (lat, lon, alt_km)
}

/// Converts ECI velocity vector to ECF velocity vector accounting for Earth rotation
pub fn eci_to_ecf_velocity(eci_pos: [f64; 3], eci_vel: [f64; 3], lst_rad: f64) -> [f64; 3] {
    let omega_e = 7.2921151467e-5; // Earth rotation angular velocity in rad/s
    let vx_eff = eci_vel[0] + omega_e * eci_pos[1];
    let vy_eff = eci_vel[1] - omega_e * eci_pos[0];
    let vz_eff = eci_vel[2];

    let vx_ecf = vx_eff * lst_rad.cos() + vy_eff * lst_rad.sin();
    let vy_ecf = -vx_eff * lst_rad.sin() + vy_eff * lst_rad.cos();
    let vz_ecf = vz_eff;

    [vx_ecf, vy_ecf, vz_ecf]
}

const WGS84_A_KM: f64 = 6378.137;
const WGS84_B_KM: f64 = 6_356.752_314_2;
const WGS84_E2: f64 = 1.0 - (WGS84_B_KM * WGS84_B_KM) / (WGS84_A_KM * WGS84_A_KM);

/// WGS-84 geocentric radius (km) at a geocentric latitude in radians
pub fn earth_radius_at_geocentric_lat_km(phi_gc_rad: f64) -> f64 {
    let (s, c) = phi_gc_rad.sin_cos();
    WGS84_A_KM * WGS84_B_KM
        / (WGS84_B_KM * WGS84_B_KM * c * c + WGS84_A_KM * WGS84_A_KM * s * s).sqrt()
}

/// WGS-84 geocentric radius (km) at a geodetic latitude in degrees
pub fn local_earth_radius_km(lat_deg: f64) -> f64 {
    let phi_gc = ((1.0 - WGS84_E2) * lat_deg.to_radians().tan()).atan();
    earth_radius_at_geocentric_lat_km(phi_gc)
}

/// Perigee and apogee altitudes (km) above the WGS-84 ellipsoid for mean/osculating elements.
/// The apsides lie at geocentric latitudes +/- asin(sin i sin w), which share one ellipsoid radius.
pub fn apsis_altitudes_km(a_km: f64, ecc: f64, inc_deg: f64, argp_deg: f64) -> (f64, f64) {
    let phi_gc = (inc_deg.to_radians().sin() * argp_deg.to_radians().sin()).asin();
    let re = earth_radius_at_geocentric_lat_km(phi_gc);
    (a_km * (1.0 - ecc) - re, a_km * (1.0 + ecc) - re)
}

/// Computes the sub-satellite footprint coverage circle radius on Earth in km,
/// using the local geocentric radius at the sub-satellite geodetic latitude
pub fn calculate_footprint_radius(alt_km: f64, lat_deg: f64) -> f64 {
    let re = local_earth_radius_km(lat_deg);
    let safe_alt = alt_km.max(0.0);
    let cos_val = (re / (re + safe_alt)).clamp(-1.0, 1.0);
    re * cos_val.acos()
}

/// Calculates 36 boundary ring coordinates forming a sub-satellite ground footprint circle on Earth
pub fn generate_footprint_polygon_coords(
    lat_deg: f64,
    lon_deg: f64,
    radius_km: f64,
) -> Vec<Vec<[f64; 3]>> {
    let ang_dist = radius_km / local_earth_radius_km(lat_deg);
    let lat0 = lat_deg.to_radians();
    let lon0 = lon_deg.to_radians();

    let mut ring = Vec::with_capacity(37);
    for step in 0..=36 {
        let azimuth = (step as f64 * 10.0).to_radians();
        let lat_rad =
            (lat0.sin() * ang_dist.cos() + lat0.cos() * ang_dist.sin() * azimuth.cos()).asin();
        let dlon = (azimuth.sin() * ang_dist.sin() * lat0.cos())
            .atan2(ang_dist.cos() - lat0.sin() * lat_rad.sin());
        let mut lon = (lon0 + dlon).to_degrees();

        if lon > 180.0 {
            lon -= 360.0;
        } else if lon < -180.0 {
            lon += 360.0;
        }
        let lat = lat_rad.to_degrees();

        ring.push([lon, lat, 0.0]);
    }

    vec![ring]
}

/// True when the footprint ring neither encloses a pole nor crosses the anti-meridian,
/// i.e. when a single unsplit GeoJSON Polygon ring represents it correctly.
pub fn footprint_ring_is_simple(lat_deg: f64, lon_deg: f64, radius_km: f64) -> bool {
    let ang_deg = (radius_km / local_earth_radius_km(lat_deg)).to_degrees();
    // Polar guard first: |phi0| + sigma >= 90 deg means the cap contains a pole, so the
    // longitude half-width division below is never evaluated.
    if lat_deg.abs() + ang_deg >= 90.0 {
        return false;
    }
    // Max longitude half-width of a spherical cap: asin(sin(r) / cos(lat))
    let half_width = (ang_deg.to_radians().sin() / lat_deg.to_radians().cos())
        .min(1.0)
        .asin()
        .to_degrees();
    lon_deg - half_width > -180.0 && lon_deg + half_width < 180.0
}

/// Splits continuous coordinates across the ±180° Anti-Meridian line to prevent rendering streaks
pub fn build_anti_meridian_geojson_geometry(points: &[[f64; 3]]) -> GeoJsonGeometry {
    if points.is_empty() {
        return GeoJsonGeometry {
            r#type: "LineString".to_string(),
            coordinates: serde_json::json!([]),
        };
    }

    let mut segments: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut current_segment: Vec<[f64; 3]> = Vec::new();

    for pt in points {
        if let Some(last_pt) = current_segment.last() {
            let lon_diff = (pt[0] - last_pt[0]).abs();
            if lon_diff > 180.0 && !current_segment.is_empty() {
                segments.push(current_segment);
                current_segment = Vec::new();
            }
        }
        current_segment.push(*pt);
    }
    if !current_segment.is_empty() {
        segments.push(current_segment);
    }

    if segments.len() == 1 {
        GeoJsonGeometry {
            r#type: "LineString".to_string(),
            coordinates: serde_json::json!(segments[0]),
        }
    } else {
        GeoJsonGeometry {
            r#type: "MultiLineString".to_string(),
            coordinates: serde_json::json!(segments),
        }
    }
}

/// Generates a full orbital 3D ground track trajectory, GeoJSON Anti-Meridian line, Footprint Polygon, and CZML
pub fn generate_ground_track(
    satellite: &Satellite,
    start_time: DateTime<Utc>,
    duration_minutes: usize,
    step_seconds: usize,
    include_geojson: bool,
    include_czml: bool,
    cancel: &CancellationToken,
) -> Result<GroundTrackResponse, AppError> {
    let eph = Ephemeris::from_satellite(satellite)?;
    let mean_motion_revs_day = eph.mean_motion_revs_day;
    let orbital_period_minutes = if mean_motion_revs_day > 0.0 {
        1440.0 / mean_motion_revs_day
    } else {
        90.0
    };

    if include_czml {
        czml::check_step(step_seconds)?;
    }
    let Samples {
        points: trajectory,
        dropped: dropped_samples,
    } = eph.range(start_time, duration_minutes * 60, step_seconds, cancel)?;
    let geojson_coords: Vec<[f64; 3]> = if include_geojson {
        trajectory
            .iter()
            .map(|p| [p.lon, p.lat, p.alt_km])
            .collect()
    } else {
        Vec::new()
    };

    // Footprint is instantaneous: evaluated at the first trajectory point.
    let footprint_radius_km = trajectory
        .first()
        .map_or(0.0, |p| calculate_footprint_radius(p.alt_km, p.lat));

    let (geojson, footprint_polygon) = if include_geojson {
        let geometry = build_anti_meridian_geojson_geometry(&geojson_coords);
        let feature = GeoJsonFeature {
            r#type: "Feature".to_string(),
            geometry,
            properties: serde_json::json!({
                "satelliteId": satellite.id,
                "satelliteName": satellite.name,
                "periodMinutes": orbital_period_minutes,
                "footprintRadiusKm": footprint_radius_km,
            }),
        };

        let footprint_poly = if let Some(first_pt) = trajectory
            .first()
            .filter(|p| footprint_ring_is_simple(p.lat, p.lon, footprint_radius_km))
        {
            let poly_ring =
                generate_footprint_polygon_coords(first_pt.lat, first_pt.lon, footprint_radius_km);
            Some(GeoJsonFeature {
                r#type: "Feature".to_string(),
                geometry: GeoJsonGeometry {
                    r#type: "Polygon".to_string(),
                    coordinates: serde_json::json!(poly_ring),
                },
                properties: serde_json::json!({
                    "satelliteId": satellite.id,
                    "satelliteName": satellite.name,
                    "footprintRadiusKm": footprint_radius_km,
                    "centerLat": first_pt.lat,
                    "centerLon": first_pt.lon,
                }),
            })
        } else {
            None
        };

        (Some(feature), footprint_poly)
    } else {
        (None, None)
    };

    let czml = if include_czml {
        Some(czml::trajectory_document(
            satellite.id,
            &satellite.name,
            &trajectory,
        ))
    } else {
        None
    };

    Ok(GroundTrackResponse {
        satellite_id: satellite.id,
        satellite_name: satellite.name.clone(),
        orbital_period_minutes,
        footprint_radius_km,
        duration_minutes,
        step_seconds,
        trajectory,
        dropped_samples,
        geojson,
        footprint_polygon,
        czml,
    })
}

/// Mean obliquity of the ecliptic (spec 4.1) at `d` days TT from J2000.0, in radians
pub fn mean_obliquity_rad(d_tt: f64) -> f64 {
    (23.439 - 0.00000036 * d_tt).to_radians()
}

/// Computes low-precision Sun ECI position vector [x, y, z] in km
pub fn calculate_sun_position_eci(time: DateTime<Utc>) -> [f64; 3] {
    let d = julian_date_tt(time) - 2451545.0;

    let l_deg = (280.459 + 0.98564736 * d) % 360.0;
    let g_deg = ((357.529 + 0.98560028 * d) % 360.0).to_radians();

    let lambda_deg = l_deg + 1.915 * g_deg.sin() + 0.020 * (2.0 * g_deg).sin();
    let lambda_rad = lambda_deg.to_radians();

    let eps_rad = mean_obliquity_rad(d);

    let r_au = 1.00014 - 0.01671 * g_deg.cos() - 0.00014 * (2.0 * g_deg).cos();
    let r_km = r_au * 149_597_870.7;

    let x = r_km * lambda_rad.cos();
    let y = r_km * eps_rad.cos() * lambda_rad.sin();
    let z = r_km * eps_rad.sin() * lambda_rad.sin();

    [x, y, z]
}

/// Computes solar shadow geometry and observer twilight state
pub fn calculate_illumination(
    satellite: &Satellite,
    lat: f64,
    lon: f64,
    alt: f64,
    time: DateTime<Utc>,
) -> Result<IlluminationResponse, AppError> {
    let line1 = normalize_tle_line(&satellite.tle.line_one, '1');
    let line2 = normalize_tle_line(&satellite.tle.line_two, '2');

    let elements = Elements::from_tle(
        Some(satellite.name.clone()),
        line1.as_bytes(),
        line2.as_bytes(),
    )
    .map_err(|e| AppError::Sgp4Error(format!("Failed to parse TLE: {:?}", e)))?;

    let constants = Constants::from_elements(&elements)
        .map_err(|e| AppError::Sgp4Error(format!("Constants error: {:?}", e)))?;

    let epoch_dt = DateTime::<Utc>::from_naive_utc_and_offset(elements.datetime, Utc);
    let minutes_since_epoch = (time - epoch_dt).num_milliseconds() as f64 / 60000.0;

    let prediction = constants
        .propagate(minutes_since_epoch)
        .map_err(|e| AppError::Sgp4Error(format!("SGP4 propagation error: {:?}", e)))?;

    let sat_eci = prediction.position;
    let sun_eci = calculate_sun_position_eci(time);

    let rs_km = 696_340.0;

    let r_sat_mag =
        (sat_eci[0] * sat_eci[0] + sat_eci[1] * sat_eci[1] + sat_eci[2] * sat_eci[2]).sqrt();
    // Oblate Earth: local radius at the satellite's geocentric latitude
    let re_km = earth_radius_at_geocentric_lat_km((sat_eci[2] / r_sat_mag).asin());
    let d_vec = [
        sun_eci[0] - sat_eci[0],
        sun_eci[1] - sat_eci[1],
        sun_eci[2] - sat_eci[2],
    ];
    let d_mag = (d_vec[0] * d_vec[0] + d_vec[1] * d_vec[1] + d_vec[2] * d_vec[2]).sqrt();

    let theta_e = (re_km / r_sat_mag).asin();
    let theta_s = (rs_km / d_mag).asin();

    let dot_prod = (-sat_eci[0] * d_vec[0] - sat_eci[1] * d_vec[1] - sat_eci[2] * d_vec[2])
        / (r_sat_mag * d_mag);
    let theta = dot_prod.clamp(-1.0, 1.0).acos();

    let lighting_state = if theta < theta_e - theta_s {
        LightingState::Umbra
    } else if (theta_e - theta_s).abs() <= theta && theta < theta_e + theta_s {
        LightingState::Penumbra
    } else {
        LightingState::FullSunlight
    };

    let gmst = calculate_local_sidereal_time(time, 0.0);
    let sun_ecf = eci_to_ecf(sun_eci, gmst);
    let sun_look = ecf_to_look_angles(lat, lon, alt / 1000.0, sun_ecf);
    let observer_sun_elevation_deg = sun_look.elevation;

    let observer_twilight_state = if observer_sun_elevation_deg > 0.0 {
        ObserverTwilightState::Daylight
    } else if observer_sun_elevation_deg >= -6.0 {
        ObserverTwilightState::CivilTwilight
    } else if observer_sun_elevation_deg >= -12.0 {
        ObserverTwilightState::NauticalTwilight
    } else if observer_sun_elevation_deg >= -18.0 {
        ObserverTwilightState::AstronomicalTwilight
    } else {
        ObserverTwilightState::Night
    };

    let sat_ecf = eci_to_ecf(sat_eci, gmst);
    let obs_look = ecf_to_look_angles(lat, lon, alt / 1000.0, sat_ecf);

    let is_visibly_observable = lighting_state != LightingState::Umbra
        && observer_sun_elevation_deg <= -6.0
        && obs_look.elevation > 0.0;
    Ok(IlluminationResponse {
        satellite_id: satellite.id,
        satellite_name: satellite.name.clone(),
        lighting_state,
        observer_twilight_state,
        observer_sun_elevation_deg,
        is_visibly_observable,
    })
}

/// Foster 2D collision probability for a Gaussian encounter-plane covariance given by its
/// principal axes. Integrates the density over the hard-body disk with the erf reduction of
/// spec 10.5 (substitution u1 = R sin(t) removes the endpoint singularity; Simpson in t).
/// All lengths in metres; (m1, m2) is the miss vector in the principal-axis frame.
pub fn foster_pc_principal_axes(r_hbr: f64, sigma_1: f64, sigma_2: f64, m1: f64, m2: f64) -> f64 {
    const STEPS: usize = 20_000; // even
    let s2 = sigma_2 * std::f64::consts::SQRT_2;
    let m2 = m2.abs();
    let integrand = |t: f64| {
        let (sin_t, cos_t) = t.sin_cos();
        let u1 = r_hbr * sin_t;
        let w = r_hbr * cos_t;
        let g = (-(u1 - m1).powi(2) / (2.0 * sigma_1 * sigma_1)).exp()
            / (sigma_1 * (2.0 * std::f64::consts::PI).sqrt());
        // 0.5 [erf((w - m2)/s2) - erf((-w - m2)/s2)], rewritten with erfc to avoid cancellation
        let band = 0.5 * (libm::erfc((m2 - w) / s2) - libm::erfc((m2 + w) / s2));
        g * band * r_hbr * cos_t
    };
    let (lo, hi) = (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
    let h = (hi - lo) / STEPS as f64;
    let mut sum = integrand(lo) + integrand(hi);
    for k in 1..STEPS {
        let w = if k % 2 == 1 { 4.0 } else { 2.0 };
        sum += w * integrand(lo + k as f64 * h);
    }
    (sum * h / 3.0).clamp(0.0, 1.0)
}

/// Computes Foster 2D encounter-plane collision probability (Pc) from principal-axis
/// 1-sigma uncertainties (sigma_1_m, sigma_2_m) of the combined encounter-plane covariance.
/// `miss_angle_deg` is the angle of the miss vector from the sigma_1 axis.
/// The covariance is never circularized (spec 10.5).
pub fn calculate_foster_collision_probability(
    miss_distance_km: f64,
    relative_velocity_kms: f64,
    hard_body_radius_m: f64,
    sigma_1_m: f64,
    sigma_2_m: f64,
    miss_angle_deg: f64,
) -> CollisionProbabilityResponse {
    let d_m = (miss_distance_km * 1000.0).max(0.0);
    let r_hbr = hard_body_radius_m.max(0.1);
    let alpha = miss_angle_deg.to_radians();
    let pc = foster_pc_principal_axes(
        r_hbr,
        sigma_1_m,
        sigma_2_m,
        d_m * alpha.cos(),
        d_m * alpha.sin(),
    );

    let (risk_category, recommendation) = if pc >= 1e-4 {
        (
            "Critical (Pc >= 1e-4)".to_string(),
            "Immediate collision avoidance maneuver (CAM) planning recommended.".to_string(),
        )
    } else if pc >= 1e-5 {
        (
            "Elevated (1e-5 <= Pc < 1e-4)".to_string(),
            "Elevated conjunction risk; monitor tracking updates closely.".to_string(),
        )
    } else if pc >= 1e-7 {
        (
            "Low (1e-7 <= Pc < 1e-5)".to_string(),
            "Conjunction watch active; routine tracking monitoring.".to_string(),
        )
    } else {
        (
            "Negligible (Pc < 1e-7)".to_string(),
            "Collision probability negligible; no maneuver action required.".to_string(),
        )
    };

    CollisionProbabilityResponse {
        miss_distance_km,
        relative_velocity_kms,
        hard_body_radius_m: r_hbr,
        sigma_1_m,
        sigma_2_m,
        miss_angle_deg,
        collision_probability: pc,
        risk_category,
        recommendation,
    }
}

/// Multi-threaded conjunction and satellite collision radar using Rayon `par_iter()`
pub fn find_conjunctions(
    satellites: &[Satellite],
    start_time: DateTime<Utc>,
    max_distance_km: f64,
    duration_hours: i64,
) -> ConjunctionSearchResponse {
    let duration_hrs = duration_hours.clamp(1, 72);
    // Fixed 60 s grid; every sampled local minimum is refined below. Coarser grids can
    // step over an entire encounter, so the step is not user-configurable.
    let step_mins: i64 = 1;
    let total_steps = (duration_hrs * 60) / step_mins;

    // Two records with the same NORAD ID are one object, not a conjunction.
    let norad = |s: &Satellite| {
        normalize_tle_line(&s.tle.line_one, '1')
            .get(2..7)
            .map(str::to_owned)
    };
    let mut pairs = Vec::new();
    for i in 0..satellites.len() {
        for j in (i + 1)..satellites.len() {
            if norad(&satellites[i]) != norad(&satellites[j]) {
                pairs.push((&satellites[i], &satellites[j]));
            }
        }
    }

    let matches: Vec<ConjunctionMatch> = pairs
        .par_iter()
        .filter_map(|(sat_a, sat_b)| {
            let line1_a = normalize_tle_line(&sat_a.tle.line_one, '1');
            let line2_a = normalize_tle_line(&sat_a.tle.line_two, '2');
            let elem_a = Elements::from_tle(
                Some(sat_a.name.clone()),
                line1_a.as_bytes(),
                line2_a.as_bytes(),
            )
            .ok()?;
            let const_a = Constants::from_elements(&elem_a).ok()?;
            let epoch_a = DateTime::<Utc>::from_naive_utc_and_offset(elem_a.datetime, Utc);

            let line1_b = normalize_tle_line(&sat_b.tle.line_one, '1');
            let line2_b = normalize_tle_line(&sat_b.tle.line_two, '2');
            let elem_b = Elements::from_tle(
                Some(sat_b.name.clone()),
                line1_b.as_bytes(),
                line2_b.as_bytes(),
            )
            .ok()?;
            let const_b = Constants::from_elements(&elem_b).ok()?;
            let epoch_b = DateTime::<Utc>::from_naive_utc_and_offset(elem_b.datetime, Utc);

            let mut min_dist_km = f64::MAX;
            let mut closest_time = start_time;
            let mut rel_vel_kms = 0.0;
            let mut samples: Vec<(i64, f64)> = Vec::new();

            for step in 0..=total_steps {
                let current_time = start_time + chrono::Duration::minutes(step * step_mins);
                let mins_a = (current_time - epoch_a).num_milliseconds() as f64 / 60000.0;
                let mins_b = (current_time - epoch_b).num_milliseconds() as f64 / 60000.0;

                if let (Ok(pred_a), Ok(pred_b)) =
                    (const_a.propagate(mins_a), const_b.propagate(mins_b))
                {
                    let dx = pred_a.position[0] - pred_b.position[0];
                    let dy = pred_a.position[1] - pred_b.position[1];
                    let dz = pred_a.position[2] - pred_b.position[2];
                    let dist_km = (dx * dx + dy * dy + dz * dz).sqrt();
                    samples.push((step, dist_km));

                    if dist_km < min_dist_km {
                        min_dist_km = dist_km;
                        closest_time = current_time;

                        let dvx = pred_a.velocity[0] - pred_b.velocity[0];
                        let dvy = pred_a.velocity[1] - pred_b.velocity[1];
                        let dvz = pred_a.velocity[2] - pred_b.velocity[2];
                        rel_vel_kms = (dvx * dvx + dvy * dvy + dvz * dvz).sqrt();
                    }
                }
            }

            let dist_at = |mins_from_start: f64| -> Option<(f64, f64)> {
                let ms = (mins_from_start * 60000.0) as i64;
                let t = start_time + chrono::Duration::milliseconds(ms);
                let pa = const_a
                    .propagate((t - epoch_a).num_milliseconds() as f64 / 60000.0)
                    .ok()?;
                let pb = const_b
                    .propagate((t - epoch_b).num_milliseconds() as f64 / 60000.0)
                    .ok()?;
                let d = (0..3)
                    .map(|k| (pa.position[k] - pb.position[k]).powi(2))
                    .sum::<f64>();
                let v = (0..3)
                    .map(|k| (pa.velocity[k] - pb.velocity[k]).powi(2))
                    .sum::<f64>();
                Some((d.sqrt(), v.sqrt()))
            };
            let gr = 0.618_033_988_75;
            for w in 0..samples.len() {
                let prev = if w > 0 { samples[w - 1].1 } else { f64::MAX };
                let next = samples.get(w + 1).map_or(f64::MAX, |x| x.1);
                if samples[w].1 > prev || samples[w].1 > next {
                    continue;
                }
                let centre = (samples[w].0 * step_mins) as f64;
                let (mut lo, mut hi) = (centre - step_mins as f64, centre + step_mins as f64);
                while hi - lo > 1.0 / 600.0 {
                    let m1 = hi - gr * (hi - lo);
                    let m2 = lo + gr * (hi - lo);
                    let d1 = dist_at(m1).map_or(f64::MAX, |x| x.0);
                    let d2 = dist_at(m2).map_or(f64::MAX, |x| x.0);
                    if d1 < d2 {
                        hi = m2
                    } else {
                        lo = m1
                    }
                }
                let tca = (lo + hi) / 2.0;
                if let Some((d, v)) = dist_at(tca) {
                    if d < min_dist_km {
                        min_dist_km = d;
                        rel_vel_kms = v;
                        closest_time =
                            start_time + chrono::Duration::milliseconds((tca * 60000.0) as i64);
                    }
                }
            }

            if min_dist_km <= max_distance_km {
                Some(ConjunctionMatch {
                    satellite_a: SatelliteSummary {
                        id: sat_a.id,
                        name: sat_a.name.clone(),
                    },
                    satellite_b: SatelliteSummary {
                        id: sat_b.id,
                        name: sat_b.name.clone(),
                    },
                    closest_approach_time: closest_time,
                    min_distance_km: min_dist_km,
                    relative_velocity_kms: rel_vel_kms,
                    // No covariance is available from TLEs, so no Pc is reported here.
                    collision_probability: None,
                    risk_category: None,
                })
            } else {
                None
            }
        })
        .collect();

    let mut sorted_matches = matches;
    sorted_matches.sort_by(|a, b| {
        a.min_distance_km
            .partial_cmp(&b.min_distance_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    ConjunctionSearchResponse {
        search_duration_hours: duration_hrs,
        max_distance_km,
        conjunctions_found: sorted_matches.len(),
        results: sorted_matches,
    }
}

/// Relativistic received frequency (spec 8.2): time dilation of transmitter and receiver in the
/// inertial frame, line-of-sight term and gravitational blue shift. Speeds are inertial (ECI)
/// speeds in km/s, radii in km.
pub fn relativistic_received_freq_hz(
    f0_hz: f64,
    range_rate_kms: f64,
    sat_speed_eci_kms: f64,
    obs_speed_eci_kms: f64,
    r_obs_km: f64,
    r_sat_km: f64,
) -> f64 {
    const C_KMS: f64 = 299_792.458;
    const MU_KM3_S2: f64 = 398_600.441_8;
    let beta_sat = sat_speed_eci_kms / C_KMS;
    let beta_obs = obs_speed_eci_kms / C_KMS;
    let grav = MU_KM3_S2 / (C_KMS * C_KMS) * (1.0 / r_obs_km - 1.0 / r_sat_km);
    f0_hz * ((1.0 - beta_sat * beta_sat) / (1.0 - beta_obs * beta_obs)).sqrt()
        / (1.0 + range_rate_kms / C_KMS)
        * (1.0 + grav)
}

/// Computes range rate (km/s) and real-time RF Doppler frequency shift (Hz) for a ground observer
pub fn calculate_doppler_shift(
    satellite: &Satellite,
    center_freq_hz: f64,
    lat: f64,
    lon: f64,
    alt_km: f64,
    time: DateTime<Utc>,
    relativistic: bool,
) -> Result<DopplerResponse, AppError> {
    let line1 = normalize_tle_line(&satellite.tle.line_one, '1');
    let line2 = normalize_tle_line(&satellite.tle.line_two, '2');

    let elements = Elements::from_tle(
        Some(satellite.name.clone()),
        line1.as_bytes(),
        line2.as_bytes(),
    )
    .map_err(|e| AppError::Sgp4Error(format!("Failed to parse TLE: {:?}", e)))?;

    let constants = Constants::from_elements(&elements)
        .map_err(|e| AppError::Sgp4Error(format!("Constants error: {:?}", e)))?;

    let epoch_dt = DateTime::<Utc>::from_naive_utc_and_offset(elements.datetime, Utc);
    let minutes_since_epoch = (time - epoch_dt).num_milliseconds() as f64 / 60000.0;

    let prediction: Prediction = constants
        .propagate(minutes_since_epoch)
        .map_err(|e| AppError::Sgp4Error(format!("SGP4 propagation error: {:?}", e)))?;

    let lst = calculate_local_sidereal_time(time, 0.0);
    let sat_ecf = eci_to_ecf(prediction.position, lst);
    let sat_vel_ecf = eci_to_ecf_velocity(prediction.position, prediction.velocity, lst);

    let lat_rad = lat.to_radians();
    let lon_rad = lon.to_radians();

    let re = 6378.137; // WGS84 Equatorial radius (km)
    let f = 1.0 / 298.257223563;
    let c_flat = 1.0 / (1.0 - (2.0 * f - f * f) * lat_rad.sin() * lat_rad.sin()).sqrt();

    let obs_x = (re * c_flat + alt_km) * lat_rad.cos() * lon_rad.cos();
    let obs_y = (re * c_flat + alt_km) * lat_rad.cos() * lon_rad.sin();
    let obs_z = (re * (c_flat * (1.0 - f) * (1.0 - f)) + alt_km) * lat_rad.sin();

    // Range vector in ECF
    let rx = sat_ecf[0] - obs_x;
    let ry = sat_ecf[1] - obs_y;
    let rz = sat_ecf[2] - obs_z;
    let range_km = (rx * rx + ry * ry + rz * rz).sqrt();

    if range_km < 1e-6 {
        return Err(AppError::InternalServerError(
            "Zero range calculation".to_string(),
        ));
    }

    // Range rate (dot product of range vector and satellite relative velocity vector in ECF)
    let range_rate_kms =
        (rx * sat_vel_ecf[0] + ry * sat_vel_ecf[1] + rz * sat_vel_ecf[2]) / range_km;

    // Speed of light in km/s
    let c_kms = 299_792.458;
    let corrected_freq_hz = if relativistic {
        let v_sat = prediction
            .velocity
            .iter()
            .map(|v| v * v)
            .sum::<f64>()
            .sqrt();
        // Observer speed in the inertial frame comes from Earth rotation only
        let v_obs = 7.2921151467e-5 * obs_x.hypot(obs_y);
        let r_sat = (sat_ecf[0].powi(2) + sat_ecf[1].powi(2) + sat_ecf[2].powi(2)).sqrt();
        let r_obs = (obs_x * obs_x + obs_y * obs_y + obs_z * obs_z).sqrt();
        relativistic_received_freq_hz(center_freq_hz, range_rate_kms, v_sat, v_obs, r_obs, r_sat)
    } else {
        center_freq_hz - center_freq_hz * (range_rate_kms / c_kms)
    };
    let doppler_shift_hz = corrected_freq_hz - center_freq_hz;

    let signal_direction = if range_rate_kms < -1e-5 {
        "Approaching (Blue Shift)".to_string()
    } else if range_rate_kms > 1e-5 {
        "Receding (Red Shift)".to_string()
    } else {
        "Stationary / Zero Doppler".to_string()
    };

    Ok(DopplerResponse {
        satellite_id: satellite.id,
        satellite_name: satellite.name.clone(),
        center_freq_hz,
        range_rate_kms,
        doppler_shift_hz,
        corrected_freq_hz,
        signal_direction,
    })
}

/// Computes low-precision geocentric Moon position vector [X, Y, Z] in ECI (km) using Meeus Chapter 47 algorithm
pub fn calculate_lunar_position_eci(time: DateTime<Utc>) -> [f64; 3] {
    let d = julian_date_tt(time) - 2451545.0;

    // Mean longitude of the Moon in degrees
    let l_prime = (218.316 + 13.176396 * d) % 360.0;
    // Mean anomaly of the Moon in degrees
    let m_prime = (134.963 + 13.064993 * d) % 360.0;
    // Argument of latitude of the Moon in degrees
    let f = (93.272 + 13.229350 * d) % 360.0;

    let m_prime_rad = m_prime.to_radians();
    let f_rad = f.to_radians();

    // Ecliptic longitude & latitude in radians
    let lambda = (l_prime + 6.289 * m_prime_rad.sin()).to_radians();
    let beta = (5.128 * f_rad.sin()).to_radians();
    // Distance in km
    let r_km = 385_001.0 - 20_905.0 * m_prime_rad.cos();

    let eps_rad = mean_obliquity_rad(d);

    let x = r_km * beta.cos() * lambda.cos();
    let y = r_km * (beta.cos() * lambda.sin() * eps_rad.cos() - beta.sin() * eps_rad.sin());
    let z = r_km * (beta.cos() * lambda.sin() * eps_rad.sin() + beta.sin() * eps_rad.cos());

    [x, y, z]
}

/// Multi-threaded evaluation of solar or lunar satellite transits across a multi-day forecast window
#[allow(clippy::too_many_arguments)]
pub fn find_transits(
    target: TransitTarget,
    satellites: &[Satellite],
    lat: f64,
    lon: f64,
    alt_km: f64,
    start_time: DateTime<Utc>,
    duration_days: usize,
    max_angular_separation_deg: f64,
) -> Result<TransitPredictionResponse, AppError> {
    let forecast_hours = duration_days * 24;
    let total_minutes = forecast_hours * 60;

    let results: Vec<TransitMatch> = satellites
        .par_iter()
        .flat_map(|sat| {
            let mut matches = Vec::new();
            let line1 = normalize_tle_line(&sat.tle.line_one, '1');
            let line2 = normalize_tle_line(&sat.tle.line_two, '2');

            let elements = match Elements::from_tle(
                Some(sat.name.clone()),
                line1.as_bytes(),
                line2.as_bytes(),
            ) {
                Ok(e) => e,
                Err(_) => return matches,
            };
            let constants = match Constants::from_elements(&elements) {
                Ok(c) => c,
                Err(_) => return matches,
            };
            let epoch_dt = DateTime::<Utc>::from_naive_utc_and_offset(elements.datetime, Utc);

            // Angular separation (deg) and target elevation (deg) at `t`; None when SGP4 fails or
            // either body is below the horizon.
            let sample = |t: DateTime<Utc>| -> Option<(f64, f64)> {
                let minutes_since_epoch = (t - epoch_dt).num_milliseconds() as f64 / 60000.0;
                let prediction = constants.propagate(minutes_since_epoch).ok()?;
                let gmst = calculate_local_sidereal_time(t, 0.0);
                let sat_look =
                    ecf_to_look_angles(lat, lon, alt_km, eci_to_ecf(prediction.position, gmst));
                if sat_look.elevation <= 0.0 {
                    return None;
                }
                let target_eci = match target {
                    TransitTarget::Sun => calculate_sun_position_eci(t),
                    TransitTarget::Moon => calculate_lunar_position_eci(t),
                };
                let target_look =
                    ecf_to_look_angles(lat, lon, alt_km, eci_to_ecf(target_eci, gmst));
                if target_look.elevation <= 0.0 {
                    return None;
                }
                let unit = |az_deg: f64, el_deg: f64| {
                    let (az, el) = (az_deg.to_radians(), el_deg.to_radians());
                    [el.cos() * az.sin(), el.cos() * az.cos(), el.sin()]
                };
                let u_sat = unit(sat_look.azimuth, sat_look.elevation);
                let u_tgt = unit(target_look.azimuth, target_look.elevation);
                let dot = (u_sat[0] * u_tgt[0] + u_sat[1] * u_tgt[1] + u_sat[2] * u_tgt[2])
                    .clamp(-1.0, 1.0);
                Some((dot.acos().to_degrees(), target_look.elevation))
            };
            let inside = |t: DateTime<Utc>| {
                sample(t).is_some_and(|(sep, _)| sep <= max_angular_separation_deg)
            };
            let step_time = |m: usize| start_time + chrono::Duration::minutes(m as i64);

            // Detection runs on a 1-minute grid. Each contiguous run of in-threshold samples is one
            // transit whose edges and closest approach are then refined to ~0.1 s.
            let mut run: Option<(usize, usize, usize, f64)> = None; // first, last, best, best_sep
            let mut runs = Vec::new();
            for min_step in 0..total_minutes {
                let hit = sample(step_time(min_step))
                    .filter(|(sep, _)| *sep <= max_angular_separation_deg)
                    .map(|(sep, _)| sep);
                match (hit, run.as_mut()) {
                    (Some(sep), Some(r)) => {
                        r.1 = min_step;
                        if sep < r.3 {
                            r.2 = min_step;
                            r.3 = sep;
                        }
                    }
                    (Some(sep), None) => run = Some((min_step, min_step, min_step, sep)),
                    (None, _) => runs.extend(run.take()),
                }
            }
            runs.extend(run.take());

            for (first, last, best, _) in runs {
                // Bisect the in/out boundary between an outside and an inside instant
                let edge = |mut out: DateTime<Utc>, mut inn: DateTime<Utc>| {
                    while (inn - out).num_milliseconds().abs() > 100 {
                        let mid = out + (inn - out) / 2;
                        if inside(mid) {
                            inn = mid;
                        } else {
                            out = mid;
                        }
                    }
                    inn
                };
                let one_min = chrono::Duration::minutes(1);
                let transit_start = if first == 0 {
                    step_time(0)
                } else {
                    edge(step_time(first) - one_min, step_time(first))
                };
                let transit_end = if last + 1 >= total_minutes {
                    step_time(last)
                } else {
                    edge(step_time(last) + one_min, step_time(last))
                };

                // Ternary search for the closest approach within one minute of the best sample
                let mut lo = (step_time(best) - one_min).max(transit_start);
                let mut hi = (step_time(best) + one_min).min(transit_end);
                let sep_at = |t| sample(t).map_or(f64::INFINITY, |(sep, _)| sep);
                while (hi - lo).num_milliseconds() > 100 {
                    let third = (hi - lo) / 3;
                    let (m1, m2) = (lo + third, hi - third);
                    if sep_at(m1) < sep_at(m2) {
                        hi = m2;
                    } else {
                        lo = m1;
                    }
                }
                let transit_center = lo + (hi - lo) / 2;
                let Some((sep_deg, target_el)) = sample(transit_center) else {
                    continue;
                };

                matches.push(TransitMatch {
                    satellite_id: sat.id,
                    satellite_name: sat.name.clone(),
                    transit_start_utc: transit_start,
                    transit_center_utc: transit_center,
                    transit_end_utc: transit_end,
                    transit_duration_seconds: (transit_end - transit_start).num_milliseconds()
                        as f64
                        / 1000.0,
                    min_angular_separation_deg: (sep_deg * 1000.0).round() / 1000.0,
                    target_elevation_deg: (target_el * 100.0).round() / 100.0,
                });
            }
            matches
        })
        .collect();

    Ok(TransitPredictionResponse {
        target,
        observer_lat: lat,
        observer_lon: lon,
        forecast_days: duration_days,
        transits_found: results.len(),
        results,
    })
}

/// Multi-day ground station pass prediction and scheduling engine
#[allow(clippy::too_many_arguments)]
pub fn find_pass_schedule(
    satellite: &Satellite,
    lat: f64,
    lon: f64,
    alt_m: f64,
    start_time: DateTime<Utc>,
    elevation_threshold_deg: f64,
    duration_days: usize,
    visible_only: bool,
    cancel: &CancellationToken,
) -> Result<PassScheduleResponse, AppError> {
    let days = duration_days.clamp(1, 14);
    let total_minutes = (days * 24 * 60) as i64;
    let threshold = elevation_threshold_deg.clamp(-10.0, 89.0);

    let elev = |t: DateTime<Utc>| {
        calculate_look_angles(satellite, lat, lon, alt_m, t)
            .map(|l| l.elevation)
            .unwrap_or(f64::NEG_INFINITY)
    };

    let mut passes = Vec::new();
    let mut pass_id = 1;
    let mut k: i64 = 0;

    while k < total_minutes {
        if cancel.is_cancelled() {
            return Err(crate::services::ephemeris::cancelled());
        }
        let t_curr = start_time + chrono::Duration::minutes(k);
        let e_curr = elev(t_curr);

        if e_curr >= threshold {
            // Find AOS (crossing threshold from below)
            let mut below = t_curr - chrono::Duration::minutes(1);
            let mut above = t_curr;
            let aos_time = if k == 0 {
                start_time
            } else {
                while (above - below).num_milliseconds() > 1000 {
                    let mid = below + (above - below) / 2;
                    if elev(mid) >= threshold {
                        above = mid;
                    } else {
                        below = mid;
                    }
                }
                above
            };

            let aos_look = calculate_look_angles(satellite, lat, lon, alt_m, aos_time)?;

            // Trace forward to find LOS (crossing threshold back below)
            let mut t_scan = t_curr + chrono::Duration::minutes(1);
            let mut peak_time = t_curr;
            let mut peak_elev = e_curr;

            while k < total_minutes {
                let e_scan = elev(t_scan);
                if e_scan > peak_elev {
                    peak_elev = e_scan;
                    peak_time = t_scan;
                }
                if e_scan < threshold {
                    break;
                }
                t_scan += chrono::Duration::minutes(1);
                k += 1;
            }

            let los_time = if k >= total_minutes {
                start_time + chrono::Duration::minutes(total_minutes)
            } else {
                let mut los_above = t_scan - chrono::Duration::minutes(1);
                let mut los_below = t_scan;
                while (los_below - los_above).num_milliseconds() > 1000 {
                    let mid = los_above + (los_below - los_above) / 2;
                    if elev(mid) >= threshold {
                        los_above = mid;
                    } else {
                        los_below = mid;
                    }
                }
                los_below
            };

            // Refine peak elevation with golden section search around peak_time
            let gr = 0.618_033_988_75;
            let (mut lo_t, mut hi_t) = (
                peak_time - chrono::Duration::minutes(1),
                peak_time + chrono::Duration::minutes(1),
            );
            if lo_t < aos_time {
                lo_t = aos_time;
            }
            if hi_t > los_time {
                hi_t = los_time;
            }

            while (hi_t - lo_t).num_milliseconds() > 1000 {
                let diff_ms = (hi_t - lo_t).num_milliseconds() as f64;
                let m1 = hi_t - chrono::Duration::milliseconds((gr * diff_ms) as i64);
                let m2 = lo_t + chrono::Duration::milliseconds((gr * diff_ms) as i64);
                if elev(m1) > elev(m2) {
                    hi_t = m2;
                } else {
                    lo_t = m1;
                }
            }
            let tca_time = lo_t + (hi_t - lo_t) / 2;
            let tca_look = calculate_look_angles(satellite, lat, lon, alt_m, tca_time)?;
            let los_look = calculate_look_angles(satellite, lat, lon, alt_m, los_time)?;

            let duration_sec = (los_time - aos_time).num_milliseconds() as f64 / 1000.0;

            // Illumination and visibility at TCA
            let is_visible = match calculate_illumination(satellite, lat, lon, alt_m, tca_time) {
                Ok(illum) => illum.is_visibly_observable,
                Err(_) => false,
            };

            let visual_magnitude = if is_visible {
                let sat_eci = satellite_position_eci(satellite, tca_time)?;
                let gamma = phase_angle_rad(
                    sat_eci,
                    observer_position_eci(lat, lon, alt_m / 1000.0, tca_time),
                    calculate_sun_position_eci(tca_time),
                );
                let mag = visual_magnitude(tca_look.range_km, gamma);
                Some((mag * 10.0).round() / 10.0)
            } else {
                None
            };

            if !visible_only || is_visible {
                passes.push(SatellitePass {
                    pass_id,
                    aos_time,
                    aos_azimuth_deg: (aos_look.azimuth * 100.0).round() / 100.0,
                    aos_elevation_deg: (aos_look.elevation * 100.0).round() / 100.0,
                    tca_time,
                    tca_azimuth_deg: (tca_look.azimuth * 100.0).round() / 100.0,
                    max_elevation_deg: (tca_look.elevation * 100.0).round() / 100.0,
                    tca_range_km: (tca_look.range_km * 10.0).round() / 10.0,
                    los_time,
                    los_azimuth_deg: (los_look.azimuth * 100.0).round() / 100.0,
                    los_elevation_deg: (los_look.elevation * 100.0).round() / 100.0,
                    duration_seconds: (duration_sec * 10.0).round() / 10.0,
                    is_visible,
                    visual_magnitude,
                });
                pass_id += 1;
            }
        }
        k += 1;
    }

    Ok(PassScheduleResponse {
        satellite_id: satellite.id,
        satellite_name: satellite.name.clone(),
        observer_lat: lat,
        observer_lon: lon,
        observer_alt_m: alt_m,
        elevation_threshold_deg: threshold,
        forecast_days: days,
        passes_found: passes.len(),
        passes,
        czml: None,
        czml_dropped_samples: None,
    })
}

/// Evaluates relative position and velocity vectors in Local-Vertical Local-Horizontal (LVLH / Hill's) frame
pub fn calculate_relative_motion(
    primary: &Satellite,
    target: &Satellite,
    epoch: DateTime<Utc>,
    duration_minutes: usize,
    step_seconds: usize,
) -> Result<RelativeMotionResponse, AppError> {
    let line1_p = normalize_tle_line(&primary.tle.line_one, '1');
    let line2_p = normalize_tle_line(&primary.tle.line_two, '2');
    let elem_p = Elements::from_tle(
        Some(primary.name.clone()),
        line1_p.as_bytes(),
        line2_p.as_bytes(),
    )
    .map_err(|e| AppError::Sgp4Error(format!("Failed to parse primary TLE: {:?}", e)))?;
    let const_p = Constants::from_elements(&elem_p)
        .map_err(|e| AppError::Sgp4Error(format!("Primary constants error: {:?}", e)))?;
    let epoch_p = DateTime::<Utc>::from_naive_utc_and_offset(elem_p.datetime, Utc);

    let line1_t = normalize_tle_line(&target.tle.line_one, '1');
    let line2_t = normalize_tle_line(&target.tle.line_two, '2');
    let elem_t = Elements::from_tle(
        Some(target.name.clone()),
        line1_t.as_bytes(),
        line2_t.as_bytes(),
    )
    .map_err(|e| AppError::Sgp4Error(format!("Failed to parse target TLE: {:?}", e)))?;
    let const_t = Constants::from_elements(&elem_t)
        .map_err(|e| AppError::Sgp4Error(format!("Target constants error: {:?}", e)))?;
    let epoch_t = DateTime::<Utc>::from_naive_utc_and_offset(elem_t.datetime, Utc);

    let compute_pt = |t: DateTime<Utc>| -> Result<(RelativeMotionPoint, [f64; 3]), AppError> {
        let dt_p = (t - epoch_p).num_milliseconds() as f64 / 60000.0;
        let dt_t = (t - epoch_t).num_milliseconds() as f64 / 60000.0;

        let pred_p = const_p
            .propagate(dt_p)
            .map_err(|e| AppError::Sgp4Error(format!("Primary SGP4 error: {:?}", e)))?;
        let pred_t = const_t
            .propagate(dt_t)
            .map_err(|e| AppError::Sgp4Error(format!("Target SGP4 error: {:?}", e)))?;

        let rp = pred_p.position;
        let vp = pred_p.velocity;
        let rt = pred_t.position;
        let vt = pred_t.velocity;

        let rp_mag = (rp[0] * rp[0] + rp[1] * rp[1] + rp[2] * rp[2]).sqrt();
        if rp_mag < 1.0 {
            return Err(AppError::InternalServerError(
                "Primary satellite orbital radius near zero".to_string(),
            ));
        }

        // Hill's frame unit vectors:
        // Radial e_r = r / |r|
        let er = [rp[0] / rp_mag, rp[1] / rp_mag, rp[2] / rp_mag];

        // Angular momentum h = r x v
        let h_vec = [
            rp[1] * vp[2] - rp[2] * vp[1],
            rp[2] * vp[0] - rp[0] * vp[2],
            rp[0] * vp[1] - rp[1] * vp[0],
        ];
        let h_mag = (h_vec[0] * h_vec[0] + h_vec[1] * h_vec[1] + h_vec[2] * h_vec[2]).sqrt();
        if h_mag < 1e-6 {
            return Err(AppError::InternalServerError(
                "Angular momentum near zero".to_string(),
            ));
        }

        // Cross-track e_c = h / |h|
        let ec = [h_vec[0] / h_mag, h_vec[1] / h_mag, h_vec[2] / h_mag];

        // In-track e_i = e_c x e_r
        let ei = [
            ec[1] * er[2] - ec[2] * er[1],
            ec[2] * er[0] - ec[0] * er[2],
            ec[0] * er[1] - ec[1] * er[0],
        ];

        // Relative position in ECI
        let dr = [rt[0] - rp[0], rt[1] - rp[1], rt[2] - rp[2]];
        let radial_km = dr[0] * er[0] + dr[1] * er[1] + dr[2] * er[2];
        let in_track_km = dr[0] * ei[0] + dr[1] * ei[1] + dr[2] * ei[2];
        let cross_track_km = dr[0] * ec[0] + dr[1] * ec[1] + dr[2] * ec[2];
        let range_km = (dr[0] * dr[0] + dr[1] * dr[1] + dr[2] * dr[2]).sqrt();

        // Orbit angular velocity vector w = h / |r|^2
        let omega = [
            h_vec[0] / (rp_mag * rp_mag),
            h_vec[1] / (rp_mag * rp_mag),
            h_vec[2] / (rp_mag * rp_mag),
        ];
        // Inertial relative velocity
        let dv = [vt[0] - vp[0], vt[1] - vp[1], vt[2] - vp[2]];
        // Apparent velocity in rotating frame: v_rel = dv - (omega x dr)
        let w_cross_dr = [
            omega[1] * dr[2] - omega[2] * dr[1],
            omega[2] * dr[0] - omega[0] * dr[2],
            omega[0] * dr[1] - omega[1] * dr[0],
        ];
        let v_rel = [
            dv[0] - w_cross_dr[0],
            dv[1] - w_cross_dr[1],
            dv[2] - w_cross_dr[2],
        ];

        let v_radial = v_rel[0] * er[0] + v_rel[1] * er[1] + v_rel[2] * er[2];
        let v_in_track = v_rel[0] * ei[0] + v_rel[1] * ei[1] + v_rel[2] * ei[2];
        let v_cross_track = v_rel[0] * ec[0] + v_rel[1] * ec[1] + v_rel[2] * ec[2];

        let range_rate_kms = (dr[0] * dv[0] + dr[1] * dv[1] + dr[2] * dv[2]) / range_km.max(1e-6);

        Ok((
            RelativeMotionPoint {
                timestamp: t,
                radial_km,
                in_track_km,
                cross_track_km,
                range_km,
                range_rate_kms,
            },
            [v_radial, v_in_track, v_cross_track],
        ))
    };

    let (initial_pt, v_components) = compute_pt(epoch)?;

    let rpo_regime = if initial_pt.range_km < 1.0 {
        "Mating / Docking Box (< 1 km)".to_string()
    } else if initial_pt.range_km < 10.0 {
        "Close Proximity / Inspection Zone (1 - 10 km)".to_string()
    } else if initial_pt.range_km < 100.0 {
        "Intermediate Proximity Operations (10 - 100 km)".to_string()
    } else if initial_pt.range_km < 1000.0 {
        "Far-Field Rendezvous Operations (100 - 1000 km)".to_string()
    } else {
        "Co-orbital Drift / Distant Tracking (> 1000 km)".to_string()
    };

    let trajectory = if duration_minutes > 0 {
        let dur_mins = duration_minutes.min(1440);
        let step_s = step_seconds.clamp(5, 300);
        let total_secs = dur_mins * 60;
        let mut pts = Vec::with_capacity(total_secs / step_s + 1);

        for sec in (0..=total_secs).step_by(step_s) {
            let t = epoch + chrono::Duration::seconds(sec as i64);
            if let Ok((pt, _)) = compute_pt(t) {
                pts.push(pt);
            }
        }
        Some(pts)
    } else {
        None
    };

    Ok(RelativeMotionResponse {
        primary_satellite: SatelliteSummary {
            id: primary.id,
            name: primary.name.clone(),
        },
        target_satellite: SatelliteSummary {
            id: target.id,
            name: target.name.clone(),
        },
        epoch,
        relative_distance_km: (initial_pt.range_km * 1000.0).round() / 1000.0,
        range_rate_kms: (initial_pt.range_rate_kms * 10000.0).round() / 10000.0,
        radial_distance_km: (initial_pt.radial_km * 1000.0).round() / 1000.0,
        in_track_distance_km: (initial_pt.in_track_km * 1000.0).round() / 1000.0,
        cross_track_distance_km: (initial_pt.cross_track_km * 1000.0).round() / 1000.0,
        radial_velocity_kms: (v_components[0] * 10000.0).round() / 10000.0,
        in_track_velocity_kms: (v_components[1] * 10000.0).round() / 10000.0,
        cross_track_velocity_kms: (v_components[2] * 10000.0).round() / 10000.0,
        rpo_regime,
        trajectory,
    })
}

/// Brouwer mean semi-major axis a0'' (km) recovered from the TLE Kozai mean motion with the
/// SGP4 initialization (Spacetrack Report #3) and WGS-72 constants.
pub fn brouwer_mean_semi_major_axis_km(elements: &Elements) -> f64 {
    let g = &sgp4::WGS72;
    let n_o = elements.mean_motion * 2.0 * std::f64::consts::PI / 1440.0; // rad/min
    let e = elements.eccentricity;
    let k2 = 0.5 * g.j2;
    let cos_i = elements.inclination.to_radians().cos();
    let shape = (3.0 * cos_i * cos_i - 1.0) / (1.0 - e * e).powf(1.5);

    let a1 = (g.ke / n_o).powf(2.0 / 3.0);
    let d1 = 1.5 * k2 / (a1 * a1) * shape;
    let a0 = a1 * (1.0 - d1 / 3.0 - d1 * d1 - 134.0 / 81.0 * d1 * d1 * d1);
    let d0 = 1.5 * k2 / (a0 * a0) * shape;
    a0 / (1.0 - d0) * g.ae
}

/// Osculating classical elements from a TEME state (km, km/s). Angles of circular and
/// equatorial orbits are folded per spec 7.6 so position stays recoverable.
#[derive(Debug, Clone, Copy)]
pub struct OsculatingElements {
    pub semi_major_axis_km: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub arg_of_perigee_deg: f64,
    pub true_anomaly_deg: f64,
    pub mean_anomaly_deg: f64,
}

pub fn osculating_elements(pos_eci: [f64; 3], vel_eci: [f64; 3]) -> OsculatingElements {
    let mu = 398_600.441_8; // Earth gravitational parameter km^3 / s^2

    let r = (pos_eci[0] * pos_eci[0] + pos_eci[1] * pos_eci[1] + pos_eci[2] * pos_eci[2]).sqrt();
    let v = (vel_eci[0] * vel_eci[0] + vel_eci[1] * vel_eci[1] + vel_eci[2] * vel_eci[2]).sqrt();

    // Specific angular momentum h = r x v
    let h_vec = [
        pos_eci[1] * vel_eci[2] - pos_eci[2] * vel_eci[1],
        pos_eci[2] * vel_eci[0] - pos_eci[0] * vel_eci[2],
        pos_eci[0] * vel_eci[1] - pos_eci[1] * vel_eci[0],
    ];
    let h = (h_vec[0] * h_vec[0] + h_vec[1] * h_vec[1] + h_vec[2] * h_vec[2]).sqrt();

    // Specific orbital energy
    let energy = (v * v) / 2.0 - mu / r;
    let semi_major_axis_km = -mu / (2.0 * energy);

    // Eccentricity vector e = ((v^2 - mu/r)*r - (r.v)*v) / mu
    let r_dot_v = pos_eci[0] * vel_eci[0] + pos_eci[1] * vel_eci[1] + pos_eci[2] * vel_eci[2];
    let e_vec = [
        ((v * v - mu / r) * pos_eci[0] - r_dot_v * vel_eci[0]) / mu,
        ((v * v - mu / r) * pos_eci[1] - r_dot_v * vel_eci[1]) / mu,
        ((v * v - mu / r) * pos_eci[2] - r_dot_v * vel_eci[2]) / mu,
    ];
    let eccentricity = (e_vec[0] * e_vec[0] + e_vec[1] * e_vec[1] + e_vec[2] * e_vec[2]).sqrt();

    let inclination_deg = (h_vec[2] / h).clamp(-1.0, 1.0).acos().to_degrees();

    // Line of nodes n = z_unit x h = [-h_y, h_x, 0]
    let n_vec = [-h_vec[1], h_vec[0], 0.0];
    let n_mag = (n_vec[0] * n_vec[0] + n_vec[1] * n_vec[1]).sqrt();

    // Non-singular angle handling (spec 7.6): fold the undefined angle into the defined one
    // so that position along the orbit is preserved.
    let circular = eccentricity < 1e-6;
    let equatorial = n_mag / h < 1e-8;
    let retrograde = h_vec[2] < 0.0;
    let wrap = |deg: f64| deg.rem_euclid(360.0);

    let raan_deg = if equatorial {
        0.0
    } else {
        let mut raan = (n_vec[0] / n_mag).clamp(-1.0, 1.0).acos().to_degrees();
        if n_vec[1] < 0.0 {
            raan = 360.0 - raan;
        }
        raan
    };

    let (arg_of_perigee_deg, true_anomaly_deg) = if circular && equatorial {
        // true longitude l = atan2(r_y, r_x), flipped for retrograde
        let l = pos_eci[1].atan2(pos_eci[0]).to_degrees();
        (0.0, wrap(if retrograde { -l } else { l }))
    } else if circular {
        // argument of latitude u
        let dot = (n_vec[0] * pos_eci[0] + n_vec[1] * pos_eci[1]) / (n_mag * r);
        let mut u = dot.clamp(-1.0, 1.0).acos().to_degrees();
        if pos_eci[2] < 0.0 {
            u = 360.0 - u;
        }
        (0.0, u)
    } else {
        let omega = if equatorial {
            // longitude of periapsis varpi = atan2(e_y, e_x), flipped for retrograde
            let w = e_vec[1].atan2(e_vec[0]).to_degrees();
            wrap(if retrograde { -w } else { w })
        } else {
            let dot = (n_vec[0] * e_vec[0] + n_vec[1] * e_vec[1]) / (n_mag * eccentricity);
            let mut arg = dot.clamp(-1.0, 1.0).acos().to_degrees();
            if e_vec[2] < 0.0 {
                arg = 360.0 - arg;
            }
            arg
        };
        let dot = (e_vec[0] * pos_eci[0] + e_vec[1] * pos_eci[1] + e_vec[2] * pos_eci[2])
            / (eccentricity * r);
        let mut nu = dot.clamp(-1.0, 1.0).acos().to_degrees();
        if r_dot_v < 0.0 {
            nu = 360.0 - nu;
        }
        (omega, nu)
    };

    // Mean anomaly from true anomaly via eccentric anomaly E
    let nu_rad = true_anomaly_deg.to_radians();
    let cos_e = (eccentricity + nu_rad.cos()) / (1.0 + eccentricity * nu_rad.cos());
    let sin_e = ((1.0 - eccentricity * eccentricity).max(0.0).sqrt() * nu_rad.sin())
        / (1.0 + eccentricity * nu_rad.cos());
    let e_anom_rad = sin_e.atan2(cos_e);
    let mean_anom_rad = e_anom_rad - eccentricity * e_anom_rad.sin();
    let mut mean_anomaly_deg = mean_anom_rad.to_degrees() % 360.0;
    if mean_anomaly_deg < 0.0 {
        mean_anomaly_deg += 360.0;
    }

    OsculatingElements {
        semi_major_axis_km,
        eccentricity,
        inclination_deg,
        raan_deg,
        arg_of_perigee_deg,
        true_anomaly_deg,
        mean_anomaly_deg,
    }
}

/// Computes state vector (ECI, ECEF, Geodetic) and osculating Keplerian orbital elements
pub fn calculate_satellite_state(
    satellite: &Satellite,
    epoch: DateTime<Utc>,
) -> Result<SatelliteStateResponse, AppError> {
    let line1 = normalize_tle_line(&satellite.tle.line_one, '1');
    let line2 = normalize_tle_line(&satellite.tle.line_two, '2');

    let elements = Elements::from_tle(
        Some(satellite.name.clone()),
        line1.as_bytes(),
        line2.as_bytes(),
    )
    .map_err(|e| AppError::Sgp4Error(format!("Failed to parse TLE: {:?}", e)))?;

    let constants = Constants::from_elements(&elements)
        .map_err(|e| AppError::Sgp4Error(format!("Constants error: {:?}", e)))?;

    let epoch_dt = DateTime::<Utc>::from_naive_utc_and_offset(elements.datetime, Utc);
    let minutes_since_epoch = (epoch - epoch_dt).num_milliseconds() as f64 / 60000.0;

    let prediction = constants
        .propagate(minutes_since_epoch)
        .map_err(|e| AppError::Sgp4Error(format!("SGP4 propagation error: {:?}", e)))?;

    let pos_eci = prediction.position;
    let vel_eci = prediction.velocity;

    let lst = calculate_local_sidereal_time(epoch, 0.0);
    let pos_ecef = eci_to_ecf(pos_eci, lst);
    let vel_ecef = eci_to_ecf_velocity(pos_eci, vel_eci, lst);

    let (latitude_deg, longitude_deg, altitude_km) = ecf_to_geodetic(pos_ecef);

    let mu = 398_600.441_8; // Earth gravitational parameter km^3 / s^2
    let OsculatingElements {
        semi_major_axis_km,
        eccentricity,
        inclination_deg,
        raan_deg,
        arg_of_perigee_deg,
        true_anomaly_deg,
        mean_anomaly_deg,
    } = osculating_elements(pos_eci, vel_eci);

    let orbital_period_minutes =
        (2.0 * std::f64::consts::PI * (semi_major_axis_km.powi(3) / mu).sqrt()) / 60.0;
    let (perigee_altitude_km, apogee_altitude_km) = apsis_altitudes_km(
        semi_major_axis_km,
        eccentricity,
        inclination_deg,
        arg_of_perigee_deg,
    );

    Ok(SatelliteStateResponse {
        satellite_id: satellite.id,
        satellite_name: satellite.name.clone(),
        epoch,
        position_eci_km: pos_eci,
        velocity_eci_kms: vel_eci,
        position_ecef_km: pos_ecef,
        velocity_ecef_kms: vel_ecef,
        latitude_deg: (latitude_deg * 10000.0).round() / 10000.0,
        longitude_deg: (longitude_deg * 10000.0).round() / 10000.0,
        altitude_km: (altitude_km * 100.0).round() / 100.0,
        keplerian_elements: KeplerianElements {
            semi_major_axis_km: (semi_major_axis_km * 100.0).round() / 100.0,
            eccentricity: (eccentricity * 1000000.0).round() / 1000000.0,
            inclination_deg: (inclination_deg * 10000.0).round() / 10000.0,
            raan_deg: (raan_deg * 10000.0).round() / 10000.0,
            arg_of_perigee_deg: (arg_of_perigee_deg * 10000.0).round() / 10000.0,
            true_anomaly_deg: (true_anomaly_deg * 10000.0).round() / 10000.0,
            mean_anomaly_deg: (mean_anomaly_deg * 10000.0).round() / 10000.0,
            orbital_period_minutes: (orbital_period_minutes * 100.0).round() / 100.0,
            perigee_altitude_km: (perigee_altitude_km * 100.0).round() / 100.0,
            apogee_altitude_km: (apogee_altitude_km * 100.0).round() / 100.0,
        },
    })
}

/// Evaluates atmospheric drag decay rate, perigee altitude, and re-entry risk for a satellite
pub fn calculate_decay_risk(satellite: &Satellite) -> Result<SatelliteDecayRiskResponse, AppError> {
    let line1 = normalize_tle_line(&satellite.tle.line_one, '1');
    let line2 = normalize_tle_line(&satellite.tle.line_two, '2');

    let elements = Elements::from_tle(
        Some(satellite.name.clone()),
        line1.as_bytes(),
        line2.as_bytes(),
    )
    .map_err(|e| AppError::Sgp4Error(format!("Failed to parse TLE: {:?}", e)))?;

    let semi_major_axis = brouwer_mean_semi_major_axis_km(&elements);

    let ecc = elements.eccentricity;
    let (perigee_altitude_km, apogee_altitude_km) = apsis_altitudes_km(
        semi_major_axis,
        ecc,
        elements.inclination,
        elements.argument_of_perigee,
    );
    let bstar = elements.drag_term;
    let mean_motion_derivative = elements.mean_motion_dot * 2.0;

    // Regime and score depend on perigee altitude only. B* is an SGP4 fit parameter, not a
    // ballistic coefficient, so no lifetime is estimated from a TLE alone (spec 9.2).
    let (decay_status, risk_score) = if perigee_altitude_km < 150.0 {
        (
            "Critical Re-entry Imminent (< 48 hrs)".to_string(),
            (100.0 - (perigee_altitude_km - 100.0).clamp(0.0, 50.0) * 0.1).clamp(0.0, 100.0),
        )
    } else if perigee_altitude_km < 200.0 {
        (
            "High Risk / Imminent Re-entry (< 2 weeks)".to_string(),
            (85.0 + (200.0 - perigee_altitude_km) * 0.3).clamp(0.0, 100.0),
        )
    } else if perigee_altitude_km < 300.0 {
        (
            "Moderate Risk / Active Orbital Decay".to_string(),
            (50.0 + (300.0 - perigee_altitude_km) * 0.35).clamp(0.0, 100.0),
        )
    } else if perigee_altitude_km < 500.0 {
        (
            "Low Risk / Long-Term LEO Decay".to_string(),
            (10.0 + (500.0 - perigee_altitude_km) * 0.2).clamp(0.0, 100.0),
        )
    } else {
        (
            "Stable Orbit / Negligible Drag".to_string(),
            ((600.0 - perigee_altitude_km).max(0.0) * 0.05).clamp(0.0, 9.9),
        )
    };

    Ok(SatelliteDecayRiskResponse {
        satellite_id: satellite.id,
        satellite_name: satellite.name.clone(),
        perigee_altitude_km: (perigee_altitude_km * 10.0).round() / 10.0,
        apogee_altitude_km: (apogee_altitude_km * 10.0).round() / 10.0,
        bstar_drag: bstar,
        mean_motion_derivative_rev_day2: mean_motion_derivative,
        orbital_lifetime_days_estimate: None,
        decay_status,
        reentry_risk_score: (risk_score * 10.0).round() / 10.0,
    })
}

/// Catalog-wide scan for decaying satellites and uncontrolled re-entry hazards
pub fn scan_decay_watch(
    satellites: &[Satellite],
    max_perigee_km: f64,
    limit: usize,
) -> DecayWatchResponse {
    let mut objects: Vec<SatelliteDecayRiskResponse> = satellites
        .par_iter()
        .filter_map(|sat| {
            calculate_decay_risk(sat)
                .ok()
                .filter(|risk| risk.perigee_altitude_km <= max_perigee_km)
        })
        .collect();

    objects.sort_by(|a, b| {
        b.reentry_risk_score
            .partial_cmp(&a.reentry_risk_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    objects.truncate(limit.clamp(1, 500));

    DecayWatchResponse {
        scanned_satellites_count: satellites.len(),
        decaying_satellites_found: objects.len(),
        threshold_perigee_km: max_perigee_km,
        objects,
    }
}

/// Converts a Cartesian TEME state (km, km/s) into classical Keplerian elements.
/// Delegates to `osculating_elements` (spec 7.6 angle folding) so state and transform endpoints agree.
pub fn cartesian_to_keplerian(pos_eci: [f64; 3], vel_eci: [f64; 3]) -> ClassicalKeplerianElements {
    let mu = 398_600.441_8; // Earth gravitational parameter km^3 / s^2
    let o = osculating_elements(pos_eci, vel_eci);
    let (perigee_altitude_km, apogee_altitude_km) = apsis_altitudes_km(
        o.semi_major_axis_km,
        o.eccentricity,
        o.inclination_deg,
        o.arg_of_perigee_deg,
    );
    ClassicalKeplerianElements {
        semi_major_axis_km: o.semi_major_axis_km,
        eccentricity: o.eccentricity,
        inclination_deg: o.inclination_deg,
        raan_deg: o.raan_deg,
        arg_of_perigee_deg: o.arg_of_perigee_deg,
        true_anomaly_deg: o.true_anomaly_deg,
        mean_anomaly_deg: o.mean_anomaly_deg,
        orbital_period_minutes: 2.0
            * std::f64::consts::PI
            * (o.semi_major_axis_km.abs().powi(3) / mu).sqrt()
            / 60.0,
        perigee_altitude_km,
        apogee_altitude_km,
    }
}

/// Solves Kepler's equation M = E - e*sin(E) for Eccentric Anomaly, then computes True Anomaly (rad)
pub fn solve_kepler_equation(mean_anomaly_rad: f64, eccentricity: f64) -> f64 {
    let m = mean_anomaly_rad.rem_euclid(2.0 * std::f64::consts::PI);
    let mut e_anom = if eccentricity < 0.8 {
        m
    } else {
        std::f64::consts::PI
    };

    for _ in 0..60 {
        let f = e_anom - eccentricity * e_anom.sin() - m;
        let f_prime = 1.0 - eccentricity * e_anom.cos();
        let delta = f / f_prime;
        e_anom -= delta;
        if delta.abs() < 1e-13 {
            break;
        }
    }

    let cos_e = e_anom.cos();
    let sin_e = e_anom.sin();
    let cos_nu = (cos_e - eccentricity) / (1.0 - eccentricity * cos_e);
    let sin_nu = ((1.0 - eccentricity * eccentricity).max(0.0).sqrt() * sin_e)
        / (1.0 - eccentricity * cos_e);
    let mut nu = sin_nu.atan2(cos_nu);
    if nu < 0.0 {
        nu += 2.0 * std::f64::consts::PI;
    }
    nu
}

/// Converts Classical Keplerian Elements into Cartesian State Vector in ECI (TEME) frame
pub fn keplerian_to_cartesian(
    a: f64,
    e: f64,
    inc_deg: f64,
    raan_deg: f64,
    arg_pe_deg: f64,
    true_anomaly_deg: f64,
) -> Result<CartesianState, AppError> {
    if a <= 0.0 {
        return Err(AppError::BadRequest(
            "Semi-major axis must be positive for bound orbits".to_string(),
        ));
    }
    if !(0.0..1.0).contains(&e) {
        return Err(AppError::BadRequest(
            "Eccentricity must be in range [0, 1) for bound elliptic orbits".to_string(),
        ));
    }
    let mu = 398_600.441_8;
    let p = a * (1.0 - e * e);
    let nu = true_anomaly_deg.to_radians();
    let r = p / (1.0 + e * nu.cos());

    // Perifocal state
    let r_pqw = [r * nu.cos(), r * nu.sin(), 0.0];
    let v_factor = (mu / p).sqrt();
    let v_pqw = [-v_factor * nu.sin(), v_factor * (e + nu.cos()), 0.0];

    // Transformation PQW -> ECI: Rz(-raan) * Rx(-inc) * Rz(-arg_pe)
    let raan = raan_deg.to_radians();
    let inc = inc_deg.to_radians();
    let arg_pe = arg_pe_deg.to_radians();

    let c_raan = raan.cos();
    let s_raan = raan.sin();
    let c_inc = inc.cos();
    let s_inc = inc.sin();
    let c_pe = arg_pe.cos();
    let s_pe = arg_pe.sin();

    // Basis vectors of perifocal frame in ECI
    let p_hat = [
        c_raan * c_pe - s_raan * s_pe * c_inc,
        s_raan * c_pe + c_raan * s_pe * c_inc,
        s_pe * s_inc,
    ];
    let q_hat = [
        -c_raan * s_pe - s_raan * c_pe * c_inc,
        -s_raan * s_pe + c_raan * c_pe * c_inc,
        c_pe * s_inc,
    ];

    let pos_eci = [
        r_pqw[0] * p_hat[0] + r_pqw[1] * q_hat[0],
        r_pqw[0] * p_hat[1] + r_pqw[1] * q_hat[1],
        r_pqw[0] * p_hat[2] + r_pqw[1] * q_hat[2],
    ];

    let vel_eci = [
        v_pqw[0] * p_hat[0] + v_pqw[1] * q_hat[0],
        v_pqw[0] * p_hat[1] + v_pqw[1] * q_hat[1],
        v_pqw[0] * p_hat[2] + v_pqw[1] * q_hat[2],
    ];

    Ok(CartesianState {
        position_km: pos_eci,
        velocity_kms: vel_eci,
    })
}

/// Converts Classical Keplerian Elements into Singularity-Free Modified Equinoctial Elements (p, f, g, h, k, L)
pub fn keplerian_to_equinoctial(
    kep: &ClassicalKeplerianElements,
    retrograde_factor: Option<i32>,
) -> ModifiedEquinoctialElements {
    let p = kep.semi_major_axis_km * (1.0 - kep.eccentricity * kep.eccentricity);
    let inc_rad = kep.inclination_deg.to_radians();
    let raan_rad = kep.raan_deg.to_radians();
    let arg_pe_rad = kep.arg_of_perigee_deg.to_radians();
    let nu_rad = kep.true_anomaly_deg.to_radians();
    let m_rad = kep.mean_anomaly_deg.to_radians();

    let i_factor = match retrograde_factor {
        Some(-1) => -1,
        Some(_) => 1,
        None => {
            if kep.inclination_deg > 90.0 {
                -1
            } else {
                1
            }
        }
    };

    let (f, g, h, k, l_rad, lambda_rad) = if i_factor == 1 {
        let tan_half_i = (inc_rad / 2.0).tan();
        let f = kep.eccentricity * (arg_pe_rad + raan_rad).cos();
        let g = kep.eccentricity * (arg_pe_rad + raan_rad).sin();
        let h = tan_half_i * raan_rad.cos();
        let k = tan_half_i * raan_rad.sin();
        let l_rad = (raan_rad + arg_pe_rad + nu_rad).rem_euclid(2.0 * std::f64::consts::PI);
        let lambda_rad = (raan_rad + arg_pe_rad + m_rad).rem_euclid(2.0 * std::f64::consts::PI);
        (f, g, h, k, l_rad, lambda_rad)
    } else {
        let cot_half_i = 1.0 / (inc_rad / 2.0).tan();
        let f = kep.eccentricity * (arg_pe_rad - raan_rad).cos();
        let g = kep.eccentricity * (arg_pe_rad - raan_rad).sin();
        let h = cot_half_i * raan_rad.cos();
        let k = cot_half_i * raan_rad.sin();
        let l_rad = (-raan_rad + arg_pe_rad + nu_rad).rem_euclid(2.0 * std::f64::consts::PI);
        let lambda_rad = (-raan_rad + arg_pe_rad + m_rad).rem_euclid(2.0 * std::f64::consts::PI);
        (f, g, h, k, l_rad, lambda_rad)
    };

    ModifiedEquinoctialElements {
        p_km: p,
        f,
        g,
        h,
        k,
        true_longitude_deg: l_rad.to_degrees(),
        mean_longitude_deg: lambda_rad.to_degrees(),
        retrograde_factor: i_factor,
    }
}

/// Converts Modified Equinoctial Elements into Classical Keplerian Elements
pub fn equinoctial_to_keplerian(
    mee: &ModifiedEquinoctialElements,
) -> Result<ClassicalKeplerianElements, AppError> {
    let e = (mee.f * mee.f + mee.g * mee.g).sqrt();
    if e >= 1.0 {
        return Err(AppError::BadRequest(
            "Modified equinoctial eccentricity sqrt(f^2 + g^2) must be < 1 for bound orbits"
                .to_string(),
        ));
    }
    let a = mee.p_km / (1.0 - e * e);
    let i_factor = if mee.retrograde_factor == -1 { -1 } else { 1 };
    let tan_half_i = (mee.h * mee.h + mee.k * mee.k).sqrt();

    let inc_rad = if i_factor == 1 {
        2.0 * tan_half_i.atan()
    } else {
        2.0 * (1.0 / tan_half_i).atan()
    };

    let raan_rad = mee.k.atan2(mee.h).rem_euclid(2.0 * std::f64::consts::PI);
    let varpi_rad = mee.g.atan2(mee.f).rem_euclid(2.0 * std::f64::consts::PI);

    let arg_pe_rad = if i_factor == 1 {
        (varpi_rad - raan_rad).rem_euclid(2.0 * std::f64::consts::PI)
    } else {
        (varpi_rad + raan_rad).rem_euclid(2.0 * std::f64::consts::PI)
    };

    let nu_rad =
        (mee.true_longitude_deg.to_radians() - varpi_rad).rem_euclid(2.0 * std::f64::consts::PI);

    // Compute mean anomaly from nu and e
    let cos_e = (e + nu_rad.cos()) / (1.0 + e * nu_rad.cos());
    let sin_e = ((1.0 - e * e).max(0.0).sqrt() * nu_rad.sin()) / (1.0 + e * nu_rad.cos());
    let e_anom_rad = sin_e.atan2(cos_e);
    let m_rad = (e_anom_rad - e * e_anom_rad.sin()).rem_euclid(2.0 * std::f64::consts::PI);

    let mu = 398_600.441_8;
    let orbital_period_minutes = (2.0 * std::f64::consts::PI * (a.powi(3) / mu).sqrt()) / 60.0;
    let (perigee_altitude_km, apogee_altitude_km) =
        apsis_altitudes_km(a, e, inc_rad.to_degrees(), arg_pe_rad.to_degrees());

    Ok(ClassicalKeplerianElements {
        semi_major_axis_km: a,
        eccentricity: e,
        inclination_deg: inc_rad.to_degrees(),
        raan_deg: raan_rad.to_degrees(),
        arg_of_perigee_deg: arg_pe_rad.to_degrees(),
        true_anomaly_deg: nu_rad.to_degrees(),
        mean_anomaly_deg: m_rad.to_degrees(),
        orbital_period_minutes,
        perigee_altitude_km,
        apogee_altitude_km,
    })
}

/// Unified element transformation: converts between Cartesian, Classical Keplerian, and Modified Equinoctial Elements
pub fn transform_orbital_elements(
    req: &ElementTransformRequest,
) -> Result<ElementTransformResponse, AppError> {
    if let Some(cart) = &req.cartesian {
        let keplerian = cartesian_to_keplerian(cart.position_km, cart.velocity_kms);
        if !(keplerian.semi_major_axis_km > 0.0 && keplerian.eccentricity < 1.0) {
            return Err(AppError::BadRequest(
                "Cartesian state is not a bound elliptic orbit (need a > 0 and e < 1)".to_string(),
            ));
        }
        let equinoctial = keplerian_to_equinoctial(&keplerian, None);
        Ok(ElementTransformResponse {
            cartesian: cart.clone(),
            keplerian,
            equinoctial,
        })
    } else if let Some(kep) = &req.keplerian {
        let nu_deg = if let Some(nu) = kep.true_anomaly_deg {
            nu
        } else if let Some(m) = kep.mean_anomaly_deg {
            solve_kepler_equation(m.to_radians(), kep.eccentricity).to_degrees()
        } else {
            return Err(AppError::BadRequest(
                "keplerian input requires trueAnomalyDeg or meanAnomalyDeg".to_string(),
            ));
        };

        let cartesian = keplerian_to_cartesian(
            kep.semi_major_axis_km,
            kep.eccentricity,
            kep.inclination_deg,
            kep.raan_deg,
            kep.arg_of_perigee_deg,
            nu_deg,
        )?;

        let full_keplerian = cartesian_to_keplerian(cartesian.position_km, cartesian.velocity_kms);
        let equinoctial = keplerian_to_equinoctial(&full_keplerian, None);

        Ok(ElementTransformResponse {
            cartesian,
            keplerian: full_keplerian,
            equinoctial,
        })
    } else if let Some(eq) = &req.equinoctial {
        let true_long = if let Some(l) = eq.true_longitude_deg {
            l
        } else if let Some(lambda) = eq.mean_longitude_deg {
            let e = (eq.f * eq.f + eq.g * eq.g).sqrt();
            let varpi = eq.g.atan2(eq.f).to_degrees();
            let m_deg = (lambda - varpi).rem_euclid(360.0);
            let nu_deg = solve_kepler_equation(m_deg.to_radians(), e).to_degrees();
            (varpi + nu_deg).rem_euclid(360.0)
        } else {
            return Err(AppError::BadRequest(
                "equinoctial input requires trueLongitudeDeg or meanLongitudeDeg".to_string(),
            ));
        };

        let mee = ModifiedEquinoctialElements {
            p_km: eq.p_km,
            f: eq.f,
            g: eq.g,
            h: eq.h,
            k: eq.k,
            true_longitude_deg: true_long,
            mean_longitude_deg: eq.mean_longitude_deg.unwrap_or(0.0),
            retrograde_factor: eq.retrograde_factor.unwrap_or(1),
        };

        let keplerian = equinoctial_to_keplerian(&mee)?;
        let cartesian = keplerian_to_cartesian(
            keplerian.semi_major_axis_km,
            keplerian.eccentricity,
            keplerian.inclination_deg,
            keplerian.raan_deg,
            keplerian.arg_of_perigee_deg,
            keplerian.true_anomaly_deg,
        )?;
        let full_equinoctial = keplerian_to_equinoctial(&keplerian, Some(mee.retrograde_factor));

        Ok(ElementTransformResponse {
            cartesian,
            keplerian: full_equinoctial_round(keplerian),
            equinoctial: full_equinoctial,
        })
    } else {
        Err(AppError::BadRequest(
            "One of 'cartesian', 'keplerian', or 'equinoctial' must be provided in request"
                .to_string(),
        ))
    }
}

fn full_equinoctial_round(mut k: ClassicalKeplerianElements) -> ClassicalKeplerianElements {
    k.true_anomaly_deg = k.true_anomaly_deg.rem_euclid(360.0);
    k.mean_anomaly_deg = k.mean_anomaly_deg.rem_euclid(360.0);
    k.raan_deg = k.raan_deg.rem_euclid(360.0);
    k.arg_of_perigee_deg = k.arg_of_perigee_deg.rem_euclid(360.0);
    k
}

/// Converts ECF position [X, Y, Z] to ECI position using Local/Greenwich Sidereal Time
pub fn ecf_to_eci(ecf: [f64; 3], lst_rad: f64) -> [f64; 3] {
    let x = ecf[0] * lst_rad.cos() - ecf[1] * lst_rad.sin();
    let y = ecf[0] * lst_rad.sin() + ecf[1] * lst_rad.cos();
    let z = ecf[2];
    [x, y, z]
}

/// Converts ECF velocity vector to ECI velocity vector accounting for Earth rotation kinematics
pub fn ecf_to_eci_velocity(eci_pos: [f64; 3], ecf_vel: [f64; 3], lst_rad: f64) -> [f64; 3] {
    let omega_e = 7.2921151467e-5; // rad/s
    let vx_eff = ecf_vel[0] * lst_rad.cos() - ecf_vel[1] * lst_rad.sin();
    let vy_eff = ecf_vel[0] * lst_rad.sin() + ecf_vel[1] * lst_rad.cos();
    let vz_eff = ecf_vel[2];

    let vx_eci = vx_eff - omega_e * eci_pos[1];
    let vy_eci = vy_eff + omega_e * eci_pos[0];
    let vz_eci = vz_eff;

    [vx_eci, vy_eci, vz_eci]
}

/// Computes observer ECEF Cartesian coordinates from WGS-84 Geodetic Latitude, Longitude, and Altitude
pub fn observer_geodetic_to_ecf(lat_deg: f64, lon_deg: f64, alt_km: f64) -> [f64; 3] {
    let lat_rad = lat_deg.to_radians();
    let lon_rad = lon_deg.to_radians();
    let re = 6378.137;
    let f = 1.0 / 298.257223563;
    let c = 1.0 / (1.0 - (2.0 * f - f * f) * lat_rad.sin().powi(2)).sqrt();
    let obs_x = (re * c + alt_km) * lat_rad.cos() * lon_rad.cos();
    let obs_y = (re * c + alt_km) * lat_rad.cos() * lon_rad.sin();
    let obs_z = (re * (c * (1.0 - f).powi(2)) + alt_km) * lat_rad.sin();
    [obs_x, obs_y, obs_z]
}

/// Transforms ECEF satellite position into Topocentric Horizon SEZ (South, East, Zenith) coordinates
pub fn ecf_to_sez(sat_ecf: [f64; 3], lat_deg: f64, lon_deg: f64, alt_km: f64) -> [f64; 3] {
    let obs = observer_geodetic_to_ecf(lat_deg, lon_deg, alt_km);
    let rx = sat_ecf[0] - obs[0];
    let ry = sat_ecf[1] - obs[1];
    let rz = sat_ecf[2] - obs[2];

    let lat_rad = lat_deg.to_radians();
    let lon_rad = lon_deg.to_radians();

    let s = lat_rad.sin() * lon_rad.cos() * rx + lat_rad.sin() * lon_rad.sin() * ry
        - lat_rad.cos() * rz;
    let e = -lon_rad.sin() * rx + lon_rad.cos() * ry;
    let z = lat_rad.cos() * lon_rad.cos() * rx
        + lat_rad.cos() * lon_rad.sin() * ry
        + lat_rad.sin() * rz;
    [s, e, z]
}

/// Transforms ECEF velocity vector into Topocentric Horizon SEZ velocity
pub fn ecf_to_sez_velocity(vel_ecf: [f64; 3], lat_deg: f64, lon_deg: f64) -> [f64; 3] {
    let lat_rad = lat_deg.to_radians();
    let lon_rad = lon_deg.to_radians();
    let vx = vel_ecf[0];
    let vy = vel_ecf[1];
    let vz = vel_ecf[2];

    let vs = lat_rad.sin() * lon_rad.cos() * vx + lat_rad.sin() * lon_rad.sin() * vy
        - lat_rad.cos() * vz;
    let ve = -lon_rad.sin() * vx + lon_rad.cos() * vy;
    let vz_top = lat_rad.cos() * lon_rad.cos() * vx
        + lat_rad.cos() * lon_rad.sin() * vy
        + lat_rad.sin() * vz;
    [vs, ve, vz_top]
}

/// Transforms Topocentric Horizon SEZ slant vector into ECEF satellite position
pub fn sez_to_ecf(sez: [f64; 3], lat_deg: f64, lon_deg: f64, alt_km: f64) -> [f64; 3] {
    let obs = observer_geodetic_to_ecf(lat_deg, lon_deg, alt_km);
    let lat_rad = lat_deg.to_radians();
    let lon_rad = lon_deg.to_radians();

    let s = sez[0];
    let e = sez[1];
    let z = sez[2];

    // Transpose of direction cosine rotation matrix
    let rx =
        lat_rad.sin() * lon_rad.cos() * s - lon_rad.sin() * e + lat_rad.cos() * lon_rad.cos() * z;
    let ry =
        lat_rad.sin() * lon_rad.sin() * s + lon_rad.cos() * e + lat_rad.cos() * lon_rad.sin() * z;
    let rz = -lat_rad.cos() * s + lat_rad.sin() * z;

    [obs[0] + rx, obs[1] + ry, obs[2] + rz]
}

/// Transforms Topocentric Horizon SEZ velocity vector into ECEF velocity
pub fn sez_to_ecf_velocity(vel_sez: [f64; 3], lat_deg: f64, lon_deg: f64) -> [f64; 3] {
    let lat_rad = lat_deg.to_radians();
    let lon_rad = lon_deg.to_radians();
    let vs = vel_sez[0];
    let ve = vel_sez[1];
    let vz = vel_sez[2];

    let vx = lat_rad.sin() * lon_rad.cos() * vs - lon_rad.sin() * ve
        + lat_rad.cos() * lon_rad.cos() * vz;
    let vy = lat_rad.sin() * lon_rad.sin() * vs
        + lon_rad.cos() * ve
        + lat_rad.cos() * lon_rad.sin() * vz;
    let vz_ecf = -lat_rad.cos() * vs + lat_rad.sin() * vz;

    [vx, vy, vz_ecf]
}

/// Transforms Cartesian state vectors across ECI (TEME), ECEF (WGS-84), SEZ, and NED frames
pub fn transform_coordinate_frame(
    req: &FrameTransformRequest,
) -> Result<FrameTransformResponse, AppError> {
    let lst = calculate_local_sidereal_time(req.epoch, 0.0);

    // Step 1: Convert source frame to ECEF
    let (pos_ecef, vel_ecef) = match req.source_frame {
        CoordinateFrame::EcefWgs84 => (req.position, req.velocity),
        CoordinateFrame::EciTeme => {
            let pos_ecf = eci_to_ecf(req.position, lst);
            let vel_ecf = req
                .velocity
                .map(|v| eci_to_ecf_velocity(req.position, v, lst));
            (pos_ecf, vel_ecf)
        }
        CoordinateFrame::TopocentricSez => {
            let lat = req.observer_lat_deg.ok_or_else(|| {
                AppError::BadRequest(
                    "observerLatDeg is required when sourceFrame is TOPOCENTRIC_SEZ".to_string(),
                )
            })?;
            let lon = req.observer_lon_deg.ok_or_else(|| {
                AppError::BadRequest(
                    "observerLonDeg is required when sourceFrame is TOPOCENTRIC_SEZ".to_string(),
                )
            })?;
            let alt_km = req.observer_alt_m.unwrap_or(0.0) / 1000.0;
            let pos_ecf = sez_to_ecf(req.position, lat, lon, alt_km);
            let vel_ecf = req.velocity.map(|v| sez_to_ecf_velocity(v, lat, lon));
            (pos_ecf, vel_ecf)
        }
        CoordinateFrame::TopocentricNed => {
            let lat = req.observer_lat_deg.ok_or_else(|| {
                AppError::BadRequest(
                    "observerLatDeg is required when sourceFrame is TOPOCENTRIC_NED".to_string(),
                )
            })?;
            let lon = req.observer_lon_deg.ok_or_else(|| {
                AppError::BadRequest(
                    "observerLonDeg is required when sourceFrame is TOPOCENTRIC_NED".to_string(),
                )
            })?;
            let alt_km = req.observer_alt_m.unwrap_or(0.0) / 1000.0;
            // NED [North, East, Down] -> SEZ [-North, East, -Down]
            let pos_sez = [-req.position[0], req.position[1], -req.position[2]];
            let pos_ecf = sez_to_ecf(pos_sez, lat, lon, alt_km);
            let vel_ecf = req.velocity.map(|v| {
                let v_sez = [-v[0], v[1], -v[2]];
                sez_to_ecf_velocity(v_sez, lat, lon)
            });
            (pos_ecf, vel_ecf)
        }
    };

    // Step 2: Convert ECEF to target frame
    let (target_pos, target_vel) = match req.target_frame {
        CoordinateFrame::EcefWgs84 => (pos_ecef, vel_ecef),
        CoordinateFrame::EciTeme => {
            let pos_eci = ecf_to_eci(pos_ecef, lst);
            let vel_eci = vel_ecef.map(|v| ecf_to_eci_velocity(pos_eci, v, lst));
            (pos_eci, vel_eci)
        }
        CoordinateFrame::TopocentricSez => {
            let lat = req.observer_lat_deg.ok_or_else(|| {
                AppError::BadRequest(
                    "observerLatDeg is required when targetFrame is TOPOCENTRIC_SEZ".to_string(),
                )
            })?;
            let lon = req.observer_lon_deg.ok_or_else(|| {
                AppError::BadRequest(
                    "observerLonDeg is required when targetFrame is TOPOCENTRIC_SEZ".to_string(),
                )
            })?;
            let alt_km = req.observer_alt_m.unwrap_or(0.0) / 1000.0;
            let pos_sez = ecf_to_sez(pos_ecef, lat, lon, alt_km);
            let vel_sez = vel_ecef.map(|v| ecf_to_sez_velocity(v, lat, lon));
            (pos_sez, vel_sez)
        }
        CoordinateFrame::TopocentricNed => {
            let lat = req.observer_lat_deg.ok_or_else(|| {
                AppError::BadRequest(
                    "observerLatDeg is required when targetFrame is TOPOCENTRIC_NED".to_string(),
                )
            })?;
            let lon = req.observer_lon_deg.ok_or_else(|| {
                AppError::BadRequest(
                    "observerLonDeg is required when targetFrame is TOPOCENTRIC_NED".to_string(),
                )
            })?;
            let alt_km = req.observer_alt_m.unwrap_or(0.0) / 1000.0;
            let pos_sez = ecf_to_sez(pos_ecef, lat, lon, alt_km);
            let vel_sez = vel_ecef.map(|v| ecf_to_sez_velocity(v, lat, lon));
            // SEZ [South, East, Zenith] -> NED [-South, East, -Zenith]
            let pos_ned = [-pos_sez[0], pos_sez[1], -pos_sez[2]];
            let vel_ned = vel_sez.map(|v| [-v[0], v[1], -v[2]]);
            (pos_ned, vel_ned)
        }
    };

    // Geodetic sub-satellite coordinates
    let (geo_lat, geo_lon, geo_alt) = ecf_to_geodetic(pos_ecef);
    let geodetic = Some(GeodeticCoordinates {
        latitude_deg: (geo_lat * 10000.0).round() / 10000.0,
        longitude_deg: (geo_lon * 10000.0).round() / 10000.0,
        altitude_km: (geo_alt * 100.0).round() / 100.0,
    });

    // Optional look angles if observer position is provided
    let look_angles = if let (Some(lat), Some(lon)) = (req.observer_lat_deg, req.observer_lon_deg) {
        let alt_km = req.observer_alt_m.unwrap_or(0.0) / 1000.0;
        let pos_sez = ecf_to_sez(pos_ecef, lat, lon, alt_km);
        let range =
            (pos_sez[0] * pos_sez[0] + pos_sez[1] * pos_sez[1] + pos_sez[2] * pos_sez[2]).sqrt();
        let el = if range > 1e-6 {
            (pos_sez[2] / range).clamp(-1.0, 1.0).asin().to_degrees()
        } else {
            0.0
        };
        let mut az = pos_sez[1].atan2(-pos_sez[0]).to_degrees();
        if az < 0.0 {
            az += 360.0;
        }
        let range_rate = vel_ecef.and_then(|v| {
            if range > 1e-6 {
                let v_sez = ecf_to_sez_velocity(v, lat, lon);
                Some(
                    (pos_sez[0] * v_sez[0] + pos_sez[1] * v_sez[1] + pos_sez[2] * v_sez[2]) / range,
                )
            } else {
                None
            }
        });
        Some(LookAnglesDetail {
            azimuth_deg: (az * 10000.0).round() / 10000.0,
            elevation_deg: (el * 10000.0).round() / 10000.0,
            slant_range_km: (range * 100.0).round() / 100.0,
            range_rate_kms: range_rate.map(|rr| (rr * 10000.0).round() / 10000.0),
        })
    } else {
        None
    };

    Ok(FrameTransformResponse {
        source_frame: req.source_frame,
        target_frame: req.target_frame,
        epoch: req.epoch,
        position: target_pos,
        velocity: target_vel,
        geodetic,
        look_angles,
    })
}
