//! Pure CZML packet builders. No propagation, no domain types: callers pass
//! plain times and ECEF kilometres. Frame and interpolation are fixed here so
//! no caller can emit a packet that relies on Cesium defaults (LINEAR degree 1
//! measured at 0.85-85 km error for LEO; see docs/CZML_VIEWER_LOG.md R1).

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};

const FRAME: &str = "FIXED";
const INTERPOLATION: &str = "LAGRANGE";
const DEGREE: u8 = 5;

fn iso(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// `document` packet: version plus a clock spanning `[start, end]`.
pub fn document(name: &str, start: DateTime<Utc>, end: DateTime<Utc>) -> Value {
    json!({
        "id": "document",
        "name": name,
        "version": "1.0",
        "clock": {
            "interval": format!("{}/{}", iso(start), iso(end)),
            "currentTime": iso(start),
            "multiplier": 60,
            "range": "LOOP_STOP",
            "step": "SYSTEM_CLOCK_MULTIPLIER",
        },
    })
}

/// Sampled `position` property from ECEF km samples (converted to metres).
pub fn sampled_position(
    epoch: DateTime<Utc>,
    samples: impl IntoIterator<Item = (DateTime<Utc>, [f64; 3])>,
) -> Value {
    let cartesian: Vec<f64> = samples
        .into_iter()
        .flat_map(|(t, p)| {
            let dt = (t - epoch).num_milliseconds() as f64 / 1000.0;
            [dt, p[0] * 1000.0, p[1] * 1000.0, p[2] * 1000.0]
        })
        .collect();
    json!({
        "epoch": iso(epoch),
        "referenceFrame": FRAME,
        "interpolationAlgorithm": INTERPOLATION,
        "interpolationDegree": DEGREE,
        "cartesian": cartesian,
    })
}

/// `path` property with a solid colour.
pub fn path(rgba: [u8; 4]) -> Value {
    json!({
        "show": true,
        "width": 2,
        "resolution": 120,
        "material": { "solidColor": { "color": { "rgba": rgba } } },
    })
}

/// `availability` interval string.
pub fn availability(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    format!("{}/{}", iso(start), iso(end))
}
