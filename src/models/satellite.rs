use crate::pagination::IdentifiableCheckpoint;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Tle {
    pub line_one: String,
    pub line_two: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Satellite {
    pub id: Uuid,
    pub name: String,
    pub tle: Tle,
    pub created_date: DateTime<Utc>,
    pub last_modified_date: DateTime<Utc>,
}

impl IdentifiableCheckpoint for Satellite {
    fn checkpoint_id(&self) -> Uuid {
        self.id
    }

    fn checkpoint_timestamp(&self) -> i64 {
        self.created_date.timestamp_millis()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSatelliteDto {
    pub name: String,
    #[serde(alias = "tleLineOne")]
    pub line_one: String,
    #[serde(alias = "tleLineTwo")]
    pub line_two: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSatelliteDto {
    pub name: Option<String>,
    #[serde(alias = "tleLineOne")]
    pub line_one: Option<String>,
    #[serde(alias = "tleLineTwo")]
    pub line_two: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GroundPositionQuery {
    pub lat: f64,
    pub lon: f64,
    pub alt: Option<f64>,
    pub time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverheadResponse {
    pub satellite: Satellite,
    pub elevation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NextVisiblePassResponse {
    pub satellite_id: Uuid,
    pub satellite_name: String,
    pub pass_time: DateTime<Utc>,
    pub elevation_deg: f64,
    pub azimuth_deg: f64,
    pub range_km: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroundTrackPoint {
    pub timestamp: DateTime<Utc>,
    pub lat: f64,
    pub lon: f64,
    pub alt_km: f64,
    pub position_ecf_km: [f64; 3],
    pub velocity_ecf_kms: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GeoJsonGeometry {
    pub r#type: String,
    pub coordinates: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GeoJsonFeature {
    pub r#type: String,
    pub geometry: GeoJsonGeometry,
    pub properties: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroundTrackResponse {
    pub satellite_id: Uuid,
    pub satellite_name: String,
    pub orbital_period_minutes: f64,
    pub footprint_radius_km: f64,
    pub duration_minutes: usize,
    pub step_seconds: usize,
    pub trajectory: Vec<GroundTrackPoint>,
    /// Requested sample instants SGP4 could not produce (omitted from `trajectory`)
    pub dropped_samples: usize,
    pub geojson: Option<GeoJsonFeature>,
    pub footprint_polygon: Option<GeoJsonFeature>,
    #[schema(value_type = Option<Vec<Object>>)]
    pub czml: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub enum LightingState {
    FullSunlight,
    Penumbra,
    Umbra,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub enum ObserverTwilightState {
    Daylight,
    CivilTwilight,
    NauticalTwilight,
    AstronomicalTwilight,
    Night,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct IlluminationResponse {
    pub satellite_id: Uuid,
    pub satellite_name: String,
    pub lighting_state: LightingState,
    pub observer_twilight_state: ObserverTwilightState,
    pub observer_sun_elevation_deg: f64,
    pub is_visibly_observable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConjunctionSearchQuery {
    pub max_distance_km: Option<f64>,
    pub duration_hours: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SatelliteSummary {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConjunctionMatch {
    pub satellite_a: SatelliteSummary,
    pub satellite_b: SatelliteSummary,
    pub closest_approach_time: DateTime<Utc>,
    pub min_distance_km: f64,
    pub relative_velocity_kms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision_probability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConjunctionSearchResponse {
    pub search_duration_hours: i64,
    pub max_distance_km: f64,
    pub conjunctions_found: usize,
    pub results: Vec<ConjunctionMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DopplerResponse {
    pub satellite_id: Uuid,
    pub satellite_name: String,
    pub center_freq_hz: f64,
    pub range_rate_kms: f64,
    pub doppler_shift_hz: f64,
    pub corrected_freq_hz: f64,
    pub signal_direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SatellitePass {
    pub pass_id: usize,
    pub aos_time: DateTime<Utc>,
    pub aos_azimuth_deg: f64,
    pub aos_elevation_deg: f64,
    pub tca_time: DateTime<Utc>,
    pub tca_azimuth_deg: f64,
    pub max_elevation_deg: f64,
    pub tca_range_km: f64,
    pub los_time: DateTime<Utc>,
    pub los_azimuth_deg: f64,
    pub los_elevation_deg: f64,
    pub duration_seconds: f64,
    pub is_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual_magnitude: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PassScheduleResponse {
    pub satellite_id: Uuid,
    pub satellite_name: String,
    pub observer_lat: f64,
    pub observer_lon: f64,
    pub observer_alt_m: f64,
    pub elevation_threshold_deg: f64,
    pub forecast_days: usize,
    pub passes_found: usize,
    pub passes: Vec<SatellitePass>,
    /// CZML document with one entity per pass (only when `format=czml`)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Vec<Object>>)]
    pub czml: Option<serde_json::Value>,
    /// Sample instants SGP4 could not produce for the CZML tracks (only when `format=czml`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub czml_dropped_samples: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelativeMotionPoint {
    pub timestamp: DateTime<Utc>,
    pub radial_km: f64,
    pub in_track_km: f64,
    pub cross_track_km: f64,
    pub range_km: f64,
    pub range_rate_kms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RelativeMotionResponse {
    pub primary_satellite: SatelliteSummary,
    pub target_satellite: SatelliteSummary,
    pub epoch: DateTime<Utc>,
    pub relative_distance_km: f64,
    pub range_rate_kms: f64,
    pub radial_distance_km: f64,
    pub in_track_distance_km: f64,
    pub cross_track_distance_km: f64,
    pub radial_velocity_kms: f64,
    pub in_track_velocity_kms: f64,
    pub cross_track_velocity_kms: f64,
    pub rpo_regime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trajectory: Option<Vec<RelativeMotionPoint>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeplerianElements {
    pub semi_major_axis_km: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub arg_of_perigee_deg: f64,
    pub true_anomaly_deg: f64,
    pub mean_anomaly_deg: f64,
    pub orbital_period_minutes: f64,
    pub perigee_altitude_km: f64,
    pub apogee_altitude_km: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SatelliteStateResponse {
    pub satellite_id: Uuid,
    pub satellite_name: String,
    pub epoch: DateTime<Utc>,
    pub position_eci_km: [f64; 3],
    pub velocity_eci_kms: [f64; 3],
    pub position_ecef_km: [f64; 3],
    pub velocity_ecef_kms: [f64; 3],
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_km: f64,
    pub keplerian_elements: KeplerianElements,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SatelliteDecayRiskResponse {
    pub satellite_id: Uuid,
    pub satellite_name: String,
    pub perigee_altitude_km: f64,
    pub apogee_altitude_km: f64,
    pub bstar_drag: f64,
    pub mean_motion_derivative_rev_day2: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orbital_lifetime_days_estimate: Option<f64>,
    pub decay_status: String,
    pub reentry_risk_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DecayWatchResponse {
    pub scanned_satellites_count: usize,
    pub decaying_satellites_found: usize,
    pub threshold_perigee_km: f64,
    pub objects: Vec<SatelliteDecayRiskResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CollisionProbabilityRequest {
    pub miss_distance_km: f64,
    pub relative_velocity_kms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_body_radius_m: Option<f64>,
    /// Principal-axis 1-sigma position uncertainty in the encounter plane (metres), major axis
    pub sigma_1_m: f64,
    /// Principal-axis 1-sigma position uncertainty in the encounter plane (metres), minor axis
    pub sigma_2_m: f64,
    /// Angle of the miss vector from the sigma_1 axis in degrees (default 0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub miss_angle_deg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CollisionProbabilityResponse {
    pub miss_distance_km: f64,
    pub relative_velocity_kms: f64,
    pub hard_body_radius_m: f64,
    pub sigma_1_m: f64,
    pub sigma_2_m: f64,
    pub miss_angle_deg: f64,
    pub collision_probability: f64,
    pub risk_category: String,
    pub recommendation: String,
}
