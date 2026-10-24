use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Query parameters for scene generation.
#[derive(Debug, Deserialize, IntoParams)]
pub struct SceneQueryParams {
    /// Format of the response: `json` (default, returns `SceneResponse`) or `czml` (returns raw CZML array).
    pub format: Option<String>,
}

/// Observer geodetic location for ground station pass layers.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SceneObserverLocation {
    /// Observer latitude in decimal degrees (-90.0 to 90.0)
    pub lat: f64,
    /// Observer longitude in decimal degrees (-180.0 to 180.0)
    pub lon: f64,
    /// Observer altitude above sea level in meters (defaults to 0.0)
    pub alt_m: Option<f64>,
}

/// Discriminated union of scene layer specifications.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SceneLayer {
    /// 3D orbital trajectory / ground track for a satellite
    Groundtrack {
        /// Unique satellite identifier
        satellite_id: Uuid,
        /// Optional human-readable display label
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        /// Optional RGBA color override [r, g, b, a] with values 0-255
        #[serde(skip_serializing_if = "Option::is_none")]
        rgba: Option<[u8; 4]>,
    },
    /// Ground station pass visibility intervals and topocentric tracks
    Passes {
        /// Unique satellite identifier
        satellite_id: Uuid,
        /// Observer ground station geodetic location
        observer: SceneObserverLocation,
        /// Minimum elevation angle threshold in degrees (defaults to 10.0)
        #[serde(skip_serializing_if = "Option::is_none")]
        min_elevation: Option<f64>,
        /// Optional human-readable display label
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

/// Top-level request payload for composing a 3D/4D CZML scene.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SceneRequest {
    /// Calculation start epoch in UTC (defaults to current time if omitted)
    pub start: Option<DateTime<Utc>>,
    /// Scene duration in hours (0.1 to 24.0, defaults to 6.0)
    pub duration_hours: Option<f64>,
    /// Ephemeris trajectory sample spacing in seconds (1 to 120, defaults to 30)
    pub step_seconds: Option<usize>,
    /// List of renderable layers in the scene (1 to 20 layers)
    pub layers: Vec<SceneLayer>,
}

/// Structured response for a composed scene.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SceneResponse {
    /// Composed CZML document containing the document packet followed by layer entity packets
    #[schema(value_type = Vec<Object>)]
    pub czml: Vec<serde_json::Value>,
    /// Total count of sample instants SGP4 could not produce across all layers
    pub dropped_samples: usize,
    /// Total count of successfully composed layers in the scene
    pub layer_count: usize,
    /// Scene calculation time window start in UTC
    pub start: DateTime<Utc>,
    /// Scene calculation time window end in UTC
    pub end: DateTime<Utc>,
}
