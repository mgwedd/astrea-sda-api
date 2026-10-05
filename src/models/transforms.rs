use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoordinateFrame {
    EciTeme,
    EcefWgs84,
    TopocentricSez,
    TopocentricNed,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CartesianState {
    pub position_km: [f64; 3],
    pub velocity_kms: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClassicalKeplerianElements {
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
pub struct ModifiedEquinoctialElements {
    pub p_km: f64,
    pub f: f64,
    pub g: f64,
    pub h: f64,
    pub k: f64,
    pub true_longitude_deg: f64,
    pub mean_longitude_deg: f64,
    pub retrograde_factor: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeplerianInput {
    pub semi_major_axis_km: f64,
    pub eccentricity: f64,
    pub inclination_deg: f64,
    pub raan_deg: f64,
    pub arg_of_perigee_deg: f64,
    pub true_anomaly_deg: Option<f64>,
    pub mean_anomaly_deg: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EquinoctialInput {
    pub p_km: f64,
    pub f: f64,
    pub g: f64,
    pub h: f64,
    pub k: f64,
    pub true_longitude_deg: Option<f64>,
    pub mean_longitude_deg: Option<f64>,
    pub retrograde_factor: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ElementTransformRequest {
    pub cartesian: Option<CartesianState>,
    pub keplerian: Option<KeplerianInput>,
    pub equinoctial: Option<EquinoctialInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ElementTransformResponse {
    pub cartesian: CartesianState,
    pub keplerian: ClassicalKeplerianElements,
    pub equinoctial: ModifiedEquinoctialElements,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GeodeticCoordinates {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_km: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LookAnglesDetail {
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    pub slant_range_km: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_rate_kms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FrameTransformRequest {
    pub source_frame: CoordinateFrame,
    pub target_frame: CoordinateFrame,
    pub epoch: DateTime<Utc>,
    pub position: [f64; 3],
    pub velocity: Option<[f64; 3]>,
    pub observer_lat_deg: Option<f64>,
    pub observer_lon_deg: Option<f64>,
    pub observer_alt_m: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FrameTransformResponse {
    pub source_frame: CoordinateFrame,
    pub target_frame: CoordinateFrame,
    pub epoch: DateTime<Utc>,
    pub position: [f64; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geodetic: Option<GeodeticCoordinates>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub look_angles: Option<LookAnglesDetail>,
}
