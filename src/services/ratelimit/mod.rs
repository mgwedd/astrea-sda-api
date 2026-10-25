//! Rate Limiting Subsystem
//!
//! Provides distributed, Redis/Upstash-backed rate limiting with two swappable algorithm modules:
//! - **Sliding Window Counter** (default): Blends two fixed-window STRING counters using
//!   a weighted average to approximate a true sliding window. Near-exact accuracy with O(2)
//!   memory per client. Uses hash-tagged keys `{base}:windowNum` for Redis Cluster compatibility.
//! - **Token Bucket**: Classic token-bucket with configurable capacity and continuous refill rate.
//!   Allows controlled bursts up to bucket capacity, then enforces a steady average rate.
//!   Uses a Redis HASH with `tokens` and `last_refill` fields.
//!
//! ### Production Deployment Model:
//! Rate limiting is strictly a production capability enabled only when Redis or Upstash (Redis)
//! is configured (via `REDIS_URL`).
//! - If Redis is configured: Rate limiting is actively enforced via atomic Lua scripts (`EVAL`).
//! - If Redis is unconfigured: Rate limiting is cleanly disabled (zero-overhead no-op).
//!
//! ### Configuration:
//! Algorithm selection is controlled by `RATE_LIMIT_ALGORITHM`:
//! - `"SLIDING_WINDOW"` (default)
//! - `"TOKEN_BUCKET"`
//!
//! Reference: <https://redis.io/tutorials/howtos/ratelimiting/>

pub mod middleware;
pub mod noop;
pub mod sliding_window;
pub mod token_bucket;

pub use noop::NoOpRateLimiter;
pub use sliding_window::SlidingWindowLimiter;
pub use token_bucket::TokenBucketLimiter;

use async_trait::async_trait;
use redis::aio::ConnectionManager;
use std::time::Duration;

/// Result of a rate limit check.
#[derive(Debug, Clone)]
pub struct RateLimitDecision {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// Number of remaining requests/tokens in the current window/bucket.
    pub remaining: u64,
    /// Maximum capacity (bucket size or window limit).
    pub limit: u64,
    /// Duration until the bucket refills or the window resets.
    pub retry_after: Option<Duration>,
}

/// Trait implemented by all rate limiting algorithm backends.
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if a request identified by `key` should be allowed.
    /// Returns a `RateLimitDecision` with allow/deny and metadata.
    async fn check(&self, key: &str) -> Result<RateLimitDecision, RateLimitError>;

    /// Returns the algorithm name for logging/headers (e.g., `"sliding_window"` or `"token_bucket"`).
    fn algorithm_name(&self) -> &'static str;

    /// Returns whether rate limiting is actively enabled (false when Redis is unconfigured).
    fn is_enabled(&self) -> bool {
        true
    }

    /// Returns whether client-supplied `X-Forwarded-For` and `X-Real-IP` headers should be trusted.
    fn trust_forwarded_for(&self) -> bool {
        false
    }
}

/// Rate limiter errors.
#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Rate limiter configuration error: {0}")]
    Config(String),
}

/// Supported rate limiting algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RateLimitAlgorithm {
    #[default]
    SlidingWindow,
    TokenBucket,
}

/// Parsed rate limiter configuration.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Which algorithm to use.
    pub algorithm: RateLimitAlgorithm,
    /// Maximum requests per window (sliding window) or bucket capacity (token bucket).
    pub max_requests: u64,
    /// Window duration in seconds (sliding window counter) — used for window boundaries.
    pub window_seconds: u64,
    /// Tokens added per second (token bucket only). Continuous refill rate.
    pub refill_rate: f64,
    /// Key prefix in Redis.
    pub key_prefix: String,
    /// Whether to trust `X-Forwarded-For` and `X-Real-IP` headers (default: false).
    pub trust_forwarded_for: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            algorithm: RateLimitAlgorithm::SlidingWindow,
            max_requests: 100,
            window_seconds: 60,
            refill_rate: 100.0 / 60.0,
            key_prefix: "rl:".to_string(),
            trust_forwarded_for: false,
        }
    }
}

impl RateLimitConfig {
    /// Parse rate limit configuration from environment variables.
    ///
    /// | Variable                          | Default          | Description                          |
    /// |-----------------------------------|------------------|--------------------------------------|
    /// | `RATE_LIMIT_ALGORITHM`            | `SLIDING_WINDOW` | `SLIDING_WINDOW` or `TOKEN_BUCKET`   |
    /// | `RATE_LIMIT_MAX`                  | `100`            | Max requests / bucket capacity       |
    /// | `RATE_LIMIT_WINDOW_SECS`          | `60`             | Window size in seconds               |
    /// | `RATE_LIMIT_REFILL_RATE`          | `1.67`           | Tokens per second (token bucket)     |
    /// | `RATE_LIMIT_PREFIX`               | `rl:`            | Redis key prefix                     |
    /// | `RATE_LIMIT_TRUST_FORWARDED_FOR`  | `false`          | Trust client X-Forwarded-For headers |
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Parse configuration from a key-lookup function or map.
    /// This enables parallel, non-leaky unit testing without modifying process environment variables.
    pub fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let algorithm = match lookup("RATE_LIMIT_ALGORITHM")
            .unwrap_or_else(|| "SLIDING_WINDOW".to_string())
            .to_uppercase()
            .as_str()
        {
            "TOKEN_BUCKET" => RateLimitAlgorithm::TokenBucket,
            _ => RateLimitAlgorithm::SlidingWindow,
        };

        let max_requests = lookup("RATE_LIMIT_MAX")
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let window_seconds = lookup("RATE_LIMIT_WINDOW_SECS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(60u64);

        let refill_rate = lookup("RATE_LIMIT_REFILL_RATE")
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| max_requests as f64 / window_seconds as f64);

        let key_prefix = lookup("RATE_LIMIT_PREFIX").unwrap_or_else(|| "rl:".to_string());

        let trust_forwarded_for = lookup("RATE_LIMIT_TRUST_FORWARDED_FOR")
            .or_else(|| lookup("TRUST_FORWARDED_FOR"))
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false);

        RateLimitConfig {
            algorithm,
            max_requests,
            window_seconds,
            refill_rate,
            key_prefix,
            trust_forwarded_for,
        }
    }
}

/// Factory: build the appropriate `RateLimiter` backend based on the config and optional Redis connection.
///
/// When `redis_conn` is `Some`, builds a distributed Redis Lua-backed rate limiter.
/// When `redis_conn` is `None`, rate limiting is disabled via `NoOpRateLimiter`.
pub fn build_rate_limiter(
    config: RateLimitConfig,
    redis_conn: Option<ConnectionManager>,
) -> Result<Box<dyn RateLimiter>, RateLimitError> {
    match redis_conn {
        Some(conn) => match config.algorithm {
            RateLimitAlgorithm::SlidingWindow => {
                Ok(Box::new(sliding_window::SlidingWindowLimiter::with_trust(
                    conn,
                    config.max_requests,
                    Duration::from_secs(config.window_seconds),
                    config.key_prefix,
                    config.trust_forwarded_for,
                )))
            }
            RateLimitAlgorithm::TokenBucket => {
                Ok(Box::new(token_bucket::TokenBucketLimiter::with_trust(
                    conn,
                    config.max_requests,
                    config.refill_rate,
                    config.key_prefix,
                    config.trust_forwarded_for,
                )))
            }
        },
        None => Ok(Box::new(noop::NoOpRateLimiter)),
    }
}

/// Convenience factory: connects to Redis or Upstash at `redis_url` (or `REDIS_URL` env) if configured.
/// If Redis is not configured or connection fails, rate limiting is disabled (NoOpRateLimiter).
pub async fn build_rate_limiter_from_url(
    config: RateLimitConfig,
    redis_url: Option<&str>,
) -> Box<dyn RateLimiter> {
    let effective_redis_url = redis_url
        .map(String::from)
        .or_else(|| std::env::var("REDIS_URL").ok());

    let redis_conn = if let Some(ref url) = effective_redis_url {
        if !url.trim().is_empty() {
            match redis::Client::open(url.as_str()) {
                Ok(client) => match ConnectionManager::new(client).await {
                    Ok(manager) => {
                        tracing::info!("Connected to Redis/Upstash rate limiter at {}", url);
                        Some(manager)
                    }
                    Err(err) => {
                        tracing::warn!(
                            "Failed to create Redis connection manager for rate limiter: {:?}. Rate limiting disabled.",
                            err
                        );
                        None
                    }
                },
                Err(err) => {
                    tracing::warn!(
                        "Failed to open Redis client for rate limiter: {:?}. Rate limiting disabled.",
                        err
                    );
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    if redis_conn.is_none() {
        tracing::info!("Redis/Upstash not configured; rate limiting is disabled.");
    }

    build_rate_limiter(config, redis_conn)
        .expect("Rate limiter initialization with fallback is infallible")
}
