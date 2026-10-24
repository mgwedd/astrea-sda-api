//! Token Bucket Rate Limiter — Redis-backed with atomic Lua script.
//!
//! Classic token-bucket: a bucket holds up to `max_tokens` tokens. Tokens refill at
//! `refill_rate` tokens per second continuously. Each request consumes one token.
//! Allows controlled bursts up to bucket capacity while enforcing a steady average rate.
//!
//! Uses a Redis HASH with two fields: `tokens` (current count) and `last_refill`
//! (timestamp of last refill in seconds with fractional precision). The entire
//! refill-check-consume cycle runs inside a single Lua EVAL call for atomicity.
//!
//! Reference: <https://redis.io/tutorials/howtos/ratelimiting/#4-token-bucket>

use super::{RateLimitDecision, RateLimitError, RateLimiter};
use async_trait::async_trait;
use redis::aio::ConnectionManager;
use std::time::{SystemTime, UNIX_EPOCH};

/// Atomic Lua script for the token bucket algorithm.
///
/// On each invocation:
/// 1. Reads current state via `HGETALL`. On first request the hash doesn't exist,
///    so tokens defaults to `max_tokens` (a full bucket).
/// 2. Computes `elapsed * refill_rate` to determine accumulated tokens since last
///    request. Caps at `max_tokens` so tokens don't grow unbounded.
/// 3. If at least one token is available, decrements and sets `allowed = 1`.
/// 4. Persists updated state via `HSET` with a TTL of `max_tokens / refill_rate + 1`
///    (time to fully refill plus a buffer) so idle keys auto-delete.
/// 5. Timestamp comes from the caller, not Redis's clock — deterministic and testable.
///
/// Returns: `{allowed (0|1), remaining_tokens}`
pub const TOKEN_BUCKET_SCRIPT: &str = r#"
local key = KEYS[1]
local max_tokens = tonumber(ARGV[1])
local refill_rate = tonumber(ARGV[2])
local now = tonumber(ARGV[3])

local data = redis.call('HGETALL', key)
local tokens = max_tokens
local last_refill = now

if #data > 0 then
    local fields = {}
    for i = 1, #data, 2 do
        fields[data[i]] = data[i + 1]
    end
    tokens = tonumber(fields['tokens']) or max_tokens
    last_refill = tonumber(fields['last_refill']) or now
end

-- Refill tokens based on elapsed time (continuous)
local elapsed = now - last_refill
local new_tokens = elapsed * refill_rate
tokens = math.min(max_tokens, tokens + new_tokens)

local allowed = 0
local remaining = tokens

if tokens >= 1 then
    tokens = tokens - 1
    remaining = tokens
    allowed = 1
end

redis.call('HSET', key, 'tokens', tostring(tokens), 'last_refill', tostring(now))
redis.call('EXPIRE', key, math.ceil(max_tokens / refill_rate) + 1)

return { allowed, math.floor(remaining) }
"#;

/// Token bucket rate limiter backed by Redis.
pub struct TokenBucketLimiter {
    redis_conn: ConnectionManager,
    max_tokens: u64,
    refill_rate: f64,
    key_prefix: String,
    script: redis::Script,
    trust_forwarded_for: bool,
}

impl TokenBucketLimiter {
    /// Create a new token bucket limiter.
    ///
    /// * `max_tokens` — maximum tokens the bucket can hold (burst capacity).
    /// * `refill_rate` — tokens added per second (continuous refill).
    /// * `key_prefix` — Redis key prefix (e.g., `"rl:"`).
    pub fn new(
        redis_conn: ConnectionManager,
        max_tokens: u64,
        refill_rate: f64,
        key_prefix: String,
    ) -> Self {
        Self::with_trust(redis_conn, max_tokens, refill_rate, key_prefix, false)
    }

    /// Create a new token bucket limiter with explicit `trust_forwarded_for` configuration.
    pub fn with_trust(
        redis_conn: ConnectionManager,
        max_tokens: u64,
        refill_rate: f64,
        key_prefix: String,
        trust_forwarded_for: bool,
    ) -> Self {
        Self {
            redis_conn,
            max_tokens,
            refill_rate,
            key_prefix,
            script: redis::Script::new(TOKEN_BUCKET_SCRIPT),
            trust_forwarded_for,
        }
    }
}

#[async_trait]
impl RateLimiter for TokenBucketLimiter {
    fn trust_forwarded_for(&self) -> bool {
        self.trust_forwarded_for
    }
    async fn check(&self, key: &str) -> Result<RateLimitDecision, RateLimitError> {
        let redis_key = format!("{}tb:{}", self.key_prefix, key);

        // Seconds with fractional precision (matches tutorial: Date.now() / 1000)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        let mut conn = self.redis_conn.clone();
        let result: Vec<i64> = self
            .script
            .key(&redis_key)
            .arg(self.max_tokens)
            .arg(self.refill_rate)
            .arg(now)
            .invoke_async(&mut conn)
            .await?;

        let allowed = result.first().copied().unwrap_or(0) == 1;
        let remaining = result.get(1).copied().unwrap_or(0).max(0) as u64;

        let retry_after = if !allowed {
            // Time until one token refills: 1 / refill_rate seconds
            Some(std::time::Duration::from_secs_f64(
                (1.0 / self.refill_rate).ceil(),
            ))
        } else {
            None
        };

        Ok(RateLimitDecision {
            allowed,
            remaining,
            limit: self.max_tokens,
            retry_after,
        })
    }

    fn algorithm_name(&self) -> &'static str {
        "token_bucket"
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
        let _script = redis::Script::new(TOKEN_BUCKET_SCRIPT);
    }
}
