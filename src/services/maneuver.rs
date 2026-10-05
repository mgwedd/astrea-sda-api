use crate::error::AppError;
use crate::models::{
    AnomalyDetectionResponse, AnomalyStatus, DetectedManeuver, ManeuverType, ManeuversResponse,
    OrbitalParameterResidual, Satellite, Tle,
};
use crate::services::astrodynamics::{brouwer_mean_semi_major_axis_km, normalize_tle_line};
use chrono::{DateTime, Utc};
use sgp4::Elements;

/// Below this many residuals, median/MAD statistics are not meaningful and no verdict is given.
pub const MIN_RESIDUALS_FOR_STATISTICS: usize = 5;

/// Trailing window (samples) of consecutive da/dt used for the quiescent drift baseline.
const BASELINE_WINDOW: usize = 5;
/// Fewer prior da/dt samples than this gives a zero baseline.
const MIN_BASELINE_SAMPLES: usize = 3;

fn parse_tle(name: &str, tle: &Tle) -> Result<Elements, AppError> {
    let line1 = normalize_tle_line(&tle.line_one, '1');
    let line2 = normalize_tle_line(&tle.line_two, '2');
    Elements::from_tle(Some(name.to_string()), line1.as_bytes(), line2.as_bytes())
        .map_err(|e| AppError::Sgp4Error(format!("Failed to parse TLE: {:?}", e)))
}

fn epoch(e: &Elements) -> DateTime<Utc> {
    DateTime::<Utc>::from_naive_utc_and_offset(e.datetime, Utc)
}

/// Mean elements of every TLE in `history`, oldest first (one per epoch). For each TLE after
/// the first:
/// - SMA residual = a''(obs) - a_pred, where a'' is the Brouwer mean semi-major axis and
///   a_pred = a''_prev + median(trailing da/dt) * Δt (zero drift with < 3 prior samples).
///   TLE ṅ and n̈ are fit artifacts and are not used;
/// - inclination change = i_obs - i_prev.
pub fn tle_residuals(
    name: &str,
    history: &[Tle],
) -> Result<Vec<OrbitalParameterResidual>, AppError> {
    let mut elements = history
        .iter()
        .map(|t| parse_tle(name, t))
        .collect::<Result<Vec<_>, _>>()?;
    elements.sort_by_key(|e| e.datetime);
    elements.dedup_by_key(|e| e.datetime);

    let mut rates: Vec<f64> = Vec::with_capacity(elements.len());
    let mut out: Vec<OrbitalParameterResidual> = Vec::with_capacity(elements.len());
    for (k, e) in elements.iter().enumerate() {
        let a = brouwer_mean_semi_major_axis_km(e);
        let (sma_residual, inc_change) = match k.checked_sub(1).map(|p| &elements[p]) {
            Some(prev) => {
                let dt_days = (e.datetime - prev.datetime).num_milliseconds() as f64 / 86_400_000.0;
                let a_prev = out[k - 1].mean_semi_major_axis_km;
                let window = &rates[rates.len().saturating_sub(BASELINE_WINDOW)..];
                let drift = if window.len() >= MIN_BASELINE_SAMPLES {
                    let mut w = window.to_vec();
                    w.sort_by(|x, y| x.total_cmp(y));
                    median(&w)
                } else {
                    0.0
                };
                if dt_days > 0.0 {
                    rates.push((a - a_prev) / dt_days);
                }
                (
                    Some(a - (a_prev + drift * dt_days)),
                    Some(e.inclination - prev.inclination),
                )
            }
            None => (None, None),
        };
        out.push(OrbitalParameterResidual {
            epoch: epoch(e),
            mean_semi_major_axis_km: a,
            inclination_deg: e.inclination,
            mean_motion_revday: e.mean_motion,
            eccentricity: e.eccentricity,
            bstar_drag: e.drag_term,
            semi_major_axis_residual_km: sma_residual,
            inclination_change_deg: inc_change,
            is_anomaly: false,
        });
    }
    Ok(out)
}

/// Candidate maneuvers: consecutive-TLE windows whose SMA residual or inclination change
/// exceeds the thresholds. Only windows ending inside [start, end] are reported.
pub fn reconstruct_maneuvers(
    satellite: &Satellite,
    history: &[Tle],
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    min_sma_change_km: f64,
    min_inc_change_deg: f64,
) -> Result<ManeuversResponse, AppError> {
    let residuals = tle_residuals(&satellite.name, history)?;

    let maneuvers: Vec<DetectedManeuver> = residuals
        .windows(2)
        .filter(|w| start_time.is_none_or(|s| w[1].epoch >= s))
        .filter(|w| end_time.is_none_or(|e| w[1].epoch <= e))
        .filter_map(|w| {
            let da = w[1].semi_major_axis_residual_km?;
            let di = w[1].inclination_change_deg?;
            let sma_flag = da.abs() >= min_sma_change_km;
            let inc_flag = di.abs() >= min_inc_change_deg;
            let maneuver_type = match (sma_flag, inc_flag) {
                (true, true) => ManeuverType::Combined,
                (true, false) if da > 0.0 => ManeuverType::SemiMajorAxisIncrease,
                (true, false) => ManeuverType::SemiMajorAxisDecrease,
                (false, true) => ManeuverType::InclinationChange,
                (false, false) => return None,
            };
            Some(DetectedManeuver {
                window_start: w[0].epoch,
                window_end: w[1].epoch,
                semi_major_axis_residual_km: da,
                inclination_change_deg: di,
                maneuver_type,
            })
        })
        .collect();

    Ok(ManeuversResponse {
        satellite_id: satellite.id,
        satellite_name: satellite.name.clone(),
        tle_epochs_analyzed: residuals.len(),
        total_maneuvers_detected: maneuvers.len(),
        maneuvers,
    })
}

fn median(sorted: &[f64]) -> f64 {
    let m = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[m - 1] + sorted[m]) / 2.0
    } else {
        sorted[m]
    }
}

/// Robust z-scores |x - median| / (1.4826 · MAD). A zero MAD gives 0 for values equal to
/// the median and +inf otherwise.
fn robust_z_scores(xs: &[f64]) -> Vec<f64> {
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    let med = median(&sorted);
    let mut dev: Vec<f64> = xs.iter().map(|x| (x - med).abs()).collect();
    dev.sort_by(f64::total_cmp);
    let scale = 1.4826 * median(&dev);
    xs.iter()
        .map(|x| {
            let d = (x - med).abs();
            if d == 0.0 {
                0.0
            } else {
                d / scale
            }
        })
        .collect()
}

/// Flags residuals that are both outliers against this object's own residual history
/// (robust z-score >= threshold_sigma) and larger than the absolute thresholds.
pub fn detect_anomalies(
    satellite: &Satellite,
    history: &[Tle],
    threshold_sigma: f64,
    min_sma_change_km: f64,
    min_inc_change_deg: f64,
) -> Result<AnomalyDetectionResponse, AppError> {
    let mut residuals = tle_residuals(&satellite.name, history)?;
    let pairs: Vec<(f64, f64)> = residuals
        .iter()
        .filter_map(|r| Some((r.semi_major_axis_residual_km?, r.inclination_change_deg?)))
        .collect();

    if pairs.len() < MIN_RESIDUALS_FOR_STATISTICS {
        return Ok(AnomalyDetectionResponse {
            satellite_id: satellite.id,
            satellite_name: satellite.name.clone(),
            status: AnomalyStatus::InsufficientData,
            anomalies_detected: 0,
            residual_history: residuals,
        });
    }

    let z_sma = robust_z_scores(&pairs.iter().map(|p| p.0).collect::<Vec<_>>());
    let z_inc = robust_z_scores(&pairs.iter().map(|p| p.1).collect::<Vec<_>>());
    // residuals[0] has no predecessor, so pairs[k] belongs to residuals[k + 1].
    for (k, (da, di)) in pairs.iter().enumerate() {
        residuals[k + 1].is_anomaly = (da.abs() >= min_sma_change_km
            && z_sma[k] >= threshold_sigma)
            || (di.abs() >= min_inc_change_deg && z_inc[k] >= threshold_sigma);
    }
    let anomalies_detected = residuals.iter().filter(|r| r.is_anomaly).count();

    Ok(AnomalyDetectionResponse {
        satellite_id: satellite.id,
        satellite_name: satellite.name.clone(),
        status: if anomalies_detected > 0 {
            AnomalyStatus::AnomalyDetected
        } else {
            AnomalyStatus::Nominal
        },
        anomalies_detected,
        residual_history: residuals,
    })
}
