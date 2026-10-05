//! TLE → ECEF sampling. One place that parses a satellite's TLE and turns
//! times into `GroundTrackPoint`s, shared by ground track and pass CZML.

use crate::error::AppError;
use crate::models::{GroundTrackPoint, Satellite};
use crate::services::astrodynamics::{
    calculate_local_sidereal_time, ecf_to_geodetic, eci_to_ecf, eci_to_ecf_velocity,
    normalize_tle_line,
};
use chrono::{DateTime, Duration, Utc};
use sgp4::{Constants, Elements};

/// Longest span of a single sampled track.
pub const MAX_SPAN_SECS: usize = 24 * 3600;
/// Most samples in a single track.
pub const MAX_SAMPLES: usize = 2000;

pub struct Ephemeris {
    constants: Constants,
    epoch: DateTime<Utc>,
    pub mean_motion_revs_day: f64,
}

/// Samples plus how many requested instants SGP4 could not produce.
pub struct Samples {
    pub points: Vec<GroundTrackPoint>,
    pub dropped: usize,
}

impl Ephemeris {
    pub fn from_satellite(satellite: &Satellite) -> Result<Self, AppError> {
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
        Ok(Self {
            constants,
            epoch: DateTime::<Utc>::from_naive_utc_and_offset(elements.datetime, Utc),
            mean_motion_revs_day: elements.mean_motion,
        })
    }

    /// State at `t`, or `None` if SGP4 fails or yields a non-finite value.
    pub fn at(&self, t: DateTime<Utc>) -> Option<GroundTrackPoint> {
        let minutes = (t - self.epoch).num_milliseconds() as f64 / 60000.0;
        let pred = self.constants.propagate(minutes).ok()?;
        let lst = calculate_local_sidereal_time(t, 0.0);
        let pos = eci_to_ecf(pred.position, lst);
        let vel = eci_to_ecf_velocity(pred.position, pred.velocity, lst);
        let (lat, lon, alt_km) = ecf_to_geodetic(pos);
        let finite = pos.iter().chain(&vel).all(|v| v.is_finite());
        finite.then_some(GroundTrackPoint {
            timestamp: t,
            lat,
            lon,
            alt_km,
            position_ecf_km: pos,
            velocity_ecf_kms: vel,
        })
    }

    /// Samples `start ..= start + span_secs` every `step_secs`. Rejects
    /// requests over `MAX_SPAN_SECS` / `MAX_SAMPLES` rather than clamping.
    pub fn range(
        &self,
        start: DateTime<Utc>,
        span_secs: usize,
        step_secs: usize,
    ) -> Result<Samples, AppError> {
        if step_secs == 0 || span_secs == 0 {
            return Err(AppError::BadRequest(
                "duration and step must be positive".into(),
            ));
        }
        if span_secs > MAX_SPAN_SECS {
            return Err(AppError::BadRequest(format!(
                "duration exceeds {} h",
                MAX_SPAN_SECS / 3600
            )));
        }
        let n = span_secs / step_secs + 1;
        if n > MAX_SAMPLES {
            return Err(AppError::BadRequest(format!(
                "{n} samples exceeds the {MAX_SAMPLES} limit; increase step_seconds"
            )));
        }
        let points: Vec<_> = (0..n)
            .filter_map(|i| self.at(start + Duration::seconds((i * step_secs) as i64)))
            .collect();
        Ok(Samples {
            dropped: n - points.len(),
            points,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn eph() -> Ephemeris {
        let sat = Satellite {
            id: uuid::Uuid::nil(),
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
        Ephemeris::from_satellite(&sat).unwrap_or_else(|_| panic!("fixture TLE"))
    }

    #[test]
    fn rejects_over_cap_instead_of_clamping() {
        let e = eph();
        let t = Utc.with_ymd_and_hms(2024, 3, 25, 12, 0, 0).unwrap();
        assert!(e.range(t, MAX_SPAN_SECS + 1, 60).is_err());
        assert!(e.range(t, 3600, 1).is_err()); // 3601 samples
        assert!(e.range(t, 3600, 60).is_ok());
    }
}
