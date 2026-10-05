use crate::config::{ServerConfig, ADMIN_QUOTA_MULTIPLIER};
use axum::http::StatusCode;
use dashmap::DashMap;
use deadpool_redis::{Config as DeadpoolConfig, Pool as RedisPool, Runtime};
use moka::future::Cache as MokaCache;
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Semaphores are keyed by (user, quota) so a role change cannot reuse a semaphore of the wrong size.
pub type QuotaKey = (String, usize);
pub const QUOTA_EXCEEDED: &str = "User quota exceeded";

/// Holds one slot of a user's quota. On drop the user's semaphore is evicted once idle,
/// so `user_limits` holds only users with work in flight.
struct UserPermit {
    permit: Option<OwnedSemaphorePermit>,
    limits: Arc<DashMap<QuotaKey, Arc<Semaphore>>>,
    key: QuotaKey,
}

impl Drop for UserPermit {
    fn drop(&mut self) {
        drop(self.permit.take());
        // Acquisition happens under the same shard lock (see `acquire_user`), so an
        // idle semaphore cannot gain a holder between this check and the removal.
        let quota = self.key.1;
        self.limits
            .remove_if(&self.key, |_, s| s.available_permits() == quota);
    }
}

#[derive(Clone)]
pub struct AstreaComputeEngine {
    pub express_pool: Arc<ThreadPool>,
    pub heavy_pool: Arc<ThreadPool>,
    pub express_limit: Arc<Semaphore>,
    pub heavy_limit: Arc<Semaphore>,
    pub user_limits: Arc<DashMap<QuotaKey, Arc<Semaphore>>>,
    pub user_quota: usize,
    pub flight_tracker: MokaCache<u64, String>,
    pub redis_pool: Option<RedisPool>,
}

impl AstreaComputeEngine {
    pub fn new(config: &ServerConfig) -> Self {
        let express_pool = Arc::new(
            ThreadPoolBuilder::new()
                .num_threads(config.express_cores)
                .thread_name(|i| format!("rayon-express-{}", i))
                .build()
                .expect("Failed to build express Rayon thread pool"),
        );

        let heavy_pool = Arc::new(
            ThreadPoolBuilder::new()
                .num_threads(config.heavy_cores)
                .thread_name(|i| format!("rayon-heavy-{}", i))
                .build()
                .expect("Failed to build heavy Rayon thread pool"),
        );

        let express_limit = Arc::new(Semaphore::new(config.express_cores));
        let heavy_limit = Arc::new(Semaphore::new(config.heavy_cores));
        let user_limits = Arc::new(DashMap::new());

        let flight_tracker = MokaCache::builder()
            .max_capacity(10_000)
            .time_to_live(Duration::from_secs(300))
            .build();

        let redis_pool = if let Some(ref url) = config.redis_url {
            if !url.trim().is_empty() {
                match DeadpoolConfig::from_url(url).create_pool(Some(Runtime::Tokio1)) {
                    Ok(pool) => {
                        tracing::info!(
                            "Initialized Redis L2 connection pool for AstreaComputeEngine"
                        );
                        Some(pool)
                    }
                    Err(err) => {
                        warn!("Failed to create Redis L2 connection pool: {:?}", err);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        Self {
            express_pool,
            heavy_pool,
            express_limit,
            heavy_limit,
            user_limits,
            user_quota: config.user_quota,
            flight_tracker,
            redis_pool,
        }
    }

    /// O(1) Complexity Gatekeeper
    pub fn estimate_complexity(
        num_satellites: usize,
        duration_days: f64,
        step_seconds: f64,
    ) -> Result<u64, (StatusCode, String)> {
        if step_seconds <= 0.0 {
            return Err((
                StatusCode::BAD_REQUEST,
                "step_seconds must be greater than 0".to_string(),
            ));
        }
        let num_pairs = (num_satellites * (num_satellites.saturating_sub(1))) / 2;
        let total_steps = (duration_days * 86400.0) / step_seconds;
        const COST_PER_STEP_CONSTANT: f64 = 0.001;

        let estimated_ms = (num_pairs as f64 * total_steps * COST_PER_STEP_CONSTANT) as u64;

        if estimated_ms > 300_000 {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "Compute complexity limit exceeded. Estimated {}ms exceeds maximum allowed limit of 300,000ms",
                    estimated_ms
                ),
            ));
        }

        Ok(estimated_ms)
    }

    fn acquire_user(&self, user_id: String, user_role: &str) -> Result<UserPermit, String> {
        let quota = if user_role == "admin" {
            self.user_quota * ADMIN_QUOTA_MULTIPLIER
        } else {
            self.user_quota
        };
        let key = (user_id, quota);
        // `try_acquire` runs while the entry's shard lock is held; see `UserPermit::drop`.
        let permit = self
            .user_limits
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Semaphore::new(quota)))
            .value()
            .clone()
            .try_acquire_owned()
            .map_err(|_| QUOTA_EXCEEDED.to_string())?;
        Ok(UserPermit {
            permit: Some(permit),
            limits: self.user_limits.clone(),
            key,
        })
    }

    /// Per-user quota, swimlane choice, Rayon handoff and timeout, with no caching.
    /// Outer `Err` is an engine failure (quota, capacity, timeout, panic); the
    /// closure's own result comes back untouched in the inner `Result`.
    pub async fn run<T, E, F>(
        &self,
        estimated_ms: u64,
        user_id: String,
        user_role: String,
        compute_closure: F,
    ) -> Result<Result<T, E>, String>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(CancellationToken) -> Result<T, E> + Send + 'static,
    {
        let _user_permit = self.acquire_user(user_id, &user_role)?;

        // Swimlane routing (Rayon pool selection based on estimated_ms)
        let (pool, global_sem) = if estimated_ms <= 500 {
            (&self.express_pool, &self.express_limit)
        } else {
            (&self.heavy_pool, &self.heavy_limit)
        };

        let _global_permit = global_sem
            .acquire()
            .await
            .map_err(|_| "Compute capacity full".to_string())?;

        let (tx, rx) = oneshot::channel();
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();

        pool.spawn(move || {
            let _ = tx.send(compute_closure(token_clone));
        });

        // Timeboxing & ghost task prevention
        let _drop_guard = cancel_token.drop_guard();
        tokio::time::timeout(Duration::from_secs(30), rx)
            .await
            .map_err(|_| "Execution timed out".to_string())?
            .map_err(|_| "Worker panicked".to_string())
    }

    /// Executes compute within the L1/L2 Coalescing Swimlane Pipeline
    pub async fn execute_compute<F>(
        &self,
        request_hash: u64,
        estimated_ms: u64,
        user_id: String,
        user_role: String,
        compute_closure: F,
    ) -> Result<String, (StatusCode, String)>
    where
        F: FnOnce(CancellationToken) -> Result<String, String> + Send + 'static,
    {
        let cache_key = format!("astrea:compute:{}", request_hash);

        let redis_pool = self.redis_pool.clone();
        let this = self.clone();
        let user_role_clone = user_role.clone();
        let user_id_clone = user_id.clone();

        let result = self
            .flight_tracker
            .try_get_with(request_hash, async move {
                // 1. L2 Distributed Cache (Cluster-Global) via persistent Connection Pool
                if let Some(ref pool) = redis_pool {
                    if let Ok(mut conn) = pool.get().await {
                        if let Ok(cached) = redis::cmd("GET")
                            .arg(&cache_key)
                            .query_async::<_, String>(&mut *conn)
                            .await
                        {
                            debug!("L2 Redis HIT for compute key: {}", cache_key);
                            return Ok(cached);
                        }
                    }
                }

                let data = this
                    .run(
                        estimated_ms,
                        user_id_clone,
                        user_role_clone,
                        compute_closure,
                    )
                    .await?
                    .map_err(|e: String| e)?;

                // 6. Populate L2 Cache via persistent Connection Pool
                if let Some(ref pool) = redis_pool {
                    if let Ok(mut conn) = pool.get().await {
                        let _ = redis::cmd("SETEX")
                            .arg(&cache_key)
                            .arg(300)
                            .arg(&data)
                            .query_async::<_, ()>(&mut *conn)
                            .await;
                    }
                }

                Ok(data)
            })
            .await;

        result.map_err(|e: std::sync::Arc<String>| {
            let status = if e.as_str() == QUOTA_EXCEEDED {
                StatusCode::TOO_MANY_REQUESTS
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, e.as_ref().clone())
        })
    }
}
