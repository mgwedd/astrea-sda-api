use axum::{extract::State, Json};
use chrono::Utc;

use crate::auth::{Claims, UserRole};
use crate::error::AppError;
use crate::handlers::satellite_handler::map_calculation_error;
use crate::models::{
    SensorDefinition, SensorScheduleRequest, SensorScheduleResponse, SingleSensorPassesRequest,
};
use crate::repository::SatelliteRepository;
use crate::services::sensor::{find_sensor_passes, schedule_sensor_network};

const PASS_DAY_COST_MS: u64 = 3;

fn validate_sensor(sensor: &SensorDefinition) -> Result<(), AppError> {
    if sensor.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Sensor name cannot be empty".to_string(),
        ));
    }
    if !(-90.0..=90.0).contains(&sensor.lat) {
        return Err(AppError::BadRequest(format!(
            "Sensor '{}': latitude must be between -90.0 and 90.0 degrees",
            sensor.name
        )));
    }
    if !(-180.0..=180.0).contains(&sensor.lon) {
        return Err(AppError::BadRequest(format!(
            "Sensor '{}': longitude must be between -180.0 and 180.0 degrees",
            sensor.name
        )));
    }
    if !(-500.0..=100_000.0).contains(&sensor.alt_m) {
        return Err(AppError::BadRequest(format!(
            "Sensor '{}': altitude must be between -500 and 100,000 metres",
            sensor.name
        )));
    }
    if let Some(mask) = &sensor.horizon_mask {
        if mask.len() > 360 {
            return Err(AppError::BadRequest(format!(
                "Sensor '{}': horizon mask cannot exceed 360 points",
                sensor.name
            )));
        }
        for (idx, p) in mask.iter().enumerate() {
            if !(-10.0..=90.0).contains(&p.elevation_deg) {
                return Err(AppError::BadRequest(format!(
                    "Sensor '{}': mask point {} elevation must be between -10.0 and 90.0 degrees",
                    sensor.name, idx
                )));
            }
        }
    }
    if let Some(max_el) = sensor.max_elevation_deg {
        if !(0.0..=90.0).contains(&max_el) {
            return Err(AppError::BadRequest(format!(
                "Sensor '{}': maxElevationDeg must be between 0.0 and 90.0 degrees",
                sensor.name
            )));
        }
    }
    if let Some(min_el) = sensor.min_elevation_deg {
        if !(-10.0..=89.0).contains(&min_el) {
            return Err(AppError::BadRequest(format!(
                "Sensor '{}': minElevationDeg must be between -10.0 and 89.0 degrees",
                sensor.name
            )));
        }
    }
    if let Some(rate) = sensor.max_azimuth_rate_deg_s {
        if rate <= 0.0 {
            return Err(AppError::BadRequest(format!(
                "Sensor '{}': maxAzimuthRateDegS must be positive",
                sensor.name
            )));
        }
    }
    if let Some(rate) = sensor.max_elevation_rate_deg_s {
        if rate <= 0.0 {
            return Err(AppError::BadRequest(format!(
                "Sensor '{}': maxElevationRateDegS must be positive",
                sensor.name
            )));
        }
    }
    Ok(())
}

#[utoipa::path(
    post,
    path = "/v1/sensors/schedule",
    operation_id = "scheduleSensorNetwork",
    request_body = SensorScheduleRequest,
    responses(
        (status = 200, description = "Sensor network tracking pass schedule and handovers", body = SensorScheduleResponse),
        (status = 400, description = "Invalid sensor parameters or limits exceeded", body = ErrorResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 403, description = "Forbidden - Insufficient permissions", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Sensors"
)]
pub async fn schedule_sensor_network_handler(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Json(req): Json<SensorScheduleRequest>,
) -> Result<Json<SensorScheduleResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;

    if req.sensors.is_empty() {
        return Err(AppError::BadRequest(
            "At least one sensor must be provided in schedule request".to_string(),
        ));
    }
    if req.sensors.len() > 10 {
        return Err(AppError::BadRequest(
            "Maximum 10 sensors allowed per schedule request".to_string(),
        ));
    }
    if req.satellite_ids.is_empty() {
        return Err(AppError::BadRequest(
            "At least one satellite ID must be provided in schedule request".to_string(),
        ));
    }
    if req.satellite_ids.len() > 20 {
        return Err(AppError::BadRequest(
            "Maximum 20 satellites allowed per schedule request".to_string(),
        ));
    }

    let duration_days = req.duration_days.unwrap_or(3.0);
    if !(0.1..=14.0).contains(&duration_days) {
        return Err(AppError::BadRequest(
            "durationDays must be between 0.1 and 14.0 days".to_string(),
        ));
    }

    for sensor in &req.sensors {
        validate_sensor(sensor)?;
    }

    let mut satellites = Vec::with_capacity(req.satellite_ids.len());
    for id in &req.satellite_ids {
        let sat = repo.get_satellite_by_id(*id).await?;
        satellites.push(sat);
    }

    let start_time = req.start.unwrap_or_else(Utc::now);
    let days_ceil = duration_days.ceil() as u64;
    let est_cost_ms =
        (req.sensors.len() * req.satellite_ids.len()) as u64 * days_ceil * PASS_DAY_COST_MS;

    let engine = repo.compute.clone();
    let (user_id, role) = (claims.sub.clone(), claims.canonical_role());

    let sensors = req.sensors;
    let res = engine
        .run(est_cost_ms, user_id, role, move |cancel| {
            schedule_sensor_network(&satellites, &sensors, start_time, duration_days, &cancel)
        })
        .await
        .map_err(map_calculation_error)??;

    Ok(Json(res))
}

#[utoipa::path(
    post,
    path = "/v1/sensors/passes",
    operation_id = "evaluateSensorPasses",
    request_body = SingleSensorPassesRequest,
    responses(
        (status = 200, description = "Satellite tracking passes over the sensor", body = SensorScheduleResponse),
        (status = 400, description = "Invalid sensor or satellite parameters", body = ErrorResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 403, description = "Forbidden - Insufficient permissions", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Sensors"
)]
pub async fn evaluate_sensor_passes_handler(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Json(req): Json<SingleSensorPassesRequest>,
) -> Result<Json<SensorScheduleResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    validate_sensor(&req.sensor)?;

    let duration_days = req.duration_days.unwrap_or(3.0);
    if !(0.1..=14.0).contains(&duration_days) {
        return Err(AppError::BadRequest(
            "durationDays must be between 0.1 and 14.0 days".to_string(),
        ));
    }

    let satellite = repo.get_satellite_by_id(req.satellite_id).await?;
    let start_time = req.start.unwrap_or_else(Utc::now);
    let days_ceil = duration_days.ceil() as u64;
    let est_cost_ms = days_ceil * PASS_DAY_COST_MS;

    let engine = repo.compute.clone();
    let (user_id, role) = (claims.sub.clone(), claims.canonical_role());

    let sensor = req.sensor;
    let passes = engine
        .run(est_cost_ms, user_id, role, move |cancel| {
            find_sensor_passes(&satellite, &sensor, start_time, duration_days, &cancel)
        })
        .await
        .map_err(map_calculation_error)??;

    let end_time =
        start_time + chrono::Duration::milliseconds((duration_days * 86_400_000.0) as i64);
    let total_passes = passes.len();

    Ok(Json(SensorScheduleResponse {
        start: start_time,
        end: end_time,
        total_passes,
        passes,
        handovers: Vec::new(),
    }))
}
