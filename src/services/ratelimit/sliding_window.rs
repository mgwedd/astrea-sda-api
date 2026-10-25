//! Sliding Window Counter Rate Limiter — Redis-backed with atomic Lua script.
//!
//! A hybrid approach that blends two fixed-window STRING counters using a weighted
//! average to approximate a true sliding window. Offers near-exact accuracy with
//! the same low memory footprint as a fixed window (two keys per client).
//!
//! **How it works:**
//! - Keeps two fixed-window counters: `current` and `previous`.
//! - Computes a weighted estimate: `prev_count * (1 - elapsed) + current_count`,
//!   where `elapsed` is how far into the current window we are (0.0 → 1.0).
//! - At the start of a new window (elapsed ≈ 0), the previous window counts fully.
//!   At the end (elapsed ≈ 1), the previous window counts almost nothing.
//! - This smooths the boundary spike that plagues fixed windows.
//!
//! **Estimate-then-increment:** The script checks the estimated count *before*
//! incrementing. If the estimate already exceeds the limit, it returns immediately
//! without writing anything — avoiding inflating the counter on denied requests.
//!
//! **Dual keys with hash tags:** Both keys use `{base}:windowNum` format so they
//! always map to the same Redis Cluster hash slot — required for multi-key Lua scripts.
//!
//! Reference: <https://redis.io/tutorials/howtos/ratelimiting/#3-sliding-window-counter>

use super::{RateLimitDecision, RateLimitError, RateLimiter};
use async_trait::async_trait;
use redis::aio::ConnectionManager;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Atomic Lua script for the sliding window counter algorithm.
///
/// KEYS[1] = current window counter key
/// KEYS[2] = previous window counter key
/// ARGV[1] = max_requests (limit)
/// ARGV[2] = window_seconds
/// ARGV[3] = elapsed (0.0 to 1.0 — fraction into current window)
///
/// Returns: `{allowed (0|1), remaining, current_count}`
pub const SLIDING_WINDOW_SCRIPT: &str = r#"
local current_key = KEYS[1]
local previous_key = KEYS[2]
local max_requests = tonumber(ARGV[1])
local window_seconds = tonumber(ARGV[2])
local elapsed = tonumber(ARGV[3])

local prev_count = tonumber(redis.call('GET', previous_key) or '0') or 0
local current_count = tonumber(redis.call('GET', current_key) or '0') or 0

local weighted_prev = prev_count * (1 - elapsed)
local estimated = weighted_prev + current_count

if estimated >= max_requests then
    return { 0, 0, math.floor(current_count) }
end

local new_count = redis.call('INCR', current_key)
if new_count == 1 then
    redis.call('EXPIRE', current_key, window_seconds * 2)
end

local new_estimate = weighted_prev + new_count
local remaining = math.max(0, math.floor(max_requests - new_estimate))

return { 1, remaining, new_count }
"#;

/// Sliding window counter rate limiter backed by Redis.
pub struct SlidingWindowLimiter {
    redis_conn: ConnectionManager,
    max_requests: u64,
    window_seconds: u64,
    key_prefix: String,
    script: redis::Script,
    trust_forwarded_for: bool,
}

impl SlidingWindowLimiter {
    /// Create a new sliding window counter limiter.
    ///
    /// * `max_requests` — maximum requests allowed within the window.
    /// * `window` — rolling time window duration.
    /// * `key_prefix` — Redis key prefix (e.g., `"rl:"`).
    pub fn new(
        redis_conn: ConnectionManager,
        max_requests: u64,
        window: Duration,
        key_prefix: String,
    ) -> Self {
        Self::with_trust(redis_conn, max_requests, window, key_prefix, false)
    }

    /// Create a new sliding window counter limiter with explicit `trust_forwarded_for` configuration.
    pub fn with_trust(
        redis_conn: ConnectionManager,
        max_requests: u64,
        window: Duration,
        key_prefix: String,
        trust_forwarded_for: bool,
    ) -> Self {
        Self {
            redis_conn,
            max_requests,
            window_seconds: window.as_secs().max(1),
            key_prefix,
            script: redis::Script::new(SLIDING_WINDOW_SCRIPT),
            trust_forwarded_for,
        }
    }
}

#[async_trait]
impl RateLimiter for SlidingWindowLimiter {
    fn trust_forwarded_for(&self) -> bool {
        self.trust_forwarded_for
    }
    async fn check(&self, key: &str) -> Result<RateLimitDecision, RateLimitError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let current_window = now / self.window_seconds;
        let previous_window = current_window.saturating_sub(1);

        // Hash tags ensure both keys map to the same Redis Cluster slot
        let base_key = format!("{}sw:{}", self.key_prefix, key);
        let current_key = format!("{{{}}}:{}", base_key, current_window);
        let previous_key = format!("{{{}}}:{}", base_key, previous_window);

        // Fraction of the way through the current window (0.0 to 1.0)
        let elapsed = (now % self.window_seconds) as f64 / self.window_seconds as f64;

        let mut conn = self.redis_conn.clone();
        let result: Vec<i64> = self
            .script
            .key(&current_key)
            .key(&previous_key)
            .arg(self.max_requests)
            .arg(self.window_seconds)
            .arg(elapsed)
            .invoke_async(&mut conn)
            .await?;

        let allowed = result.first().copied().unwrap_or(0) == 1;
        let remaining = result.get(1).copied().unwrap_or(0).max(0) as u64;

        let retry_after = if !allowed {
            // Suggest waiting until the current window ends
            let remaining_secs = (self.window_seconds as f64 * (1.0 - elapsed)).ceil() as u64;
            Some(Duration::from_secs(remaining_secs.max(1)))
        } else {
            None
        };

        Ok(RateLimitDecision {
            allowed,
            remaining,
            limit: self.max_requests,
            retry_after,
        })
    }

    fn algorithm_name(&self) -> &'static str {
        "sliding_window"
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lua_script_parses() {
        let _script = redis::Script::new(SLIDING_WINDOW_SCRIPT);
    }
}
