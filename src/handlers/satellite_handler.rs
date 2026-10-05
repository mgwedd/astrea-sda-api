use crate::auth::{Claims, UserRole};
use crate::error::AppError;
use crate::models::{
    AnomalyDetectionRequest, AnomalyDetectionResponse, CollisionProbabilityRequest,
    CollisionProbabilityResponse, ConjunctionSearchResponse, CreateSatelliteDto,
    DecayWatchResponse, DopplerResponse, ElementTransformRequest, ElementTransformResponse,
    FrameTransformRequest, FrameTransformResponse, GroundTrackResponse, IlluminationResponse,
    ManeuverQueryParams, ManeuversResponse, NextVisiblePassResponse, OverheadResponse,
    PassScheduleResponse, RelativeMotionResponse, Satellite, SatelliteDecayRiskResponse,
    SatelliteStateResponse, TransitPredictionResponse, TransitQueryParams, TransitTarget,
    UpdateSatelliteDto,
};
use crate::pagination::{PaginatedResponse, PaginationQuery};
use crate::repository::SatelliteRepository;
use crate::services::compute::error_status;
use crate::services::ephemeris::Ephemeris;
use crate::services::pipeline::{CelesTrakGroup, DiscoveryPipeline};
use crate::services::{astrodynamics, czml, maneuver};

/// Engine cost estimates, measured on one ISS TLE in a release build (docs/CZML_VIEWER_LOG.md R19):
/// 0.44 µs/sample and ~1.6 ms/day of pass search, each rounded up ~2x.
const SAMPLE_COST_MS: f64 = 0.001;
const PASS_DAY_COST_MS: u64 = 2;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Deserialize, IntoParams)]
pub struct OverheadQueryParams {
    /// Observer latitude in decimal degrees (-90.0 to 90.0)
    pub lat: f64,
    /// Observer longitude in decimal degrees (-180.0 to 180.0)
    pub lon: f64,
    /// Observer altitude above sea level in meters
    pub alt: Option<f64>,
    /// UTC timestamp for calculation epoch (defaults to current time if omitted)
    pub time: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct NextVisibleQueryParams {
    /// Observer latitude in decimal degrees (-90.0 to 90.0)
    pub lat: f64,
    /// Observer longitude in decimal degrees (-180.0 to 180.0)
    pub lon: f64,
    /// Observer altitude above sea level in meters
    pub alt: Option<f64>,
    /// Minimum elevation angle threshold in degrees (default: 5.0 deg)
    pub threshold_deg: Option<f64>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PassScheduleQueryParams {
    /// Observer latitude in decimal degrees (-90.0 to 90.0)
    pub lat: f64,
    /// Observer longitude in decimal degrees (-180.0 to 180.0)
    pub lon: f64,
    /// Observer altitude above sea level in meters (default: 0.0 m)
    pub alt: Option<f64>,
    /// Minimum elevation angle threshold in degrees (default: 5.0 deg)
    pub threshold_deg: Option<f64>,
    /// Forecast schedule window in days (default: 3 days, max: 14)
    pub duration_days: Option<usize>,
    /// Filter for only visually observable passes in twilight/darkness (default: false)
    pub visible_only: Option<bool>,
    /// UTC timestamp to start pass schedule forecast from (defaults to current time if omitted)
    pub start_time: Option<DateTime<Utc>>,
    /// Output format: 'json' (default) or 'czml' (adds a `czml` document with one entity per pass)
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct RelativeMotionQueryParams {
    /// Target / secondary satellite unique UUID identifier
    pub target_id: Uuid,
    /// UTC timestamp for calculation epoch (defaults to current time if omitted)
    pub time: Option<DateTime<Utc>>,
    /// Relative motion trajectory duration in minutes (default: 0, max: 1440)
    pub duration_minutes: Option<usize>,
    /// Step sampling interval in seconds (default: 60 secs, min: 5, max: 300)
    pub step_seconds: Option<usize>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct StateVectorQueryParams {
    /// UTC timestamp for calculation epoch (defaults to current time if omitted)
    pub epoch: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct DecayWatchQueryParams {
    /// Maximum perigee altitude threshold in km to flag re-entry hazard (default: 300.0 km)
    pub max_perigee_km: Option<f64>,
    /// Maximum number of decaying objects to return (default: 50, max: 500)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct GroundTrackQueryParams {
    /// Trajectory projection duration in minutes (default: 90 mins, max: 1440)
    pub duration_minutes: Option<usize>,
    /// Step sampling interval in seconds (default: 30 secs, max: 300)
    pub step_seconds: Option<usize>,
    /// Output format: 'geojson' (default), 'json', 'czml' or 'all' (geojson + czml)
    pub format: Option<String>,
    /// UTC timestamp to start projection from (defaults to current time if omitted)
    pub start_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PipelineSyncQueryParams {
    /// CelesTrak satellite group name to synchronize. Options: 'curated' (default: stations, visual, last-30-days), 'all', 'stations', 'visual', 'starlink', 'weather', 'last-30-days', 'active', or a comma-separated list (e.g. 'stations,visual').
    pub group: Option<String>,
    /// Maximum allowed data age in hours before triggering a CelesTrak query (default: 12.0 hours). If existing data was synced within this period, CelesTrak requests are skipped to prevent spamming.
    pub max_age_hours: Option<f64>,
    /// Force an immediate fetch from CelesTrak, bypassing freshness checks.
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipelineGroupSyncDetail {
    /// Group identifier that was synchronized
    pub group: String,
    /// Number of satellites upserted for this specific group (0 if skipped because data was already fresh)
    pub synced_count: usize,
    /// Freshness status: 'synced' (fetched from CelesTrak) or 'fresh' (skipped because records are within maxAgeHours)
    pub status: String,
    /// UTC timestamp when this group was last synchronized from CelesTrak
    pub last_synced_at: Option<DateTime<Utc>>,
    /// Current age of the data in hours at query time
    pub age_hours: Option<f64>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipelineSyncResponse {
    /// Requested group name or descriptive summary
    pub group: String,
    /// Total number of satellites newly updated across all groups
    pub synced_count: usize,
    /// Informative status summary message
    pub message: String,
    /// Detailed per-group synchronization counts and freshness status
    pub groups: Option<Vec<PipelineGroupSyncDetail>>,
    /// Available CelesTrak groups supported by the API
    pub available_groups: Option<Vec<String>>,
    /// Maximum age limit applied in hours
    pub max_age_hours: Option<f64>,
    /// Whether force mode was enabled to bypass freshness checks
    pub force: Option<bool>,
}

fn map_calculation_error(e: String) -> AppError {
    if e.contains("NotFound")
        || e.contains("Satellite not found")
        || e.contains("No satellite overhead found")
    {
        AppError::NotFound
    } else if e.contains("Sgp4Error")
        || e.contains("SGP4 calculation error")
        || e.contains("Failed to parse TLE")
    {
        AppError::Sgp4Error(e)
    } else {
        match error_status(&e) {
            StatusCode::TOO_MANY_REQUESTS => AppError::TooManyRequests(e),
            StatusCode::SERVICE_UNAVAILABLE => AppError::ServiceUnavailable(e),
            StatusCode::GATEWAY_TIMEOUT => AppError::GatewayTimeout(e),
            _ => AppError::InternalServerError(e),
        }
    }
}

/// Create Satellite
///
/// Creates a new satellite record from Two-Line Element (TLE) set data. Protected by JWT auth (requires 'editor' or 'admin' role).
#[utoipa::path(
    post,
    path = "/v1/satellites",
    operation_id = "createSatellite",
    request_body = CreateSatelliteDto,
    responses(
        (status = 201, description = "Satellite created successfully", body = Satellite),
        (status = 400, description = "Invalid request payload", body = ErrorResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 403, description = "Forbidden - Insufficient role permissions", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn create_satellite(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Json(dto): Json<CreateSatelliteDto>,
) -> Result<(StatusCode, Json<Satellite>), AppError> {
    claims.require_role(UserRole::Editor)?;
    tracing::info!("Satellite creation requested by JWT user: {}", claims.sub);
    let satellite = repo.create_satellite(dto).await?;
    Ok((StatusCode::CREATED, Json(satellite)))
}

/// List Satellites
///
/// Retrieves a paginated list of satellites using base64 checkpoint cursors. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites",
    operation_id = "listSatellites",
    params(PaginationQuery),
    responses(
        (status = 200, description = "Paginated list of satellites", body = PaginatedResponseSatellite),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn list_satellites(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<Satellite>>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let paginated_res = repo.list_satellites_paginated(pagination).await?;
    Ok(Json(paginated_res))
}

/// Get Satellite by ID
///
/// Retrieves details for a specific satellite by its unique UUID. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}",
    operation_id = "getSatellite",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier")
    ),
    responses(
        (status = 200, description = "Satellite found", body = Satellite),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_satellite(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
) -> Result<Json<Satellite>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;
    Ok(Json(satellite))
}

/// Update Satellite
///
/// Updates a satellite's name or TLE orbital parameters. Protected by JWT auth (requires 'editor' or 'admin' role).
#[utoipa::path(
    patch,
    path = "/v1/satellites/{id}",
    operation_id = "updateSatellite",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier")
    ),
    request_body = UpdateSatelliteDto,
    responses(
        (status = 200, description = "Satellite updated successfully", body = Satellite),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 403, description = "Forbidden - Insufficient role permissions", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn update_satellite(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateSatelliteDto>,
) -> Result<Json<Satellite>, AppError> {
    claims.require_role(UserRole::Editor)?;
    tracing::info!("Satellite update requested by JWT user: {}", claims.sub);
    let satellite = repo.update_satellite_by_id(id, dto).await?;
    Ok(Json(satellite))
}

/// Delete Satellite
///
/// Deletes a satellite record by its unique UUID. Protected by JWT auth (requires 'admin' role).
#[utoipa::path(
    delete,
    path = "/v1/satellites/{id}",
    operation_id = "deleteSatellite",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier")
    ),
    responses(
        (status = 204, description = "Satellite deleted successfully"),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 403, description = "Forbidden - Admin role required", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn delete_satellite(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    claims.require_role(UserRole::Admin)?;
    tracing::info!("Satellite deletion requested by JWT admin: {}", claims.sub);
    repo.delete_satellite_by_id(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Trigger CelesTrak Pipeline Sync
///
/// Triggers automated CelesTrak discovery pipeline synchronization for a specific satellite group. Protected by JWT auth (requires 'admin' role).
#[utoipa::path(
    post,
    path = "/v1/pipelines/sync",
    operation_id = "triggerPipelineSync",
    params(PipelineSyncQueryParams),
    responses(
        (status = 200, description = "Pipeline sync completed successfully", body = PipelineSyncResponse),
        (status = 400, description = "Bad Request - Invalid or unknown satellite group", body = ErrorResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 403, description = "Forbidden - Insufficient role permissions", body = ErrorResponse),
        (status = 500, description = "Pipeline sync execution failed", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Pipelines"
)]
pub async fn trigger_pipeline_sync(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Query(params): Query<PipelineSyncQueryParams>,
) -> Result<Json<PipelineSyncResponse>, AppError> {
    claims.require_role(UserRole::Admin)?;
    tracing::info!("Pipeline sync triggered by JWT user: {}", claims.sub);

    let raw_input = params.group.as_deref().unwrap_or("curated");
    let requested_groups = CelesTrakGroup::parse_groups(raw_input).map_err(AppError::BadRequest)?;

    let force = params.force.unwrap_or(false);
    let max_age_hours: f64 = params.max_age_hours.unwrap_or_else(|| {
        std::env::var("PIPELINE_MAX_AGE_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12.0)
    });

    let mut group_details = Vec::with_capacity(requested_groups.len());
    let mut total_synced = 0usize;
    let mut freshly_synced_count = 0usize;
    let mut skipped_fresh_count = 0usize;

    for group in requested_groups {
        let res = DiscoveryPipeline::sync_group_with_freshness(&repo, group, max_age_hours, force)
            .await
            .map_err(AppError::InternalServerError)?;

        total_synced += res.synced_count;
        if res.status == "synced" {
            freshly_synced_count += 1;
        } else {
            skipped_fresh_count += 1;
        }

        group_details.push(PipelineGroupSyncDetail {
            group: res.group,
            synced_count: res.synced_count,
            status: res.status,
            last_synced_at: res.last_synced_at,
            age_hours: res.age_hours,
        });
    }

    let group_label = if raw_input.trim().is_empty() {
        "curated".to_string()
    } else {
        raw_input.trim().to_string()
    };

    let available_groups = CelesTrakGroup::all_names()
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    let message = if skipped_fresh_count > 0 && freshly_synced_count == 0 {
        format!(
            "All requested groups ({}) are up-to-date (within {:.1}h max age limit). CelesTrak requests were skipped to prevent spam. Pass ?force=true to override.",
            group_label, max_age_hours
        )
    } else if skipped_fresh_count > 0 {
        format!(
            "Synced {} satellites across {} groups; {} groups were already fresh within {:.1}h limit.",
            total_synced, freshly_synced_count, skipped_fresh_count, max_age_hours
        )
    } else if group_details.len() == 1 {
        format!(
            "Successfully synced {} satellites for group '{}' (max age: {:.1}h)",
            total_synced, group_details[0].group, max_age_hours
        )
    } else {
        let names: Vec<&str> = group_details.iter().map(|g| g.group.as_str()).collect();
        format!(
            "Successfully synced {} satellites across {} groups ({})",
            total_synced,
            group_details.len(),
            names.join(", ")
        )
    };

    Ok(Json(PipelineSyncResponse {
        group: group_label,
        synced_count: total_synced,
        message,
        groups: Some(group_details),
        available_groups: Some(available_groups),
        max_age_hours: Some(max_age_hours),
        force: Some(force),
    }))
}

/// Get Overhead Satellite
///
/// Computes the satellite closest to overhead (highest elevation) across all tracked satellites using parallel Rayon propagation. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/overhead",
    operation_id = "getOverheadSatellite",
    params(OverheadQueryParams),
    responses(
        (status = 200, description = "Overhead satellite computed successfully", body = OverheadResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "No satellite overhead found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_overhead(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Query(params): Query<OverheadQueryParams>,
) -> Result<Json<OverheadResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellites = repo.list_satellites().await?;
    if satellites.is_empty() {
        return Err(AppError::NotFound);
    }

    let time = params.time.unwrap_or_else(Utc::now);
    let alt = params.alt.unwrap_or(0.0);
    let cache_key = format!(
        "overhead:{}:{}:{}:{}",
        params.lat,
        params.lon,
        alt,
        time.timestamp_millis()
    );

    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let overhead_res = tokio::task::spawn_blocking(move || {
                astrodynamics::find_overhead_satellite(
                    &satellites,
                    params.lat,
                    params.lon,
                    alt,
                    time,
                )
            })
            .await
            .map_err(|e| e.to_string())?;

            overhead_res.ok_or_else(|| "No satellite overhead found".to_string())
        })
        .await
        .map_err(map_calculation_error)?;

    Ok(Json(res))
}

/// Get Next Visible Pass
///
/// Calculates the next visible pass for a specific satellite above elevation threshold. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}/next-visible",
    operation_id = "getNextVisiblePass",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier"),
        NextVisibleQueryParams
    ),
    responses(
        (status = 200, description = "Next visible pass calculated successfully", body = NextVisiblePassResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_next_visible(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
    Query(params): Query<NextVisibleQueryParams>,
) -> Result<Json<NextVisiblePassResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;
    let start_time = Utc::now();
    let alt = params.alt.unwrap_or(0.0);
    let threshold = params.threshold_deg.unwrap_or(5.0);
    let time_bucket = start_time.timestamp() / 60; // Round start time to 1-minute bucket for cache reuse

    let cache_key = format!(
        "next_visible:{}:{}:{}:{}:{}:{}",
        id, params.lat, params.lon, alt, threshold, time_bucket
    );

    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let pass_res = tokio::task::spawn_blocking(move || {
                astrodynamics::find_next_visible_pass(
                    &satellite, params.lat, params.lon, alt, start_time, threshold, 1440,
                )
            })
            .await
            .map_err(|e| e.to_string())?;

            pass_res.map_err(|e| e.to_string())
        })
        .await
        .map_err(map_calculation_error)?;

    Ok(Json(res))
}

/// Get Satellite 3D Ground Track & Trajectory
///
/// Computes 3D ECF coordinates, geodetic position, orbital period, footprint radius, and GeoJSON ground track line. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}/groundtrack",
    operation_id = "getGroundTrack",
    params(
        ("id" = Uuid, Path, description = "Satellite UUID"),
        GroundTrackQueryParams,
    ),
    responses(
        (status = 200, description = "3D Ground Track and GeoJSON trajectory", body = GroundTrackResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse),
        (status = 429, description = "Per-user concurrent compute quota exceeded (COMPUTE_JOB_QUOTA_BY_ROLE, default 5; admin 50); retry when a job finishes", body = ErrorResponse),
        (status = 503, description = "No compute slot freed within the queue wait; retry shortly", body = ErrorResponse),
        (status = 504, description = "Computation exceeded the 30 s limit", body = ErrorResponse),
        (status = 500, description = "Internal calculation error", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_ground_track(
    claims: Claims,
    Path(id): Path<Uuid>,
    Query(params): Query<GroundTrackQueryParams>,
    State(repo): State<SatelliteRepository>,
) -> Result<Json<GroundTrackResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;
    let start_time = params.start_time.unwrap_or_else(Utc::now);
    let duration_minutes = params.duration_minutes.unwrap_or(90);
    let step_seconds = params.step_seconds.unwrap_or(30);
    let (include_geojson, include_czml) = match params.format.as_deref() {
        Some(f) if f.eq_ignore_ascii_case("czml") => (false, true),
        Some(f) if f.eq_ignore_ascii_case("json") => (false, false),
        Some(f) if f.eq_ignore_ascii_case("all") => (true, true),
        _ => (true, false),
    };

    // Reject before the engine: errors raised inside it surface as 500s.
    let samples = Ephemeris::check_range(duration_minutes * 60, step_seconds)?;
    if include_czml {
        czml::check_step(step_seconds)?;
    }

    let cache_key = format!(
        "groundtrack:{}:{}:{}:{}:{}:{}:{}",
        id,
        duration_minutes,
        step_seconds,
        include_geojson,
        include_czml,
        start_time.timestamp_millis(),
        satellite.last_modified_date.timestamp()
    );

    let engine = repo.compute.clone();
    let (user_id, role) = (claims.sub.clone(), claims.canonical_role());
    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let track_res = engine
                .run(
                    ((samples as f64 * SAMPLE_COST_MS).ceil() as u64).max(1),
                    user_id,
                    role,
                    move |cancel| {
                        astrodynamics::generate_ground_track(
                            &satellite,
                            start_time,
                            duration_minutes,
                            step_seconds,
                            include_geojson,
                            include_czml,
                            &cancel,
                        )
                    },
                )
                .await?;

            track_res.map_err(|e| e.to_string())
        })
        .await
        .map_err(map_calculation_error)?;

    Ok(Json(res))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct IlluminationQueryParams {
    /// Observer latitude in decimal degrees (-90.0 to 90.0)
    pub lat: f64,
    /// Observer longitude in decimal degrees (-180.0 to 180.0)
    pub lon: f64,
    /// Observer altitude above sea level in meters (default: 0.0 m)
    pub alt: Option<f64>,
    /// UTC timestamp for calculation epoch (defaults to current time if omitted)
    pub time: Option<DateTime<Utc>>,
}

/// Get Satellite Illumination & Visual Magnitude
///
/// Computes solar shadow geometry (FullSunlight, Penumbra, Umbra), observer twilight state, observable status, and visual magnitude. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}/illumination",
    operation_id = "getSatelliteIllumination",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier"),
        IlluminationQueryParams
    ),
    responses(
        (status = 200, description = "Satellite illumination status computed successfully", body = IlluminationResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_satellite_illumination(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
    Query(params): Query<IlluminationQueryParams>,
) -> Result<Json<IlluminationResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;
    let time = params.time.unwrap_or_else(Utc::now);
    let alt = params.alt.unwrap_or(0.0);

    let cache_key = format!(
        "illumination:{}:{}:{}:{}:{}",
        id,
        params.lat,
        params.lon,
        alt,
        time.timestamp_millis()
    );

    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let sat_clone = satellite.clone();
            let illum_res = tokio::task::spawn_blocking(move || {
                astrodynamics::calculate_illumination(&sat_clone, params.lat, params.lon, alt, time)
            })
            .await
            .map_err(|e| e.to_string())?;

            illum_res.map_err(|e| e.to_string())
        })
        .await
        .map_err(map_calculation_error)?;

    Ok(Json(res))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ConjunctionSearchQueryParams {
    /// Maximum threshold distance in km between satellites to trigger a conjunction match (default: 10.0 km)
    pub max_distance_km: Option<f64>,
    /// Forecast window forward in hours (default: 24 hrs, max: 72)
    pub duration_hours: Option<i64>,
}

/// Search Conjunctions & Satellite Collision Radar
///
/// Evaluates close-approach orbital conjunctions across all active satellites using Rayon parallel execution. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/conjunctions/search",
    operation_id = "searchConjunctions",
    params(ConjunctionSearchQueryParams),
    responses(
        (status = 200, description = "Conjunction radar search completed successfully", body = ConjunctionSearchResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 500, description = "Internal calculation error", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Astrodynamics"
)]
pub async fn search_conjunctions(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Query(params): Query<ConjunctionSearchQueryParams>,
) -> Result<Json<ConjunctionSearchResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellites = repo.list_satellites().await?;
    let start_time = Utc::now();
    let max_distance_km = params.max_distance_km.unwrap_or(10.0);
    let duration_hours = params.duration_hours.unwrap_or(24);

    let time_bucket = start_time.timestamp() / 60;
    let cache_key = format!(
        "conjunctions:{}:{}:{}:{}",
        satellites.len(),
        max_distance_km,
        duration_hours,
        time_bucket
    );

    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let conjunction_res = tokio::task::spawn_blocking(move || {
                astrodynamics::find_conjunctions(
                    &satellites,
                    start_time,
                    max_distance_km,
                    duration_hours,
                )
            })
            .await
            .map_err(|e| e.to_string())?;

            Ok(conjunction_res)
        })
        .await
        .map_err(AppError::InternalServerError)?;

    Ok(Json(res))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct DopplerQueryParams {
    /// Nominal satellite transmitter frequency in Hertz (e.g. 437500000 for 437.5 MHz UHF)
    pub center_freq_hz: f64,
    /// Observer latitude in decimal degrees (-90.0 to 90.0)
    pub lat: f64,
    /// Observer longitude in decimal degrees (-180.0 to 180.0)
    pub lon: f64,
    /// Observer altitude above sea level in km (default: 0.0)
    pub alt_km: Option<f64>,
    /// UTC timestamp for calculation epoch (defaults to current time if omitted)
    pub time: Option<DateTime<Utc>>,
    /// Use the relativistic Doppler equation with gravitational shift (default: false, classical first-order)
    pub relativistic: Option<bool>,
}

/// Get Satellite RF Doppler Shift
///
/// Computes range rate (km/s), Doppler frequency shift (Hz), and corrected transmitter frequency for ground station tracking. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}/doppler",
    operation_id = "getSatelliteDoppler",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier"),
        DopplerQueryParams
    ),
    responses(
        (status = 200, description = "Doppler shift computed successfully", body = DopplerResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_satellite_doppler(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
    Query(params): Query<DopplerQueryParams>,
) -> Result<Json<DopplerResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;
    let time = params.time.unwrap_or_else(Utc::now);
    let alt_km = params.alt_km.unwrap_or(0.0);

    let relativistic = params.relativistic.unwrap_or(false);

    // Exact inputs: Doppler varies by tens of Hz per second, so no rounding or time bucketing
    let cache_key = format!(
        "doppler:{}:{}:{}:{}:{}:{}:{}",
        id,
        params.center_freq_hz,
        params.lat,
        params.lon,
        alt_km,
        time.timestamp_millis(),
        relativistic
    );

    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let sat_clone = satellite.clone();
            let doppler_res = tokio::task::spawn_blocking(move || {
                astrodynamics::calculate_doppler_shift(
                    &sat_clone,
                    params.center_freq_hz,
                    params.lat,
                    params.lon,
                    alt_km,
                    time,
                    relativistic,
                )
            })
            .await
            .map_err(|e| e.to_string())?;

            doppler_res.map_err(|e| e.to_string())
        })
        .await
        .map_err(map_calculation_error)?;

    Ok(Json(res))
}

/// Reconstruct Satellite Orbital Maneuvers
///
/// Detects trajectory step discontinuities and calculates delta-V vector components, fuel consumption, and maneuver classification. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}/maneuvers",
    operation_id = "getSatelliteManeuvers",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier"),
        ManeuverQueryParams
    ),
    responses(
        (status = 200, description = "Orbital maneuvers reconstructed successfully", body = ManeuversResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_satellite_maneuvers(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
    Query(params): Query<ManeuverQueryParams>,
) -> Result<Json<ManeuversResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;
    let min_sma = params.min_sma_change_km.unwrap_or(0.1);
    let min_inc = params.min_inclination_change_deg.unwrap_or(0.005);
    let history = [satellite.tle.clone()];

    let res = maneuver::reconstruct_maneuvers(
        &satellite,
        &history,
        params.start_time,
        params.end_time,
        min_sma,
        min_inc,
    )?;
    Ok(Json(res))
}

/// Detect Satellite Trajectory Anomalies
///
/// Performs statistical residual analysis across TLE epoch parameters to identify non-natural trajectory anomalies. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    post,
    path = "/v1/satellites/{id}/detect-anomalies",
    operation_id = "detectSatelliteAnomalies",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier")
    ),
    request_body = Option<AnomalyDetectionRequest>,
    responses(
        (status = 200, description = "Anomaly detection report generated successfully", body = AnomalyDetectionResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn detect_satellite_anomalies(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
    body: Option<Json<AnomalyDetectionRequest>>,
) -> Result<Json<AnomalyDetectionResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;
    let (sigma, sma, inc) = match body {
        Some(Json(req)) => (
            req.threshold_sigma.unwrap_or(3.0),
            req.min_sma_change_km.unwrap_or(0.1),
            req.min_inclination_change_deg.unwrap_or(0.005),
        ),
        None => (3.0, 0.1, 0.005),
    };
    let history = [satellite.tle.clone()];

    let res = maneuver::detect_anomalies(&satellite, &history, sigma, sma, inc)?;
    Ok(Json(res))
}

/// Predict Solar Satellite Transits
///
/// Predicts satellite silhouettes crossing in front of the Solar disk for a ground station observer. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/transits/solar",
    operation_id = "getSolarTransits",
    params(TransitQueryParams),
    responses(
        (status = 200, description = "Solar transit predictions generated successfully", body = TransitPredictionResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 500, description = "Internal calculation error", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Astrodynamics"
)]
pub async fn get_solar_transits(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Query(params): Query<TransitQueryParams>,
) -> Result<Json<TransitPredictionResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellites = repo.list_satellites().await?;
    let start_time = Utc::now();
    let alt_km = params.alt.unwrap_or(0.0) / 1000.0;
    let days = params.duration_days.unwrap_or(7).min(30);
    let max_sep = params.max_angular_separation_deg.unwrap_or(0.5);

    let res = tokio::task::spawn_blocking(move || {
        astrodynamics::find_transits(
            TransitTarget::Sun,
            &satellites,
            params.lat,
            params.lon,
            alt_km,
            start_time,
            days,
            max_sep,
        )
    })
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))??;

    Ok(Json(res))
}

/// Predict Lunar Satellite Transits
///
/// Predicts satellite silhouettes crossing in front of the Lunar disk for a ground station observer. Uses a low-accuracy lunar series (position error up to ~2 degrees), so results are screening-level. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/transits/lunar",
    operation_id = "getLunarTransits",
    params(TransitQueryParams),
    responses(
        (status = 200, description = "Lunar transit predictions generated successfully", body = TransitPredictionResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 500, description = "Internal calculation error", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Astrodynamics"
)]
pub async fn get_lunar_transits(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Query(params): Query<TransitQueryParams>,
) -> Result<Json<TransitPredictionResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellites = repo.list_satellites().await?;
    let start_time = Utc::now();
    let alt_km = params.alt.unwrap_or(0.0) / 1000.0;
    let days = params.duration_days.unwrap_or(7).min(30);
    let max_sep = params.max_angular_separation_deg.unwrap_or(0.5);

    let res = tokio::task::spawn_blocking(move || {
        astrodynamics::find_transits(
            TransitTarget::Moon,
            &satellites,
            params.lat,
            params.lon,
            alt_km,
            start_time,
            days,
            max_sep,
        )
    })
    .await
    .map_err(|e| AppError::InternalServerError(e.to_string()))??;

    Ok(Json(res))
}

/// Schedule Ground Station Satellite Passes
///
/// Computes multi-day pass prediction schedule (AOS, TCA, LOS, elevation, azimuth, duration, optical visibility, visual magnitude). Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}/passes",
    operation_id = "getSatellitePasses",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier"),
        PassScheduleQueryParams
    ),
    responses(
        (status = 200, description = "Satellite pass schedule computed successfully", body = PassScheduleResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse),
        (status = 429, description = "Per-user concurrent compute quota exceeded (COMPUTE_JOB_QUOTA_BY_ROLE, default 5; admin 50); retry when a job finishes", body = ErrorResponse),
        (status = 503, description = "No compute slot freed within the queue wait; retry shortly", body = ErrorResponse),
        (status = 504, description = "Computation exceeded the 30 s limit", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_satellite_passes(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
    Query(params): Query<PassScheduleQueryParams>,
) -> Result<Json<PassScheduleResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;
    let start_time = params.start_time.unwrap_or_else(Utc::now);
    let alt = params.alt.unwrap_or(0.0);
    let threshold = params.threshold_deg.unwrap_or(5.0);
    let days = params.duration_days.unwrap_or(3).clamp(1, 14);
    let visible_only = params.visible_only.unwrap_or(false);
    let include_czml = params
        .format
        .as_deref()
        .is_some_and(|f| f.eq_ignore_ascii_case("czml"));

    let cache_key = format!(
        "passes:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        id,
        params.lat,
        params.lon,
        alt,
        threshold,
        days,
        visible_only,
        include_czml,
        start_time.timestamp_millis()
    );

    let engine = repo.compute.clone();
    let (user_id, role) = (claims.sub.clone(), claims.canonical_role());
    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let pass_res = engine
                .run(
                    PASS_DAY_COST_MS * days as u64,
                    user_id,
                    role,
                    move |cancel| {
                        let mut res = astrodynamics::find_pass_schedule(
                            &satellite,
                            params.lat,
                            params.lon,
                            alt,
                            start_time,
                            threshold,
                            days,
                            visible_only,
                            &cancel,
                        )?;
                        if include_czml {
                            let (doc, dropped) =
                                czml::passes_document(&satellite, &res.passes, &cancel)?;
                            res.czml = Some(doc);
                            res.czml_dropped_samples = Some(dropped);
                        }
                        Ok::<_, AppError>(res)
                    },
                )
                .await?;

            pass_res.map_err(|e| e.to_string())
        })
        .await
        .map_err(map_calculation_error)?;

    Ok(Json(res))
}

/// Compute Satellite Relative Motion & RPO
///
/// Computes relative position, velocity vectors, range rate, and RPO regime in the Local-Vertical Local-Horizontal (LVLH / Hill's) frame between two satellites. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}/relative-motion",
    operation_id = "getRelativeMotion",
    params(
        ("id" = Uuid, Path, description = "Primary satellite unique UUID identifier"),
        RelativeMotionQueryParams
    ),
    responses(
        (status = 200, description = "Relative motion and RPO computed successfully", body = RelativeMotionResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_relative_motion(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
    Query(params): Query<RelativeMotionQueryParams>,
) -> Result<Json<RelativeMotionResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let primary = repo.get_satellite_by_id(id).await?;
    let target = repo.get_satellite_by_id(params.target_id).await?;
    let epoch = params.time.unwrap_or_else(Utc::now);
    let dur_mins = params.duration_minutes.unwrap_or(0);
    let step_secs = params.step_seconds.unwrap_or(60);

    let cache_key = format!(
        "relmotion:{}:{}:{}:{}:{}",
        id,
        params.target_id,
        dur_mins,
        step_secs,
        epoch.timestamp_millis()
    );

    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let p_clone = primary.clone();
            let t_clone = target.clone();
            let motion_res = tokio::task::spawn_blocking(move || {
                astrodynamics::calculate_relative_motion(
                    &p_clone, &t_clone, epoch, dur_mins, step_secs,
                )
            })
            .await
            .map_err(|e| e.to_string())?;

            motion_res.map_err(|e| e.to_string())
        })
        .await
        .map_err(map_calculation_error)?;

    Ok(Json(res))
}

/// Get Instantaneous State Vector & Keplerian Elements
///
/// Computes ECI (TEME), ECEF, Geodetic state vectors and osculating Keplerian orbital elements at an exact epoch. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}/state",
    operation_id = "getSatelliteState",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier"),
        StateVectorQueryParams
    ),
    responses(
        (status = 200, description = "Satellite state vector computed successfully", body = SatelliteStateResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_satellite_state(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
    Query(params): Query<StateVectorQueryParams>,
) -> Result<Json<SatelliteStateResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;
    let epoch = params.epoch.unwrap_or_else(Utc::now);

    let cache_key = format!("state:{}:{}", id, epoch.timestamp_millis());

    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let sat_clone = satellite.clone();
            let state_res = tokio::task::spawn_blocking(move || {
                astrodynamics::calculate_satellite_state(&sat_clone, epoch)
            })
            .await
            .map_err(|e| e.to_string())?;

            state_res.map_err(|e| e.to_string())
        })
        .await
        .map_err(map_calculation_error)?;

    Ok(Json(res))
}

/// Get Satellite Atmospheric Drag & Decay Risk
///
/// Evaluates perigee/apogee altitude above the WGS-84 ellipsoid, the re-entry regime and a risk score from perigee altitude alone. No lifetime is estimated because a TLE carries no ballistic coefficient. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/{id}/decay-risk",
    operation_id = "getSatelliteDecayRisk",
    params(
        ("id" = Uuid, Path, description = "Satellite unique UUID identifier")
    ),
    responses(
        (status = 200, description = "Satellite orbital decay risk assessed successfully", body = SatelliteDecayRiskResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 404, description = "Satellite not found", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Satellites"
)]
pub async fn get_satellite_decay_risk(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Path(id): Path<Uuid>,
) -> Result<Json<SatelliteDecayRiskResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellite = repo.get_satellite_by_id(id).await?;

    let cache_key = format!(
        "decay_risk:{}:{}",
        id,
        satellite.last_modified_date.timestamp()
    );

    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let sat_clone = satellite.clone();
            let decay_res = tokio::task::spawn_blocking(move || {
                astrodynamics::calculate_decay_risk(&sat_clone)
            })
            .await
            .map_err(|e| e.to_string())?;

            decay_res.map_err(|e| e.to_string())
        })
        .await
        .map_err(map_calculation_error)?;

    Ok(Json(res))
}

/// Atmospheric Drag & Orbital Decay Re-Entry Watch
///
/// Scans the satellite catalog for debris and satellites experiencing severe atmospheric drag or nearing uncontrolled atmospheric re-entry. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    get,
    path = "/v1/satellites/decay-watch",
    operation_id = "getDecayWatch",
    params(DecayWatchQueryParams),
    responses(
        (status = 200, description = "Decay watch scan completed successfully", body = DecayWatchResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse),
        (status = 500, description = "Internal calculation error", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Astrodynamics"
)]
pub async fn get_decay_watch(
    claims: Claims,
    State(repo): State<SatelliteRepository>,
    Query(params): Query<DecayWatchQueryParams>,
) -> Result<Json<DecayWatchResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let satellites = repo.list_satellites().await?;
    let max_perigee = params.max_perigee_km.unwrap_or(300.0);
    let limit = params.limit.unwrap_or(50).clamp(1, 500);

    let time_bucket = Utc::now().timestamp() / 60;
    let cache_key = format!(
        "decay_watch:{}:{}:{}:{}",
        satellites.len(),
        max_perigee,
        limit,
        time_bucket
    );

    let res = repo
        .cache
        .get_or_insert_with(&cache_key, || async move {
            let watch_res = tokio::task::spawn_blocking(move || {
                astrodynamics::scan_decay_watch(&satellites, max_perigee, limit)
            })
            .await
            .map_err(|e| e.to_string())?;

            Ok(watch_res)
        })
        .await
        .map_err(AppError::InternalServerError)?;

    Ok(Json(res))
}

/// Calculate Conjunction Collision Probability (Pc)
///
/// Computes 2D encounter-plane collision probability (Pc) using Foster's algorithm given miss distance, relative velocity, hard-body radius, and combined position covariance uncertainty. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    post,
    path = "/v1/conjunctions/collision-probability",
    operation_id = "calculateCollisionProbability",
    request_body = CollisionProbabilityRequest,
    responses(
        (status = 200, description = "Collision probability computed successfully", body = CollisionProbabilityResponse),
        (status = 400, description = "Invalid request payload", body = ErrorResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Astrodynamics"
)]
pub async fn calculate_collision_probability(
    claims: Claims,
    Json(req): Json<CollisionProbabilityRequest>,
) -> Result<Json<CollisionProbabilityResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let hbr = req.hard_body_radius_m.unwrap_or(10.0);
    if !hbr.is_finite() || hbr <= 0.0 {
        return Err(AppError::BadRequest(
            "hardBodyRadiusM must be a positive finite number".to_string(),
        ));
    }
    if !(req.sigma_1_m.is_finite() && req.sigma_2_m.is_finite())
        || req.sigma_1_m <= 0.0
        || req.sigma_2_m <= 0.0
    {
        return Err(AppError::BadRequest(
            "sigma1M and sigma2M must be positive finite numbers".to_string(),
        ));
    }

    let res = astrodynamics::calculate_foster_collision_probability(
        req.miss_distance_km,
        req.relative_velocity_kms,
        hbr,
        req.sigma_1_m,
        req.sigma_2_m,
        req.miss_angle_deg.unwrap_or(0.0),
    );

    Ok(Json(res))
}

/// Transform Orbital Elements (Cartesian, Keplerian, Modified Equinoctial)
///
/// Converts between Cartesian state vectors, Classical Keplerian elements, and singularity-free Modified Equinoctial elements (p, f, g, h, k, L). Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    post,
    path = "/v1/satellites/transforms/elements",
    operation_id = "transformOrbitalElements",
    request_body = ElementTransformRequest,
    responses(
        (status = 200, description = "Orbital elements transformed successfully across all representations", body = ElementTransformResponse),
        (status = 400, description = "Invalid request payload or singular/hyperbolic orbit", body = ErrorResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Astrodynamics"
)]
pub async fn transform_elements(
    claims: Claims,
    Json(req): Json<ElementTransformRequest>,
) -> Result<Json<ElementTransformResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let res = astrodynamics::transform_orbital_elements(&req)?;
    Ok(Json(res))
}

/// Transform Coordinate Frames (ECI, ECEF, Topocentric SEZ / NED)
///
/// Converts position and velocity state vectors across ECI (TEME), ECEF (WGS-84), and Topocentric Horizon frames (SEZ, NED) with Greenwich Mean Sidereal Time rotation, kinematic velocity transport, Bowring geodetics, and look angle slant range/range rates. Protected by JWT auth (requires 'viewer', 'editor', or 'admin' role).
#[utoipa::path(
    post,
    path = "/v1/satellites/transforms/frames",
    operation_id = "transformCoordinateFrames",
    request_body = FrameTransformRequest,
    responses(
        (status = 200, description = "State vectors transformed successfully across coordinate frames", body = FrameTransformResponse),
        (status = 400, description = "Invalid request payload or missing topocentric observer coordinates", body = ErrorResponse),
        (status = 401, description = "Unauthorized - Missing or invalid JWT token", body = ErrorResponse)
    ),
    security(("bearer_auth" = [])),
    tag = "Astrodynamics"
)]
pub async fn transform_frames(
    claims: Claims,
    Json(req): Json<FrameTransformRequest>,
) -> Result<Json<FrameTransformResponse>, AppError> {
    claims.require_role(UserRole::Viewer)?;
    let res = astrodynamics::transform_coordinate_frame(&req)?;
    Ok(Json(res))
}
