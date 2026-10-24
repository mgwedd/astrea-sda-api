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
    let mut pos = json!({
        "epoch": iso(epoch),
        "referenceFrame": FRAME,
        "cartesian": cartesian,
    });
    // Never ask for more interpolation points than the property has samples: how
    // Cesium treats degree > n-1 is unverified, and one sample needs no interpolation.
    let degree = DEGREE.min((cartesian_len(&pos) / 4).saturating_sub(1) as u8);
    if degree >= 1 {
        pos["interpolationAlgorithm"] = json!(INTERPOLATION);
        pos["interpolationDegree"] = json!(degree);
    }
    pos
}

fn cartesian_len(position: &Value) -> usize {
    position["cartesian"].as_array().map_or(0, Vec::len)
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

/// Static `position` property from ECEF km coordinates.
pub fn fixed_position(pos_km: [f64; 3]) -> Value {
    json!({
        "referenceFrame": FRAME,
        "cartesian": [pos_km[0] * 1000.0, pos_km[1] * 1000.0, pos_km[2] * 1000.0],
    })
}

/// `point` graphic primitive.
pub fn point(pixel_size: f64, rgba: [u8; 4]) -> Value {
    json!({
        "pixelSize": pixel_size,
        "color": { "rgba": rgba },
        "outlineColor": { "rgba": [0, 0, 0, 255] },
        "outlineWidth": 1.0,
    })
}

/// `label` graphic primitive.
pub fn label(text: &str, font: Option<&str>, rgba: [u8; 4]) -> Value {
    json!({
        "text": text,
        "font": font.unwrap_or("11pt sans-serif"),
        "fillColor": { "rgba": rgba },
        "outlineColor": { "rgba": [0, 0, 0, 255] },
        "outlineWidth": 2.0,
        "style": "FILL_AND_OUTLINE",
        "verticalOrigin": "BOTTOM",
        "pixelOffset": { "cartesian2": [0.0, -12.0] },
    })
}

/// `polyline` graphic primitive connecting a list of ECEF km points.
pub fn polyline(positions_km: &[[f64; 3]], rgba: [u8; 4], width: f64) -> Value {
    let cartesian: Vec<f64> = positions_km
        .iter()
        .flat_map(|p| [p[0] * 1000.0, p[1] * 1000.0, p[2] * 1000.0])
        .collect();
    json!({
        "positions": {
            "referenceFrame": FRAME,
            "cartesian": cartesian,
        },
        "material": { "solidColor": { "color": { "rgba": rgba } } },
        "width": width,
    })
}

/// `ellipsoid` 3D volume graphic (e.g. for covariance / collision keep-out zones).
pub fn ellipsoid(radii_km: [f64; 3], rgba: [u8; 4], fill: bool, outline: bool) -> Value {
    json!({
        "radii": {
            "cartesian": [
                radii_km[0] * 1000.0,
                radii_km[1] * 1000.0,
                radii_km[2] * 1000.0
            ],
        },
        "fill": fill,
        "material": { "solidColor": { "color": { "rgba": rgba } } },
        "outline": outline,
        "outlineColor": { "rgba": [rgba[0], rgba[1], rgba[2], 255] },
    })
}

/// `cylinder` 3D volume graphic (e.g. for ground station sensor visibility cones).
#[allow(dead_code)]
pub fn cylinder(length_km: f64, top_radius_km: f64, bottom_radius_km: f64, rgba: [u8; 4]) -> Value {
    json!({
        "length": length_km * 1000.0,
        "topRadius": top_radius_km * 1000.0,
        "bottomRadius": bottom_radius_km * 1000.0,
        "material": { "solidColor": { "color": { "rgba": rgba } } },
        "fill": true,
        "outline": true,
        "outlineColor": { "rgba": [rgba[0], rgba[1], rgba[2], 255] },
    })
}

/// `availability` interval string.
pub fn availability(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    format!("{}/{}", iso(start), iso(end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_position_converts_km_to_meters() {
        let p = fixed_position([7000.0, -1000.0, 500.0]);
        assert_eq!(p["referenceFrame"], "FIXED");
        assert_eq!(
            p["cartesian"].as_array().unwrap(),
            &[7_000_000.0, -1_000_000.0, 500_000.0]
        );
    }

    #[test]
    fn primitives_construct_expected_json_structures() {
        let pt = point(8.0, [255, 0, 0, 255]);
        assert_eq!(pt["pixelSize"], 8.0);
        assert_eq!(pt["color"]["rgba"].as_array().unwrap(), &[255, 0, 0, 255]);

        let lbl = label("Test", None, [0, 255, 0, 255]);
        assert_eq!(lbl["text"], "Test");
        assert_eq!(lbl["font"], "11pt sans-serif");
        assert_eq!(
            lbl["fillColor"]["rgba"].as_array().unwrap(),
            &[0, 255, 0, 255]
        );

        let pl = polyline(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]], [255, 255, 0, 255], 2.0);
        assert_eq!(pl["width"], 2.0);
        assert_eq!(
            pl["positions"]["cartesian"].as_array().unwrap(),
            &[1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0]
        );

        let ell = ellipsoid([10.0, 10.0, 10.0], [255, 0, 0, 100], true, true);
        assert_eq!(
            ell["radii"]["cartesian"].as_array().unwrap(),
            &[10000.0, 10000.0, 10000.0]
        );

        let cyl = cylinder(50.0, 20.0, 0.0, [0, 0, 255, 100]);
        assert_eq!(cyl["length"], 50000.0);
        assert_eq!(cyl["topRadius"], 20000.0);
        assert_eq!(cyl["bottomRadius"], 0.0);
    }
}
