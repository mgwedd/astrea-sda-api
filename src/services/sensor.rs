use chrono::{DateTime, Duration, Utc};
use std::f64::consts::PI;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{
    HorizonMaskPoint, Satellite, ScheduledSensorPass, SensorDefinition, SensorHandover,
    SensorScheduleResponse, TrackingRates,
};
use crate::services::astrodynamics::{
    calculate_illumination, calculate_local_sidereal_time, calculate_sun_position_eci, ecf_to_sez,
    ecf_to_sez_velocity, eci_to_ecf, eci_to_ecf_velocity, observer_position_eci, phase_angle_rad,
    visual_magnitude,
};
use crate::services::ephemeris::Ephemeris;

/// Interpolates minimum horizon elevation angle at a given azimuth using 360-degree
/// piecewise-linear interpolation with circular boundary wrap-around.
pub fn interpolate_horizon_elevation(mask: &[HorizonMaskPoint], azimuth_deg: f64) -> f64 {
    if mask.is_empty() {
        return 0.0;
    }
    if mask.len() == 1 {
        return mask[0].elevation_deg;
    }

    // Normalize input azimuth into [0.0, 360.0)
    let az = azimuth_deg.rem_euclid(360.0);

    // Normalize and sort mask points by azimuth
    let mut sorted: Vec<(f64, f64)> = mask
        .iter()
        .map(|p| (p.azimuth_deg.rem_euclid(360.0), p.elevation_deg))
        .collect();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let n = sorted.len();

    // Check if within bounds of sorted list [sorted[0].0, sorted[n - 1].0]
    for i in 0..(n - 1) {
        let (az_i, el_i) = sorted[i];
        let (az_next, el_next) = sorted[i + 1];

        if az >= az_i && az <= az_next {
            let span = az_next - az_i;
            if span < 1e-9 {
                return el_i;
            }
            let t = (az - az_i) / span;
            return el_i + t * (el_next - el_i);
        }
    }

    // Otherwise, az is in the wrap-around segment between sorted[n - 1] and sorted[0]
    let (az_last, el_last) = sorted[n - 1];
    let (az_first, el_first) = sorted[0];

    let span = az_first + 360.0 - az_last;
    if span < 1e-9 {
        return el_last;
    }

    let dist = if az >= az_last {
        az - az_last
    } else {
        az + 360.0 - az_last
    };

    let t = dist / span;
    el_last + t * (el_first - el_last)
}

/// Computes instantaneous range rate, azimuth angular tracking rate, and elevation
/// angular tracking rate from Topocentric SEZ position (km) and velocity (km/s).
pub fn calculate_tracking_rates(pos_sez: [f64; 3], vel_sez: [f64; 3]) -> TrackingRates {
    let s = pos_sez[0];
    let e = pos_sez[1];
    let z = pos_sez[2];

    let vs = vel_sez[0];
    let ve = vel_sez[1];
    let vz = vel_sez[2];

    let range = (s * s + e * e + z * z).sqrt();
    if range < 1e-6 {
        return TrackingRates {
            range_rate_kms: 0.0,
            azimuth_rate_deg_s: 0.0,
            elevation_rate_deg_s: 0.0,
        };
    }

    // Range rate: d(rho)/dt = (s*vs + e*ve + z*vz) / rho in km/s
    let range_rate_kms = (s * vs + e * ve + z * vz) / range;

    // Horizontal range squared: rho_h^2 = s^2 + e^2
    let rho_h_sq = s * s + e * e;
    let rho_h = rho_h_sq.sqrt();

    // Azimuth rate: d(atan2(e, -s))/dt = (-s*ve + e*vs) / (s^2 + e^2) in rad/s
    let azimuth_rate_deg_s = if rho_h_sq < 1e-12 {
        0.0 // Near zenith keyhole singularity
    } else {
        let az_rate_rad_s = (-s * ve + e * vs) / rho_h_sq;
        az_rate_rad_s * (180.0 / PI)
    };

    // Elevation rate: d(asin(z / rho))/dt = (rho * vz - z * range_rate) / (rho * rho_h) in rad/s
    let elevation_rate_deg_s = if rho_h < 1e-6 {
        0.0
    } else {
        let el_rate_rad_s = (range * vz - z * range_rate_kms) / (range * rho_h);
        el_rate_rad_s * (180.0 / PI)
    };

    TrackingRates {
        range_rate_kms,
        azimuth_rate_deg_s,
        elevation_rate_deg_s,
    }
}

/// Topocentric state bundle containing look angles, slant range, and tracking rates.
#[derive(Debug, Clone, PartialEq)]
pub struct TopocentricState {
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    pub range_km: f64,
    pub rates: TrackingRates,
}

/// Evaluates Topocentric SEZ state, look angles, and rates at a given epoch using an Ephemeris.
pub fn topocentric_state_from_ephemeris(
    ephem: &Ephemeris,
    lat_deg: f64,
    lon_deg: f64,
    alt_km: f64,
    time: DateTime<Utc>,
) -> Result<TopocentricState, AppError> {
    let pred = ephem.propagate(time)?;
    let lst = calculate_local_sidereal_time(time, 0.0);
    let sat_ecf = eci_to_ecf(pred.position, lst);
    let sat_vel_ecf = eci_to_ecf_velocity(pred.position, pred.velocity, lst);

    let pos_sez = ecf_to_sez(sat_ecf, lat_deg, lon_deg, alt_km);
    let vel_sez = ecf_to_sez_velocity(sat_vel_ecf, lat_deg, lon_deg);

    let s = pos_sez[0];
    let e = pos_sez[1];
    let z = pos_sez[2];

    let range_km = (s * s + e * e + z * z).sqrt();
    let elevation_deg = (z / range_km).clamp(-1.0, 1.0).asin().to_degrees();
    let azimuth_deg = e.atan2(-s).to_degrees().rem_euclid(360.0);
    let rates = calculate_tracking_rates(pos_sez, vel_sez);

    Ok(TopocentricState {
        azimuth_deg,
        elevation_deg,
        range_km,
        rates,
    })
}

/// Evaluates visibility margin for a sensor: elevation - max(min_elevation, horizon_mask(azimuth)).
fn calculate_elevation_margin(state: &TopocentricState, sensor: &SensorDefinition) -> f64 {
    let min_el = sensor.min_elevation_deg.unwrap_or(10.0).clamp(-10.0, 89.0);
    let horizon_el = match &sensor.horizon_mask {
        Some(mask) if !mask.is_empty() => interpolate_horizon_elevation(mask, state.azimuth_deg),
        _ => -90.0,
    };
    let effective_threshold = min_el.max(horizon_el);
    state.elevation_deg - effective_threshold
}

/// Finds all tracking passes for a single satellite over a specific ground sensor,
/// evaluating custom 360-degree horizon profiles, mount slewing rates, and optical constraints.
pub fn find_sensor_passes(
    satellite: &Satellite,
    sensor: &SensorDefinition,
    start_time: DateTime<Utc>,
    duration_days: f64,
    cancel: &CancellationToken,
) -> Result<Vec<ScheduledSensorPass>, AppError> {
    let days = duration_days.clamp(0.1, 14.0);
    let total_minutes = (days * 24.0 * 60.0).ceil() as i64;
    let alt_km = sensor.alt_m / 1000.0;

    let ephem = Ephemeris::from_satellite(satellite)?;
    let sensor_id = sensor.id.unwrap_or_else(Uuid::new_v4);

    let eval_state = |t: DateTime<Utc>| -> Result<TopocentricState, AppError> {
        topocentric_state_from_ephemeris(&ephem, sensor.lat, sensor.lon, alt_km, t)
    };

    let eval_margin = |t: DateTime<Utc>| -> Result<f64, AppError> {
        let st = eval_state(t)?;
        Ok(calculate_elevation_margin(&st, sensor))
    };

    let mut passes = Vec::new();
    let mut pass_id = 1;
    let mut k: i64 = 0;

    while k < total_minutes {
        if cancel.is_cancelled() {
            return Err(crate::services::ephemeris::cancelled());
        }

        let t_curr = start_time + Duration::minutes(k);
        let m_curr = eval_margin(t_curr)?;

        if m_curr >= 0.0 {
            // Find AOS (crossing threshold from below)
            let mut below = t_curr - Duration::minutes(1);
            let mut above = t_curr;
            let aos_time = if k == 0 {
                start_time
            } else {
                while (above - below).num_milliseconds() > 1000 {
                    let mid = below + (above - below) / 2;
                    if eval_margin(mid)? >= 0.0 {
                        above = mid;
                    } else {
                        below = mid;
                    }
                }
                above
            };

            let aos_state = eval_state(aos_time)?;

            // Trace forward to find LOS (crossing margin back below 0)
            let mut t_scan = t_curr + Duration::minutes(1);
            let mut peak_time = t_curr;
            let mut peak_elev = aos_state.elevation_deg;

            while k < total_minutes {
                let m_scan = eval_margin(t_scan)?;
                let st_scan = eval_state(t_scan)?;
                if st_scan.elevation_deg > peak_elev {
                    peak_elev = st_scan.elevation_deg;
                    peak_time = t_scan;
                }
                if m_scan < 0.0 {
                    break;
                }
                t_scan += Duration::minutes(1);
                k += 1;
            }

            let los_time = if k >= total_minutes {
                start_time + Duration::minutes(total_minutes)
            } else {
                let mut los_above = t_scan - Duration::minutes(1);
                let mut los_below = t_scan;
                while (los_below - los_above).num_milliseconds() > 1000 {
                    let mid = los_above + (los_below - los_above) / 2;
                    if eval_margin(mid)? >= 0.0 {
                        los_above = mid;
                    } else {
                        los_below = mid;
                    }
                }
                los_below
            };

            let duration_sec = (los_time - aos_time).num_milliseconds() as f64 / 1000.0;
            if duration_sec < 10.0 {
                k += 1;
                continue;
            }

            // Refine peak elevation with golden section search around peak_time
            let gr = 0.618_033_988_75;
            let (mut lo_t, mut hi_t) = (
                peak_time - Duration::minutes(1),
                peak_time + Duration::minutes(1),
            );
            if lo_t < aos_time {
                lo_t = aos_time;
            }
            if hi_t > los_time {
                hi_t = los_time;
            }

            while (hi_t - lo_t).num_milliseconds() > 1000 {
                let diff_ms = (hi_t - lo_t).num_milliseconds() as f64;
                let m1 = hi_t - Duration::milliseconds((gr * diff_ms) as i64);
                let m2 = lo_t + Duration::milliseconds((gr * diff_ms) as i64);
                if eval_state(m1)?.elevation_deg > eval_state(m2)?.elevation_deg {
                    hi_t = m2;
                } else {
                    lo_t = m1;
                }
            }

            let tca_time = lo_t + (hi_t - lo_t) / 2;
            let tca_state = eval_state(tca_time)?;
            let los_state = eval_state(los_time)?;

            // Sample across pass window [aos_time, los_time] to detect peak tracking rates
            let mut peak_az_rate = 0.0_f64;
            let mut peak_el_rate = 0.0_f64;
            let mut max_observed_el = peak_elev.max(tca_state.elevation_deg);

            let sample_step_s = 5;
            let pass_steps = (duration_sec / sample_step_s as f64).ceil() as i64;
            for s in 0..=pass_steps {
                let t_sample = aos_time + Duration::seconds(s * sample_step_s);
                let t_eval = if t_sample > los_time {
                    los_time
                } else {
                    t_sample
                };
                let st = eval_state(t_eval)?;

                let az_r = st.rates.azimuth_rate_deg_s.abs();
                let el_r = st.rates.elevation_rate_deg_s.abs();
                if az_r > peak_az_rate {
                    peak_az_rate = az_r;
                }
                if el_r > peak_el_rate {
                    peak_el_rate = el_r;
                }
                if st.elevation_deg > max_observed_el {
                    max_observed_el = st.elevation_deg;
                }
            }

            // Also check at TCA explicitly
            let tca_az_r = tca_state.rates.azimuth_rate_deg_s.abs();
            let tca_el_r = tca_state.rates.elevation_rate_deg_s.abs();
            if tca_az_r > peak_az_rate {
                peak_az_rate = tca_az_r;
            }
            if tca_el_r > peak_el_rate {
                peak_el_rate = tca_el_r;
            }

            // Check mount mechanical limits
            let mut is_mount_feasible = true;
            if let Some(max_el) = sensor.max_elevation_deg {
                if max_observed_el > max_el {
                    is_mount_feasible = false;
                }
            }
            if let Some(max_az_rate) = sensor.max_azimuth_rate_deg_s {
                if peak_az_rate > max_az_rate {
                    is_mount_feasible = false;
                }
            }
            if let Some(max_el_rate) = sensor.max_elevation_rate_deg_s {
                if peak_el_rate > max_el_rate {
                    is_mount_feasible = false;
                }
            }

            // Illumination and optical visibility at TCA
            let is_optical_visible = match calculate_illumination(
                satellite,
                sensor.lat,
                sensor.lon,
                sensor.alt_m,
                tca_time,
            ) {
                Ok(illum) => illum.is_visibly_observable,
                Err(_) => false,
            };

            let visual_magnitude = if is_optical_visible {
                let sat_eci = ephem
                    .propagate(tca_time)
                    .map(|p| p.position)
                    .unwrap_or([0.0; 3]);
                let obs_eci = observer_position_eci(sensor.lat, sensor.lon, alt_km, tca_time);
                let sun_eci = calculate_sun_position_eci(tca_time);
                let gamma = phase_angle_rad(sat_eci, obs_eci, sun_eci);
                let mag = visual_magnitude(tca_state.range_km, gamma);
                Some((mag * 10.0).round() / 10.0)
            } else {
                None
            };

            let optical_only = sensor.optical_only.unwrap_or(false);
            if !optical_only || is_optical_visible {
                passes.push(ScheduledSensorPass {
                    pass_id,
                    sensor_id,
                    sensor_name: sensor.name.clone(),
                    satellite_id: satellite.id,
                    satellite_name: satellite.name.clone(),
                    aos_time,
                    aos_azimuth_deg: (aos_state.azimuth_deg * 100.0).round() / 100.0,
                    aos_elevation_deg: (aos_state.elevation_deg * 100.0).round() / 100.0,
                    tca_time,
                    tca_azimuth_deg: (tca_state.azimuth_deg * 100.0).round() / 100.0,
                    max_elevation_deg: (max_observed_el * 100.0).round() / 100.0,
                    tca_range_km: (tca_state.range_km * 10.0).round() / 10.0,
                    tca_range_rate_kms: (tca_state.rates.range_rate_kms * 1000.0).round() / 1000.0,
                    tca_azimuth_rate_deg_s: (tca_state.rates.azimuth_rate_deg_s * 1000.0).round()
                        / 1000.0,
                    tca_elevation_rate_deg_s: (tca_state.rates.elevation_rate_deg_s * 1000.0)
                        .round()
                        / 1000.0,
                    peak_azimuth_rate_deg_s: (peak_az_rate * 1000.0).round() / 1000.0,
                    peak_elevation_rate_deg_s: (peak_el_rate * 1000.0).round() / 1000.0,
                    is_mount_feasible,
                    is_optical_visible,
                    visual_magnitude,
                    los_time,
                    los_azimuth_deg: (los_state.azimuth_deg * 100.0).round() / 100.0,
                    los_elevation_deg: (los_state.elevation_deg * 100.0).round() / 100.0,
                    duration_seconds: (duration_sec * 10.0).round() / 10.0,
                });
                pass_id += 1;
            }
        }
        k += 1;
    }

    Ok(passes)
}

/// Multi-station sensor network scheduler generating unified pass schedules and inter-sensor handovers.
pub fn schedule_sensor_network(
    satellites: &[Satellite],
    sensors: &[SensorDefinition],
    start_time: DateTime<Utc>,
    duration_days: f64,
    cancel: &CancellationToken,
) -> Result<SensorScheduleResponse, AppError> {
    let mut normalized_sensors = sensors.to_vec();
    for s in &mut normalized_sensors {
        if s.id.is_none() {
            s.id = Some(Uuid::new_v4());
        }
    }

    let mut all_passes = Vec::new();

    for sat in satellites {
        for sensor in &normalized_sensors {
            if cancel.is_cancelled() {
                return Err(crate::services::ephemeris::cancelled());
            }
            let passes = find_sensor_passes(sat, sensor, start_time, duration_days, cancel)?;
            all_passes.extend(passes);
        }
    }

    // Sort all passes chronologically by AOS time
    all_passes.sort_by_key(|a| a.aos_time);

    // Re-index pass_id consecutively
    for (idx, p) in all_passes.iter_mut().enumerate() {
        p.pass_id = idx + 1;
    }

    // Detect sensor handovers across passes for each satellite
    let mut handovers = Vec::new();

    for sat in satellites {
        let sat_passes: Vec<&ScheduledSensorPass> = all_passes
            .iter()
            .filter(|p| p.satellite_id == sat.id)
            .collect();

        for i in 0..sat_passes.len().saturating_sub(1) {
            let p_a = sat_passes[i];
            let p_b = sat_passes[i + 1];

            if p_a.sensor_id != p_b.sensor_id {
                let overlap_s = (p_a.los_time - p_b.aos_time).num_milliseconds() as f64 / 1000.0;
                // Overlap (dual station tracking) or sequential handover within 1 hour
                if overlap_s >= 0.0 || overlap_s.abs() <= 3600.0 {
                    handovers.push(SensorHandover {
                        satellite_id: sat.id,
                        satellite_name: sat.name.clone(),
                        from_sensor_id: p_a.sensor_id,
                        from_sensor_name: p_a.sensor_name.clone(),
                        to_sensor_id: p_b.sensor_id,
                        to_sensor_name: p_b.sensor_name.clone(),
                        handover_time: p_b.aos_time,
                        overlap_seconds: (overlap_s * 10.0).round() / 10.0,
                    });
                }
            }
        }
    }

    let end_time = start_time + Duration::milliseconds((duration_days * 86_400_000.0) as i64);
    let total_passes = all_passes.len();

    Ok(SensorScheduleResponse {
        start: start_time,
        end: end_time,
        total_passes,
        passes: all_passes,
        handovers,
    })
}
