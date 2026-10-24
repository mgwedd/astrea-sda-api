//! CZML output for Cesium. `packet` holds the stateless builders; this module
//! composes them into documents from sampled trajectories.

mod packet;

use crate::error::AppError;
use crate::models::{GroundTrackPoint, Satellite, SatellitePass, SceneLayer, SceneRequest};
use crate::services::ephemeris::{Ephemeris, Samples};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Largest sample spacing for which degree-5 Lagrange stays under ~1 m for
/// LEO and Molniya (measured, docs/CZML_VIEWER_LOG.md R1).
pub const MAX_STEP_SECS: usize = 120;

/// Maximum layers per composed scene.
pub const MAX_SCENE_LAYERS: usize = 20;
/// Maximum duration in hours for a composed scene.
pub const MAX_SCENE_DURATION_HOURS: f64 = 24.0;
/// Maximum samples per track in a scene.
pub const MAX_SAMPLES_PER_TRACK: usize = 2000;

/// Rejects sample spacing too coarse for the interpolation we declare.
pub fn check_step(step_seconds: usize) -> Result<(), AppError> {
    if step_seconds > MAX_STEP_SECS {
        return Err(AppError::BadRequest(format!(
            "step_seconds must be <= {MAX_STEP_SECS} for CZML output"
        )));
    }
    Ok(())
}

/// Validates scene parameters and caps before calculation.
pub fn check_scene_request(req: &SceneRequest) -> Result<(), AppError> {
    if req.layers.is_empty() {
        return Err(AppError::BadRequest(
            "Scene must contain at least 1 layer".into(),
        ));
    }
    if req.layers.len() > MAX_SCENE_LAYERS {
        return Err(AppError::BadRequest(format!(
            "Scene exceeds maximum of {MAX_SCENE_LAYERS} layers"
        )));
    }
    let duration_hours = req.duration_hours.unwrap_or(6.0);
    if duration_hours <= 0.0 || duration_hours > MAX_SCENE_DURATION_HOURS {
        return Err(AppError::BadRequest(format!(
            "duration_hours must be between 0.0 and {MAX_SCENE_DURATION_HOURS}"
        )));
    }
    let step_seconds = req.step_seconds.unwrap_or(30);
    if step_seconds == 0 || step_seconds > MAX_STEP_SECS {
        return Err(AppError::BadRequest(format!(
            "step_seconds must be between 1 and {MAX_STEP_SECS} for CZML output"
        )));
    }
    let samples = (duration_hours * 3600.0 / step_seconds as f64).ceil() as usize;
    if samples > MAX_SAMPLES_PER_TRACK {
        return Err(AppError::BadRequest(format!(
            "Samples per track ({samples}) exceeds maximum of {MAX_SAMPLES_PER_TRACK} (increase step_seconds or reduce duration)"
        )));
    }
    Ok(())
}

const TRACK_RGBA: [u8; 4] = [0, 255, 255, 255];

struct Track<'a> {
    id: String,
    name: &'a str,
    points: Vec<GroundTrackPoint>,
}

/// One document, one entity per non-empty track; clock spans all tracks.
fn tracks_document(title: &str, tracks: &[Track]) -> Value {
    let bounds = tracks
        .iter()
        .filter_map(|t| Some((t.points.first()?.timestamp, t.points.last()?.timestamp)));
    let (Some(start), Some(end)) = (bounds.clone().map(|b| b.0).min(), bounds.map(|b| b.1).max())
    else {
        return json!([]);
    };
    let mut doc = vec![packet::document(title, start, end)];
    for t in tracks.iter().filter(|t| !t.points.is_empty()) {
        let (first, last) = (
            t.points[0].timestamp,
            t.points[t.points.len() - 1].timestamp,
        );
        doc.push(json!({
            "id": t.id,
            "name": t.name,
            "availability": packet::availability(first, last),
            "position": packet::sampled_position(
                first,
                t.points.iter().map(|p| (p.timestamp, p.position_ecf_km)),
            ),
            "path": packet::path(TRACK_RGBA),
        }));
    }
    Value::Array(doc)
}

/// Single-satellite trajectory document. Empty input yields an empty document.
pub fn trajectory_document(id: Uuid, name: &str, points: &[GroundTrackPoint]) -> Value {
    let track = Track {
        id: id.to_string(),
        name,
        points: points.to_vec(),
    };
    tracks_document(&format!("Trajectory for {name}"), &[track])
}

/// Sample spacing inside a pass; well under `MAX_STEP_SECS`.
const PASS_STEP_SECS: usize = 30;

/// One entity per pass, sampled over [AOS, LOS] (re-propagated; passes carry
/// events only). Also returns how many sample instants SGP4 could not produce. Pass count and window follow the passes endpoint, not the
/// scene caps.
pub fn passes_document(
    satellite: &Satellite,
    passes: &[SatellitePass],
    cancel: &CancellationToken,
) -> Result<(Value, usize), AppError> {
    let eph = Ephemeris::from_satellite(satellite)?;
    let mut dropped_total = 0;
    let tracks = passes
        .iter()
        .map(|p| {
            let span = (p.los_time - p.aos_time).num_seconds().max(1) as usize;
            let Samples {
                mut points,
                dropped,
            } = eph.range(p.aos_time, span, PASS_STEP_SECS, cancel)?;
            dropped_total += dropped;
            // range() floors span/step; make sure the track ends exactly at LOS.
            if points.last().is_some_and(|l| l.timestamp < p.los_time) {
                match eph.at(p.los_time) {
                    Some(last) => points.push(last),
                    None => dropped_total += 1,
                }
            }
            Ok(Track {
                id: format!("{}-pass-{}", satellite.id, p.pass_id),
                name: &satellite.name,
                points,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let doc = tracks_document(&format!("Passes for {}", satellite.name), &tracks);
    Ok((doc, dropped_total))
}

/// Composes multiple scene layers into a single cohesive CZML document.
pub fn compose_scene(
    name: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: usize,
    layers: &[SceneLayer],
    satellites: &HashMap<Uuid, Satellite>,
    cancel: &CancellationToken,
) -> Result<(Vec<Value>, usize), AppError> {
    let mut doc = vec![packet::document(name, start, end)];
    let mut dropped_total = 0;

    for (layer_idx, layer) in layers.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(AppError::InternalServerError(
                "Computation cancelled".into(),
            ));
        }

        match layer {
            SceneLayer::Groundtrack {
                satellite_id,
                label,
                rgba,
            } => {
                let satellite = satellites.get(satellite_id).ok_or(AppError::NotFound)?;
                let eph = Ephemeris::from_satellite(satellite)?;
                let span = (end - start).num_seconds().max(1) as usize;
                let Samples {
                    mut points,
                    dropped,
                } = eph.range(start, span, step_seconds, cancel)?;
                dropped_total += dropped;
                if points.last().is_some_and(|l| l.timestamp < end) {
                    match eph.at(end) {
                        Some(last) => points.push(last),
                        None => dropped_total += 1,
                    }
                }
                if !points.is_empty() {
                    let (first, last) = (points[0].timestamp, points[points.len() - 1].timestamp);
                    let display_name = label.as_deref().unwrap_or(&satellite.name);
                    let color = rgba.unwrap_or(TRACK_RGBA);
                    doc.push(json!({
                        "id": format!("{satellite_id}-layer-{layer_idx}"),
                        "name": display_name,
                        "availability": packet::availability(first, last),
                        "position": packet::sampled_position(
                            first,
                            points.iter().map(|p| (p.timestamp, p.position_ecf_km)),
                        ),
                        "path": packet::path(color),
                    }));
                }
            }
            SceneLayer::Passes {
                satellite_id,
                observer,
                min_elevation,
                label,
            } => {
                let satellite = satellites.get(satellite_id).ok_or(AppError::NotFound)?;
                let eph = Ephemeris::from_satellite(satellite)?;
                let min_el = min_elevation.unwrap_or(10.0);
                let alt_m = observer.alt_m.unwrap_or(0.0);
                let days = ((end - start).num_seconds() as f64 / 86400.0)
                    .ceil()
                    .max(1.0) as usize;
                let sched = crate::services::astrodynamics::find_pass_schedule(
                    satellite,
                    observer.lat,
                    observer.lon,
                    alt_m,
                    start,
                    min_el,
                    days,
                    false,
                    cancel,
                )?;

                for p in &sched.passes {
                    if p.los_time <= start || p.aos_time >= end {
                        continue;
                    }
                    let pass_start = p.aos_time.max(start);
                    let pass_end = p.los_time.min(end);
                    let span = (pass_end - pass_start).num_seconds().max(1) as usize;
                    let Samples {
                        mut points,
                        dropped,
                    } = eph.range(pass_start, span, PASS_STEP_SECS, cancel)?;
                    dropped_total += dropped;
                    if points.last().is_some_and(|l| l.timestamp < pass_end) {
                        match eph.at(pass_end) {
                            Some(last) => points.push(last),
                            None => dropped_total += 1,
                        }
                    }
                    if !points.is_empty() {
                        let display_name = format!(
                            "{} - Pass {}",
                            label.as_deref().unwrap_or(&satellite.name),
                            p.pass_id
                        );
                        doc.push(json!({
                            "id": format!("{satellite_id}-layer-{layer_idx}-pass-{}", p.pass_id),
                            "name": display_name,
                            "availability": packet::availability(pass_start, pass_end),
                            "position": packet::sampled_position(
                                pass_start,
                                points.iter().map(|pt| (pt.timestamp, pt.position_ecf_km)),
                            ),
                            "path": packet::path([255, 165, 0, 255]),
                        }));
                    }
                }
            }
        }
    }

    Ok((doc, dropped_total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn position_states_frame_and_interpolation_explicitly() {
        let t = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let samples = (0..9).map(|i| (t + chrono::Duration::seconds(30 * i), [7000.0, 0.0, 0.0]));
        let pos = packet::sampled_position(t, samples);
        assert_eq!(pos["referenceFrame"], "FIXED");
        assert_eq!(pos["interpolationAlgorithm"], "LAGRANGE");
        assert_eq!(pos["interpolationDegree"], 5);
        assert_eq!(pos["cartesian"][1], 7_000_000.0);
        assert_eq!(pos["cartesian"].as_array().unwrap().len(), 36);
    }

    #[test]
    fn degree_never_exceeds_sample_count() {
        let t = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let at = |n: i64| {
            let s = (0..n).map(|i| (t + chrono::Duration::seconds(30 * i), [7000.0, 0.0, 0.0]));
            packet::sampled_position(t, s)
        };
        assert_eq!(at(9)["interpolationDegree"], 5);
        assert_eq!(at(6)["interpolationDegree"], 5);
        assert_eq!(at(4)["interpolationDegree"], 3);
        assert_eq!(at(2)["interpolationDegree"], 1);
        // A single sample is a constant: no interpolation properties at all.
        assert!(at(1).get("interpolationAlgorithm").is_none());
        assert!(at(1).get("interpolationDegree").is_none());
    }

    #[test]
    fn empty_input_is_empty_document() {
        assert_eq!(trajectory_document(Uuid::nil(), "x", &[]), json!([]));
    }

    #[test]
    fn each_pass_is_an_entity_ending_at_los() {
        use crate::services::astrodynamics::find_pass_schedule;
        let sat = Satellite {
            id: Uuid::nil(),
            name: "ISS".into(),
            tle: crate::models::Tle {
                line_one: "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996"
                    .into(),
                line_two: "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012"
                    .into(),
            },
            created_date: Utc::now(),
            last_modified_date: Utc::now(),
        };
        let t0 = Utc.with_ymd_and_hms(2024, 3, 25, 0, 0, 0).unwrap();
        let live = CancellationToken::new();
        let sched = find_pass_schedule(&sat, 40.0, -75.0, 0.0, t0, 5.0, 1, false, &live).unwrap();
        assert!(!sched.passes.is_empty());
        let (doc, dropped) = passes_document(&sat, &sched.passes, &live).unwrap();
        assert_eq!(dropped, 0);
        let doc = doc.as_array().unwrap();
        assert_eq!(doc.len(), sched.passes.len() + 1);
        let pass = &sched.passes[0];
        let cart = doc[1]["position"]["cartesian"].as_array().unwrap();
        let last_dt = cart[cart.len() - 4].as_f64().unwrap();
        let span = (pass.los_time - pass.aos_time).num_milliseconds() as f64 / 1000.0;
        assert!((last_dt - span).abs() < 1e-6);
    }
}
