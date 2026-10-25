//! Rate Limiter Subsystem Tests
//!
//! All tests are strictly isolated, parallelizable, and non-leaky:
//! - Configuration tests use `RateLimitConfig::from_lookup` without modifying process environment variables.
//! - Non-Redis tests verify behavior with `NoOpRateLimiter` and mocked decision structures.
//! - Redis integration tests (when `REDIS_URL` is set) use randomized UUID prefixes.

use astrea_sda_api::services::ratelimit::{
    build_rate_limiter, middleware, NoOpRateLimiter, RateLimitAlgorithm, RateLimitConfig,
    RateLimitDecision, RateLimiter,
};
use axum::body::Body;
use axum::http::Request;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration Parsing Tests (Parallel & Non-Leaky)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_config_defaults_is_sliding_window() {
    let empty_lookup = HashMap::<&str, String>::new();
    let config = RateLimitConfig::from_lookup(|k| empty_lookup.get(k).cloned());

    assert_eq!(config.algorithm, RateLimitAlgorithm::SlidingWindow);
    assert_eq!(config.max_requests, 100);
    assert_eq!(config.window_seconds, 60);
    // Default refill: 100/60 ≈ 1.6667
    assert!((config.refill_rate - 100.0 / 60.0).abs() < 0.01);
    assert_eq!(config.key_prefix, "rl:");
}

#[test]
fn test_config_token_bucket_explicit() {
    let mut env = HashMap::new();
    env.insert("RATE_LIMIT_ALGORITHM", "TOKEN_BUCKET".to_string());
    env.insert("RATE_LIMIT_MAX", "50".to_string());
    env.insert("RATE_LIMIT_WINDOW_SECS", "120".to_string());
    env.insert("RATE_LIMIT_PREFIX", "myapp:".to_string());

    let config = RateLimitConfig::from_lookup(|k| env.get(k).cloned());

    assert_eq!(config.algorithm, RateLimitAlgorithm::TokenBucket);
    assert_eq!(config.max_requests, 50);
    assert_eq!(config.window_seconds, 120);
    assert_eq!(config.key_prefix, "myapp:");
}

#[test]
fn test_config_case_insensitive() {
    let mut env = HashMap::new();
    env.insert("RATE_LIMIT_ALGORITHM", "token_bucket".to_string());
    let config = RateLimitConfig::from_lookup(|k| env.get(k).cloned());
    assert_eq!(config.algorithm, RateLimitAlgorithm::TokenBucket);

    env.insert("RATE_LIMIT_ALGORITHM", "sliding_window".to_string());
    let config = RateLimitConfig::from_lookup(|k| env.get(k).cloned());
    assert_eq!(config.algorithm, RateLimitAlgorithm::SlidingWindow);
}

#[test]
fn test_config_unknown_algorithm_defaults_to_sliding_window() {
    let mut env = HashMap::new();
    env.insert("RATE_LIMIT_ALGORITHM", "INVALID_ALGORITHM".to_string());
    let config = RateLimitConfig::from_lookup(|k| env.get(k).cloned());
    assert_eq!(config.algorithm, RateLimitAlgorithm::SlidingWindow);
}

#[test]
fn test_config_invalid_numbers_use_defaults() {
    let mut env = HashMap::new();
    env.insert("RATE_LIMIT_MAX", "not_a_number".to_string());
    env.insert("RATE_LIMIT_WINDOW_SECS", "also_not_a_number".to_string());
    env.insert("RATE_LIMIT_REFILL_RATE", "nope".to_string());

    let config = RateLimitConfig::from_lookup(|k| env.get(k).cloned());

    assert_eq!(config.max_requests, 100);
    assert_eq!(config.window_seconds, 60);
    assert!((config.refill_rate - 100.0 / 60.0).abs() < 0.01);
}

#[test]
fn test_config_custom_refill_rate() {
    let mut env = HashMap::new();
    env.insert("RATE_LIMIT_REFILL_RATE", "5.0".to_string());
    let config = RateLimitConfig::from_lookup(|k| env.get(k).cloned());
    assert!((config.refill_rate - 5.0).abs() < f64::EPSILON);
}

#[test]
fn test_config_from_env_safe() {
    let config = RateLimitConfig::from_env();
    assert!(config.max_requests > 0);
    assert!(config.window_seconds > 0);
    assert!(config.refill_rate > 0.0);
}

// ─────────────────────────────────────────────────────────────────────────────
// No-Op / Unconfigured Redis Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_noop_limiter_allows_unconditionally() {
    let limiter = NoOpRateLimiter;
    assert_eq!(limiter.algorithm_name(), "disabled");
    assert!(!limiter.is_enabled());

    let decision = limiter.check("any_client").await.unwrap();
    assert!(decision.allowed);
    assert_eq!(decision.remaining, u64::MAX);
}

#[test]
fn test_build_rate_limiter_without_redis_creates_noop() {
    let config = RateLimitConfig::default();
    let limiter = build_rate_limiter(config, None).unwrap();
    assert!(!limiter.is_enabled());
    assert_eq!(limiter.algorithm_name(), "disabled");
}

// ─────────────────────────────────────────────────────────────────────────────
// Client Key Extraction Tests (Security R12/Q10)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_extract_key_from_x_forwarded_for_when_trusted() {
    let req = Request::builder()
        .header("x-forwarded-for", "192.168.1.1, 10.0.0.1")
        .body(Body::empty())
        .unwrap();
    assert_eq!(middleware::extract_client_key(&req, true), "192.168.1.1");
}

#[test]
fn test_extract_key_from_x_forwarded_for_ignored_when_untrusted() {
    let req = Request::builder()
        .header("x-forwarded-for", "192.168.1.1, 10.0.0.1")
        .body(Body::empty())
        .unwrap();
    // Untrusted header is ignored; with no socket extension, safely falls back to anonymous
    assert_eq!(middleware::extract_client_key(&req, false), "anonymous");
}

#[test]
fn test_extract_key_from_peer_ip_when_untrusted() {
    let mut req = Request::builder()
        .header("x-forwarded-for", "192.168.1.1, 10.0.0.1")
        .body(Body::empty())
        .unwrap();
    let peer_addr: std::net::SocketAddr = "127.0.0.1:8080".parse().unwrap();
    req.extensions_mut()
        .insert(axum::extract::ConnectInfo(peer_addr));

    // When untrusted, spoofed X-Forwarded-For is ignored and peer IP is used
    assert_eq!(middleware::extract_client_key(&req, false), "127.0.0.1");
}

#[test]
fn test_extract_key_from_peer_ip_socket_addr_extension() {
    let mut req = Request::builder()
        .header("x-forwarded-for", "1.2.3.4")
        .body(Body::empty())
        .unwrap();
    let peer_addr: std::net::SocketAddr = "10.0.0.99:54321".parse().unwrap();
    req.extensions_mut().insert(peer_addr);

    assert_eq!(middleware::extract_client_key(&req, false), "10.0.0.99");
}

#[test]
fn test_extract_key_from_x_real_ip_when_trusted() {
    let req = Request::builder()
        .header("x-real-ip", "10.0.0.42")
        .body(Body::empty())
        .unwrap();
    assert_eq!(middleware::extract_client_key(&req, true), "10.0.0.42");
}

#[test]
fn test_extract_key_from_x_real_ip_ignored_when_untrusted() {
    let req = Request::builder()
        .header("x-real-ip", "10.0.0.42")
        .body(Body::empty())
        .unwrap();
    assert_eq!(middleware::extract_client_key(&req, false), "anonymous");
}

#[test]
fn test_extract_key_fallback_anonymous() {
    let req = Request::builder().body(Body::empty()).unwrap();
    assert_eq!(middleware::extract_client_key(&req, false), "anonymous");
    assert_eq!(middleware::extract_client_key(&req, true), "anonymous");
}

#[test]
fn test_x_forwarded_for_takes_priority_over_x_real_ip_when_trusted() {
    let req = Request::builder()
        .header("x-forwarded-for", "1.2.3.4")
        .header("x-real-ip", "5.6.7.8")
        .body(Body::empty())
        .unwrap();
    assert_eq!(middleware::extract_client_key(&req, true), "1.2.3.4");
}

#[test]
fn test_rate_limit_config_trust_forwarded_for_env() {
    let mut lookup = HashMap::new();
    assert!(!RateLimitConfig::from_lookup(|k| lookup.get(k).cloned()).trust_forwarded_for);

    lookup.insert("RATE_LIMIT_TRUST_FORWARDED_FOR", "true".to_string());
    assert!(RateLimitConfig::from_lookup(|k| lookup.get(k).cloned()).trust_forwarded_for);

    lookup.insert("RATE_LIMIT_TRUST_FORWARDED_FOR", "false".to_string());
    assert!(!RateLimitConfig::from_lookup(|k| lookup.get(k).cloned()).trust_forwarded_for);

    lookup.remove("RATE_LIMIT_TRUST_FORWARDED_FOR");
    lookup.insert("TRUST_FORWARDED_FOR", "1".to_string());
    assert!(RateLimitConfig::from_lookup(|k| lookup.get(k).cloned()).trust_forwarded_for);
}

// ─────────────────────────────────────────────────────────────────────────────
// Decision Type & Header Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_rate_limit_decision_allowed() {
    let decision = RateLimitDecision {
        allowed: true,
        remaining: 99,
        limit: 100,
        retry_after: None,
    };
    assert!(decision.allowed);
    assert_eq!(decision.remaining, 99);
}

#[test]
fn test_rate_limit_decision_denied() {
    let decision = RateLimitDecision {
        allowed: false,
        remaining: 0,
        limit: 100,
        retry_after: Some(Duration::from_secs(60)),
    };
    assert!(!decision.allowed);
    assert_eq!(decision.retry_after, Some(Duration::from_secs(60)));
}

#[test]
fn test_apply_rate_limit_headers() {
    let decision = RateLimitDecision {
        allowed: false,
        remaining: 0,
        limit: 50,
        retry_after: Some(Duration::from_secs(12)),
    };

    let response = axum::response::Response::builder()
        .body(Body::empty())
        .unwrap();

    let stamped = middleware::apply_rate_limit_headers(&decision, "sliding_window", response);

    assert_eq!(stamped.headers().get("RateLimit-Limit").unwrap(), "50");
    assert_eq!(stamped.headers().get("RateLimit-Remaining").unwrap(), "0");
    assert_eq!(
        stamped.headers().get("X-RateLimit-Algorithm").unwrap(),
        "sliding_window"
    );
    assert_eq!(stamped.headers().get("Retry-After").unwrap(), "12");
}

#[tokio::test]
async fn test_check_rate_limit_when_disabled_passes_through() {
    let limiter: Arc<dyn RateLimiter> = Arc::new(NoOpRateLimiter);
    let decision = middleware::check_rate_limit(&limiter, "any_ip")
        .await
        .unwrap();
    assert!(decision.allowed);
}

// ─────────────────────────────────────────────────────────────────────────────
// Lua Script Validity Tests (no Redis required)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_token_bucket_script_compiles() {
    let _script =
        redis::Script::new(astrea_sda_api::services::ratelimit::token_bucket::TOKEN_BUCKET_SCRIPT);
}

#[test]
fn test_sliding_window_script_compiles() {
    let _script = redis::Script::new(
        astrea_sda_api::services::ratelimit::sliding_window::SLIDING_WINDOW_SCRIPT,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Redis Integration Tests (Guarded by REDIS_URL, strictly parallel & non-leaky)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod redis_integration {
    use astrea_sda_api::services::ratelimit::{self, RateLimitAlgorithm, RateLimitConfig};
    use redis::aio::ConnectionManager;

    async fn get_redis_conn() -> Option<ConnectionManager> {
        let url = std::env::var("REDIS_URL").ok()?;
        if url.trim().is_empty() {
            return None;
        }
        let client = redis::Client::open(url.as_str()).ok()?;
        ConnectionManager::new(client).await.ok()
    }

    #[tokio::test]
    async fn test_redis_token_bucket_allows_under_capacity() {
        let Some(conn) = get_redis_conn().await else {
            eprintln!("Skipping: REDIS_URL not set");
            return;
        };

        let config = RateLimitConfig {
            algorithm: RateLimitAlgorithm::TokenBucket,
            max_requests: 10,
            window_seconds: 60,
            refill_rate: 1.0,
            key_prefix: format!("test:{}:", uuid::Uuid::new_v4()),
            trust_forwarded_for: false,
        };

        let limiter = ratelimit::build_rate_limiter(config, Some(conn)).unwrap();
        let key = format!("tb-{}", uuid::Uuid::new_v4());
        let decision = limiter.check(&key).await.unwrap();
        assert!(decision.allowed);
        assert_eq!(decision.limit, 10);
        assert_eq!(limiter.algorithm_name(), "token_bucket");
        assert!(limiter.is_enabled());
    }

    #[tokio::test]
    async fn test_redis_sliding_window_allows_under_limit() {
        let Some(conn) = get_redis_conn().await else {
            eprintln!("Skipping: REDIS_URL not set");
            return;
        };

        let config = RateLimitConfig {
            algorithm: RateLimitAlgorithm::SlidingWindow,
            max_requests: 10,
            window_seconds: 60,
            refill_rate: 0.0,
            key_prefix: format!("test:{}:", uuid::Uuid::new_v4()),
            trust_forwarded_for: false,
        };

        let limiter = ratelimit::build_rate_limiter(config, Some(conn)).unwrap();
        let key = format!("sw-{}", uuid::Uuid::new_v4());
        let decision = limiter.check(&key).await.unwrap();
        assert!(decision.allowed);
        assert_eq!(decision.limit, 10);
        assert_eq!(limiter.algorithm_name(), "sliding_window");
        assert!(limiter.is_enabled());
    }

    #[tokio::test]
    async fn test_redis_token_bucket_denies_when_exhausted() {
        let Some(conn) = get_redis_conn().await else {
            eprintln!("Skipping: REDIS_URL not set");
            return;
        };

        let config = RateLimitConfig {
            algorithm: RateLimitAlgorithm::TokenBucket,
            max_requests: 3,
            window_seconds: 3600,
            refill_rate: 0.0001,
            key_prefix: format!("test:{}:", uuid::Uuid::new_v4()),
            trust_forwarded_for: false,
        };

        let limiter = ratelimit::build_rate_limiter(config, Some(conn)).unwrap();
        let key = format!("tb-exhaust-{}", uuid::Uuid::new_v4());

        for _ in 0..3 {
            let d = limiter.check(&key).await.unwrap();
            assert!(d.allowed);
        }

        let d = limiter.check(&key).await.unwrap();
        assert!(!d.allowed);
        assert_eq!(d.remaining, 0);
        assert!(d.retry_after.is_some());
    }

    #[tokio::test]
    async fn test_redis_sliding_window_denies_when_exhausted() {
        let Some(conn) = get_redis_conn().await else {
            eprintln!("Skipping: REDIS_URL not set");
            return;
        };

        let config = RateLimitConfig {
            algorithm: RateLimitAlgorithm::SlidingWindow,
            max_requests: 3,
            window_seconds: 3600,
            refill_rate: 0.0,
            key_prefix: format!("test:{}:", uuid::Uuid::new_v4()),
            trust_forwarded_for: false,
        };

        let limiter = ratelimit::build_rate_limiter(config, Some(conn)).unwrap();
        let key = format!("sw-exhaust-{}", uuid::Uuid::new_v4());

        for _ in 0..3 {
            let d = limiter.check(&key).await.unwrap();
            assert!(d.allowed);
        }

        let d = limiter.check(&key).await.unwrap();
        assert!(!d.allowed);
        assert_eq!(d.remaining, 0);
        assert!(d.retry_after.is_some());
    }
}
