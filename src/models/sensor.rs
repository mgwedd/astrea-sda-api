use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// 2D azimuth-elevation coordinate defining a point on a terrain horizon profile.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HorizonMaskPoint {
    /// True North-referenced azimuth in degrees [0.0, 360.0)
    pub azimuth_deg: f64,
    /// Minimum local horizon elevation angle in degrees [-10.0, 90.0]
    pub elevation_deg: f64,
}

/// Definition of a ground sensor (optical telescope, radar, or tracking station).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SensorDefinition {
    /// Optional sensor unique identifier (generated if omitted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    /// Human-readable sensor name
    pub name: String,
    /// Geodetic latitude in degrees [-90.0, 90.0]
    pub lat: f64,
    /// Geodetic longitude in degrees [-180.0, 180.0]
    pub lon: f64,
    /// Altitude above WGS-84 reference ellipsoid in metres
    pub alt_m: f64,
    /// Default minimum elevation cutoff in degrees (defaults to 10.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_elevation_deg: Option<f64>,
    /// Maximum mount elevation angle in degrees (e.g. 85.0 to model zenith keyhole / dome limits)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_elevation_deg: Option<f64>,
    /// Maximum mount azimuth slew rate in deg/s (e.g. 15.0 deg/s)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_azimuth_rate_deg_s: Option<f64>,
    /// Maximum mount elevation slew rate in deg/s (e.g. 5.0 deg/s)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_elevation_rate_deg_s: Option<f64>,
    /// Custom 360° terrain horizon elevation profile
    #[serde(skip_serializing_if = "Option::is_none")]
    pub horizon_mask: Option<Vec<HorizonMaskPoint>>,
    /// Whether only sunlit passes during local darkness are tracked
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optical_only: Option<bool>,
}

/// Angular tracking rates and radial Doppler velocity at an instant.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackingRates {
    /// Slant range rate in km/s (negative = approaching, positive = receding)
    pub range_rate_kms: f64,
    /// Azimuth angular tracking rate in deg/s
    pub azimuth_rate_deg_s: f64,
    /// Elevation angular tracking rate in deg/s
    pub elevation_rate_deg_s: f64,
}

/// Scheduled satellite tracking pass over a specific ground sensor.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledSensorPass {
    pub pass_id: usize,
    pub sensor_id: Uuid,
    pub sensor_name: String,
    pub satellite_id: Uuid,
    pub satellite_name: String,
    pub aos_time: DateTime<Utc>,
    pub aos_azimuth_deg: f64,
    pub aos_elevation_deg: f64,
    pub tca_time: DateTime<Utc>,
    pub tca_azimuth_deg: f64,
    pub max_elevation_deg: f64,
    pub tca_range_km: f64,
    pub tca_range_rate_kms: f64,
    pub tca_azimuth_rate_deg_s: f64,
    pub tca_elevation_rate_deg_s: f64,
    /// Peak azimuth rate experienced during the pass in deg/s
    pub peak_azimuth_rate_deg_s: f64,
    /// Peak elevation rate experienced during the pass in deg/s
    pub peak_elevation_rate_deg_s: f64,
    /// True if pass rates and elevations stay within sensor mount mechanical limits
    pub is_mount_feasible: bool,
    /// True if satellite is illuminated and observer is in twilight/darkness
    pub is_optical_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual_magnitude: Option<f64>,
    pub los_time: DateTime<Utc>,
    pub los_azimuth_deg: f64,
    pub los_elevation_deg: f64,
    pub duration_seconds: f64,
}

/// Transition or overlap event between two sensors tracking the same satellite.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SensorHandover {
    pub satellite_id: Uuid,
    pub satellite_name: String,
    pub from_sensor_id: Uuid,
    pub from_sensor_name: String,
    pub to_sensor_id: Uuid,
    pub to_sensor_name: String,
    pub handover_time: DateTime<Utc>,
    /// Overlap duration in seconds (positive = simultaneous dual-station coverage, negative = gap)
    pub overlap_seconds: f64,
}

/// Request payload for single-sensor pass evaluation with custom horizon and mount limits.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SingleSensorPassesRequest {
    pub sensor: SensorDefinition,
    pub satellite_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_days: Option<f64>,
}

/// Request payload for multi-station sensor network tracking schedule generation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SensorScheduleRequest {
    /// List of ground stations / optical sensors in the network (max 10)
    pub sensors: Vec<SensorDefinition>,
    /// List of satellite IDs to schedule (max 20)
    pub satellite_ids: Vec<Uuid>,
    /// Start epoch for schedule (defaults to current time if omitted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<Utc>>,
    /// Forecast window duration in days [0.1, 14.0] (defaults to 3.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_days: Option<f64>,
}

/// Response payload containing network schedule and handover events.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SensorScheduleResponse {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub total_passes: usize,
    pub passes: Vec<ScheduledSensorPass>,
    pub handovers: Vec<SensorHandover>,
}
