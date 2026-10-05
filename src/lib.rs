pub mod auth;
pub mod cache;
pub mod config;
pub mod error;
pub mod handlers;
pub mod models;
pub mod pagination;
pub mod repository;
pub mod services;

use axum::{
    response::{Html, Redirect},
    routing::{get, post},
    Router,
};
use repository::SatelliteRepository;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use utoipa::{
    openapi::security::{Http, HttpAuthScheme, SecurityScheme},
    Modify, OpenApi,
};
use utoipa_swagger_ui::SwaggerUi;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        auth::signup_handler,
        auth::login_handler,
        auth::token_exchange_handler,
        auth::me_handler,
        handlers::satellite_handler::create_satellite,
        handlers::satellite_handler::list_satellites,
        handlers::satellite_handler::get_satellite,
        handlers::satellite_handler::update_satellite,
        handlers::satellite_handler::delete_satellite,
        handlers::satellite_handler::trigger_pipeline_sync,
        handlers::satellite_handler::get_overhead,
        handlers::satellite_handler::get_next_visible,
        handlers::satellite_handler::get_satellite_passes,
        handlers::satellite_handler::get_relative_motion,
        handlers::satellite_handler::get_satellite_state,
        handlers::satellite_handler::get_satellite_decay_risk,
        handlers::satellite_handler::get_decay_watch,
        handlers::satellite_handler::get_ground_track,
        handlers::satellite_handler::get_satellite_illumination,
        handlers::satellite_handler::get_satellite_doppler,
        handlers::satellite_handler::get_satellite_maneuvers,
        handlers::satellite_handler::detect_satellite_anomalies,
        handlers::satellite_handler::get_solar_transits,
        handlers::satellite_handler::get_lunar_transits,
        handlers::satellite_handler::search_conjunctions,
        handlers::satellite_handler::calculate_collision_probability,
    ),
    components(
        schemas(
            auth::UserRole,
            auth::Claims,
            auth::CnfClaim,
            auth::SignupRequest,
            auth::LoginRequest,
            auth::ClientAssertionRequest,
            auth::AuthResponse,
            models::Satellite,
            models::Tle,
            models::CreateSatelliteDto,
            models::UpdateSatelliteDto,
            models::OverheadResponse,
            models::NextVisiblePassResponse,
            models::SatellitePass,
            models::PassScheduleResponse,
            models::RelativeMotionPoint,
            models::RelativeMotionResponse,
            models::KeplerianElements,
            models::SatelliteStateResponse,
            models::SatelliteDecayRiskResponse,
            models::DecayWatchResponse,
            models::CollisionProbabilityRequest,
            models::CollisionProbabilityResponse,
            models::GroundTrackResponse,
            models::GroundTrackPoint,
            models::GeoJsonFeature,
            models::GeoJsonGeometry,
            models::IlluminationResponse,
            models::LightingState,
            models::ObserverTwilightState,
            models::DopplerResponse,
            models::ManeuversResponse,
            models::DetectedManeuver,
            models::ManeuverType,
            models::AnomalyDetectionRequest,
            models::AnomalyDetectionResponse,
            models::AnomalyStatus,
            models::OrbitalParameterResidual,
            models::TransitPredictionResponse,
            models::TransitMatch,
            models::TransitTarget,
            models::ConjunctionSearchResponse,
            models::ConjunctionMatch,
            models::SatelliteSummary,
            handlers::satellite_handler::PipelineSyncResponse,
            handlers::satellite_handler::PipelineGroupSyncDetail,
            pagination::PaginationMeta,
            pagination::PaginatedResponseSatellite,
            error::ErrorResponse,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "Authentication", description = "JWT Token issuance endpoints"),
        (name = "Satellites", description = "Satellite catalog, telemetry, and individual satellite astrodynamics"),
        (name = "Astrodynamics", description = "Multi-satellite and celestial space domain awareness (SDA) calculations"),
        (name = "Pipelines", description = "CelesTrak automated discovery pipelines")
    )
)]
pub struct ApiDoc;

pub fn default_auth_provider() -> std::sync::Arc<dyn auth::provider::AuthProvider> {
    if let (Ok(url), Ok(key)) = (
        std::env::var("SUPABASE_URL"),
        std::env::var("SUPABASE_ANON_KEY"),
    ) {
        if !url.trim().is_empty() && !key.trim().is_empty() {
            return std::sync::Arc::new(auth::provider::SupabaseAuthProvider::new(url, key));
        }
    }
    let db_url = std::env::var("POSTGRES_URI")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .ok();
    if let Some(url) = db_url {
        if !url.trim().is_empty() {
            if let Ok(pool) = sqlx::PgPool::connect_lazy(&url) {
                return std::sync::Arc::new(auth::provider::PostgresAuthProvider::new(pool));
            }
        }
    }
    std::sync::Arc::new(auth::provider::MemoryAuthProvider::new())
}

pub fn create_router_with_auth_and_limiter(
    repo: SatelliteRepository,
    auth_provider: std::sync::Arc<dyn auth::provider::AuthProvider>,
    limiter: std::sync::Arc<dyn services::ratelimit::RateLimiter>,
) -> Router {
    let auth_routes = Router::new()
        .route("/signup", post(auth::signup_handler))
        .route("/login", post(auth::login_handler))
        .route("/token", post(auth::token_exchange_handler))
        .route("/me", get(auth::me_handler))
        .with_state(auth_provider);

    let api_routes = Router::new()
        .nest("/auth", auth_routes)
        .route(
            "/satellites",
            post(handlers::create_satellite).get(handlers::list_satellites),
        )
        .route(
            "/satellites/:id",
            get(handlers::get_satellite)
                .patch(handlers::update_satellite)
                .delete(handlers::delete_satellite),
        )
        .route("/satellites/overhead", get(handlers::get_overhead))
        .route("/satellites/decay-watch", get(handlers::get_decay_watch))
        .route("/conjunctions/search", get(handlers::search_conjunctions))
        .route(
            "/conjunctions/collision-probability",
            post(handlers::calculate_collision_probability),
        )
        .route(
            "/satellites/:id/next-visible",
            get(handlers::get_next_visible),
        )
        .route(
            "/satellites/:id/passes",
            get(handlers::get_satellite_passes),
        )
        .route(
            "/satellites/:id/relative-motion",
            get(handlers::get_relative_motion),
        )
        .route("/satellites/:id/state", get(handlers::get_satellite_state))
        .route(
            "/satellites/:id/decay-risk",
            get(handlers::get_satellite_decay_risk),
        )
        .route(
            "/satellites/:id/groundtrack",
            get(handlers::get_ground_track),
        )
        .route(
            "/satellites/:id/illumination",
            get(handlers::get_satellite_illumination),
        )
        .route(
            "/satellites/:id/doppler",
            get(handlers::get_satellite_doppler),
        )
        .route(
            "/satellites/:id/maneuvers",
            get(handlers::get_satellite_maneuvers),
        )
        .route(
            "/satellites/:id/detect-anomalies",
            post(handlers::detect_satellite_anomalies),
        )
        .route("/transits/solar", get(handlers::get_solar_transits))
        .route("/transits/lunar", get(handlers::get_lunar_transits))
        .route("/pipelines/sync", post(handlers::trigger_pipeline_sync))
        .with_state(repo)
        .layer(axum::middleware::from_fn_with_state(
            limiter,
            services::ratelimit::middleware::rate_limit_layer,
        ));

    const REDOC_HTML: &str = r#"<!DOCTYPE html>
<html>
  <head>
    <title>Astrea SDA API - Interactive OpenAPI Documentation</title>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&display=swap" rel="stylesheet">
    <style>body { margin: 0; padding: 0; font-family: 'Inter', sans-serif; }</style>
  </head>
  <body>
    <redoc spec-url='/api-docs/openapi.json' expand-responses="200,201"></redoc>
    <script src="https://cdn.jsdelivr.net/npm/redoc@latest/bundles/redoc.standalone.js"></script>
  </body>
</html>"#;

    Router::new()
        .route("/", get(|| async { Redirect::temporary("/swagger-ui/") }))
        .route("/docs", get(|| async { Html(REDOC_HTML) }))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .nest("/v1", api_routes.clone())
        .nest("/api/v1", api_routes)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

pub fn create_router_with_auth(
    repo: SatelliteRepository,
    auth_provider: std::sync::Arc<dyn auth::provider::AuthProvider>,
) -> Router {
    create_router_with_auth_and_limiter(
        repo,
        auth_provider,
        std::sync::Arc::new(services::ratelimit::NoOpRateLimiter),
    )
}

pub fn create_router(repo: SatelliteRepository) -> Router {
    create_router_with_auth(repo, default_auth_provider())
}
