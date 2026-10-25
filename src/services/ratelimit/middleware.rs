//! Axum middleware layer for rate limiting.
//!
//! Extracts a client identity key from the request (IP address, or authenticated
//! user ID from the `Authorization` header) and runs it through the configured
//! `RateLimiter`. Returns standard `RateLimit-*` and `Retry-After` headers when active.

use super::{RateLimitDecision, RateLimiter};
use axum::{
    body::Body,
    http::{header, Request, Response, StatusCode},
    response::IntoResponse,
    Json,
};
use serde_json::json;
use std::sync::Arc;

/// Extract a rate limit key from the request.
///
/// If `trust_forwarded_for` is true:
/// 1. `X-Forwarded-For` header (first IP in the comma-separated chain) — for reverse-proxied deployments.
/// 2. Connected peer address (`X-Real-IP`) — direct connections / nginx proxy.
/// 3. Direct connection / peer IP from socket extensions (`ConnectInfo<SocketAddr>` or `SocketAddr`).
/// 4. Fallback to `"anonymous"`.
///
/// If `trust_forwarded_for` is false (default):
/// Client-supplied proxy headers (`X-Forwarded-For`, `X-Real-IP`) are untrusted and ignored
/// to prevent rate limit evasion through header spoofing.
/// 1. Direct connection / peer IP from socket extensions (`ConnectInfo<SocketAddr>` or `SocketAddr`).
/// 2. Fallback to `"anonymous"`.
pub fn extract_client_key(req: &Request<Body>, trust_forwarded_for: bool) -> String {
    if trust_forwarded_for {
        // Check X-Forwarded-For first (common in production behind a load balancer)
        if let Some(forwarded) = req.headers().get("x-forwarded-for") {
            if let Ok(val) = forwarded.to_str() {
                if let Some(first_ip) = val.split(',').next() {
                    let ip = first_ip.trim();
                    if !ip.is_empty() {
                        return ip.to_string();
                    }
                }
            }
        }

        // Check X-Real-IP (nginx convention)
        if let Some(real_ip) = req.headers().get("x-real-ip") {
            if let Ok(val) = real_ip.to_str() {
                let ip = val.trim();
                if !ip.is_empty() {
                    return ip.to_string();
                }
            }
        }
    }

    // Safely fall back to peer address from direct connection info
    if let Some(connect_info) = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
    {
        return connect_info.0.ip().to_string();
    }
    if let Some(addr) = req.extensions().get::<std::net::SocketAddr>() {
        return addr.ip().to_string();
    }

    "anonymous".to_string()
}

/// Build the rate-limited response with standard headers.
pub fn apply_rate_limit_headers(
    decision: &RateLimitDecision,
    algorithm: &str,
    mut response: Response<Body>,
) -> Response<Body> {
    let headers = response.headers_mut();

    // Standard rate limit headers (IETF draft-ietf-httpapi-ratelimit-headers)
    headers.insert(
        "RateLimit-Limit",
        decision.limit.to_string().parse().unwrap(),
    );
    headers.insert(
        "RateLimit-Remaining",
        decision.remaining.to_string().parse().unwrap(),
    );
    headers.insert("X-RateLimit-Algorithm", algorithm.parse().unwrap());

    if let Some(retry_after) = decision.retry_after {
        headers.insert(
            header::RETRY_AFTER,
            retry_after.as_secs().to_string().parse().unwrap(),
        );
    }

    response
}

/// Check the rate limit and either return `Ok(decision)` for allowed requests
/// or `Err(response)` for denied requests (HTTP 429).
#[allow(clippy::result_large_err)]
pub async fn check_rate_limit(
    limiter: &Arc<dyn RateLimiter>,
    client_key: &str,
) -> Result<RateLimitDecision, Response<Body>> {
    if !limiter.is_enabled() {
        return Ok(RateLimitDecision {
            allowed: true,
            remaining: u64::MAX,
            limit: 0,
            retry_after: None,
        });
    }

    match limiter.check(client_key).await {
        Ok(decision) if decision.allowed => Ok(decision),
        Ok(decision) => {
            // Rate limited: 429 Too Many Requests
            let retry_secs = decision.retry_after.map(|d| d.as_secs()).unwrap_or(60);

            let body = Json(json!({
                "error": "Too Many Requests",
                "status": 429,
                "retryAfter": retry_secs
            }));

            let response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
            let response = apply_rate_limit_headers(&decision, limiter.algorithm_name(), response);
            Err(response)
        }
        Err(err) => {
            // Redis down: fail open (allow request) but log the error
            tracing::warn!("Rate limiter error (failing open): {}", err);
            Ok(RateLimitDecision {
                allowed: true,
                remaining: 0,
                limit: 0,
                retry_after: None,
            })
        }
    }
}

/// Convenience: apply rate limit headers to an already-allowed response.
pub fn stamp_headers(
    decision: &RateLimitDecision,
    limiter: &dyn RateLimiter,
    response: Response<Body>,
) -> Response<Body> {
    if !limiter.is_enabled() {
        return response;
    }
    apply_rate_limit_headers(decision, limiter.algorithm_name(), response)
}

/// Axum middleware layer for rate limiting requests using an `Arc<dyn RateLimiter>`.
pub async fn rate_limit_layer(
    axum::extract::State(limiter): axum::extract::State<Arc<dyn RateLimiter>>,
    req: Request<Body>,
    next: axum::middleware::Next,
) -> Response<Body> {
    if !limiter.is_enabled() {
        return next.run(req).await;
    }

    let client_key = extract_client_key(&req, limiter.trust_forwarded_for());
    match limiter.check(&client_key).await {
        Ok(decision) => {
            if !decision.allowed {
                let retry_secs = decision.retry_after.map(|d| d.as_secs()).unwrap_or(60);
                let body = Json(json!({
                    "error": "Too Many Requests",
                    "status": 429,
                    "retryAfter": retry_secs
                }));
                let response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                apply_rate_limit_headers(&decision, limiter.algorithm_name(), response)
            } else {
                let response = next.run(req).await;
                apply_rate_limit_headers(&decision, limiter.algorithm_name(), response)
            }
        }
        Err(err) => {
            tracing::warn!("Rate limiter error (failing open): {}", err);
            next.run(req).await
        }
    }
}
