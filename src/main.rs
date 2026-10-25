use astrea_sda_api::{
    config::Config, repository::SatelliteRepository, services::pipeline::DiscoveryPipeline,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "astrea_sda_api=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env();
    let redis_url = std::env::var("REDIS_URL").ok();
    let repo = SatelliteRepository::new(redis_url.as_deref()).await;

    // Start background CelesTrak discovery pipeline (syncs every 6 hours)
    let enable_pipeline = std::env::var("ENABLE_DISCOVERY_PIPELINE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);

    if enable_pipeline {
        tracing::info!(
            "🚀 Starting automated CelesTrak discovery pipeline background worker (6-hour refresh)"
        );
        DiscoveryPipeline::start_background_sync(repo.clone(), 6);
    }

    let rate_limit_config = astrea_sda_api::services::ratelimit::RateLimitConfig::from_env();
    let limiter: std::sync::Arc<dyn astrea_sda_api::services::ratelimit::RateLimiter> =
        astrea_sda_api::services::ratelimit::build_rate_limiter_from_url(
            rate_limit_config,
            redis_url.as_deref(),
        )
        .await
        .into();

    let app = astrea_sda_api::create_router_with_auth_and_limiter(
        repo,
        astrea_sda_api::default_auth_provider(),
        limiter,
    );

    let addr = config.socket_addr();
    tracing::info!("🛰️ Astrea SDA API listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
