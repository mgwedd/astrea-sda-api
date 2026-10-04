//! Tiered Cache Abstraction Layer (L1 Memory + L2 External Cache)
//!
//! Separates generic cache drivers from cloud-specific providers:
//! - **L1 Memory**: Moka in-memory cache
//! - **Generic L2 Driver**: `redis-rs` protocol client for Redis
//! - **Cloud L2 Provider**: Upstash TLS (`rediss://`) Redis connection

use moka::future::Cache as MokaCache;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

#[derive(Clone)]
pub struct TieredCache {
    l1_cache: MokaCache<String, String>,
    l2_redis: Option<ConnectionManager>,
    default_ttl: Duration,
}

impl TieredCache {
    pub async fn new(redis_url: Option<&str>, default_ttl: Duration) -> Self {
        let l1_cache = MokaCache::builder()
            .max_capacity(10_000)
            .time_to_live(default_ttl)
            .build();

        let effective_redis_url = redis_url
            .map(String::from)
            .or_else(|| std::env::var("REDIS_URL").ok());

        let l2_redis = if let Some(ref url) = effective_redis_url {
            if !url.trim().is_empty() {
                match redis::Client::open(url.as_str()) {
                    Ok(client) => match ConnectionManager::new(client).await {
                        Ok(manager) => {
                            tracing::info!("Connected to L2 Redis Cache at {}", url);
                            Some(manager)
                        }
                        Err(err) => {
                            warn!("Failed to create Redis connection manager: {:?}", err);
                            None
                        }
                    },
                    Err(err) => {
                        warn!("Failed to open Redis client: {:?}", err);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        TieredCache {
            l1_cache,
            l2_redis,
            default_ttl,
        }
    }

    /// Fetches a value from L1 (Memory) or L2 (Redis).
    ///
    /// Implements the **L1/L2 Coalescing Funnel**:
    /// Concurrent requests for the same key are coalesced in memory by Moka (`try_get_with`).
    /// Exactly one request checks L2 Redis; if Redis misses, only that single request executes the fallback function.
    pub async fn get_or_insert_with<T, F, Fut>(&self, key: &str, fetch_fn: F) -> Result<T, String>
    where
        T: Serialize + DeserializeOwned + Clone,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let l2_redis = self.l2_redis.clone();
        let default_ttl = self.default_ttl;
        let key_str = key.to_string();

        let json_result = self
            .l1_cache
            .try_get_with(key_str.clone(), async move {
                // 1. Check L2 Redis Cache (Single coalesced request executes network IO to Redis)
                if let Some(mut redis_conn) = l2_redis.clone() {
                    let redis_res: Result<Option<String>, redis::RedisError> =
                        redis_conn.get(&key_str).await;
                    if let Ok(Some(cached_json)) = redis_res {
                        debug!("L2 Redis Cache HIT (coalesced) for key: {}", key_str);
                        return Ok(cached_json);
                    }
                }

                // 2. L2 Cache MISS -> Execute DB query or Rayon computation fallback
                debug!(
                    "Cache MISS (coalesced). Fetching from source for key: {}",
                    key_str
                );
                let val = fetch_fn().await?;
                let json_str = serde_json::to_string(&val).map_err(|e| e.to_string())?;

                // 3. Populate L2 Redis Cache asynchronously (fire-and-forget)
                if let Some(mut redis_conn) = l2_redis {
                    let k = key_str.clone();
                    let payload = json_str.clone();
                    let ttl_secs = default_ttl.as_secs();
                    tokio::spawn(async move {
                        let _: Result<(), redis::RedisError> =
                            redis_conn.set_ex(k, payload, ttl_secs).await;
                    });
                }

                Ok(json_str)
            })
            .await
            .map_err(|e: std::sync::Arc<String>| e.as_ref().clone())?;

        serde_json::from_str::<T>(&json_result).map_err(|e| e.to_string())
    }

    /// Invalidates a key across both L1 and L2 caches
    pub async fn invalidate(&self, key: &str) {
        self.l1_cache.invalidate(key).await;

        if let Some(mut redis_conn) = self.l2_redis.clone() {
            let key_str = key.to_string();
            tokio::spawn(async move {
                let _: Result<(), redis::RedisError> = redis_conn.del(key_str).await;
            });
        }
    }

    /// Fetches a raw String directly from L1 (Memory) or L2 (Redis) without serialization
    pub async fn get_raw(&self, key: &str) -> Option<String> {
        if let Some(val) = self.l1_cache.get(key).await {
            return Some(val);
        }
        if let Some(mut redis_conn) = self.l2_redis.clone() {
            let redis_res: Result<Option<String>, redis::RedisError> = redis_conn.get(key).await;
            if let Ok(Some(val)) = redis_res {
                self.l1_cache.insert(key.to_string(), val.clone()).await;
                return Some(val);
            }
        }
        None
    }

    /// Sets a raw String in L1 (Memory) and L2 (Redis) with custom TTL
    pub async fn set_raw(&self, key: &str, value: String, ttl: Duration) {
        self.l1_cache.insert(key.to_string(), value.clone()).await;
        if let Some(mut redis_conn) = self.l2_redis.clone() {
            let key_str = key.to_string();
            let ttl_secs = ttl.as_secs();
            tokio::spawn(async move {
                let _: Result<(), redis::RedisError> =
                    redis_conn.set_ex(key_str, value, ttl_secs).await;
            });
        }
    }
}
