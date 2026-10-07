use crate::cache::TieredCache;
use crate::config::ServerConfig;
use crate::error::AppError;
use crate::models::{CreateSatelliteDto, Satellite, Tle, UpdateSatelliteDto};
use crate::pagination::{
    CheckpointCursor, IdentifiableCheckpoint, PaginatedResponse, PaginationMeta, PaginationQuery,
};
use crate::repository::data_provider::{
    DataProvider, MemoryDataProvider, PostgresDataProvider, SupabaseDataProvider,
};
use crate::services::compute::AstreaComputeEngine;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct SatelliteRepository {
    pub provider: Arc<dyn DataProvider>,
    pub cache: TieredCache,
    pub compute: AstreaComputeEngine,
}

impl SatelliteRepository {
    pub async fn new(redis_url: Option<&str>) -> Self {
        let cache = TieredCache::new(redis_url, Duration::from_secs(300)).await;

        let provider: Arc<dyn DataProvider> = if let (Ok(s_url), Ok(s_key)) = (
            std::env::var("SUPABASE_URL"),
            std::env::var("SUPABASE_ANON_KEY"),
        ) {
            if !s_url.trim().is_empty() && !s_key.trim().is_empty() {
                Arc::new(SupabaseDataProvider::new(s_url, s_key))
            } else {
                Self::connect_postgres().await
            }
        } else {
            Self::connect_postgres().await
        };

        Self {
            provider,
            cache,
            compute: Self::compute_engine(),
        }
    }

    /// The engine's own Redis pool is only used by `execute_compute`; views cache via `TieredCache`.
    fn compute_engine() -> AstreaComputeEngine {
        AstreaComputeEngine::new(&ServerConfig {
            redis_url: None,
            ..ServerConfig::from_env()
        })
    }

    async fn connect_postgres() -> Arc<dyn DataProvider> {
        let db_url = std::env::var("POSTGRES_URI")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok();

        if let Some(url) = db_url {
            if !url.trim().is_empty() {
                if let Ok(pg) = PostgresDataProvider::connect(&url).await {
                    return Arc::new(pg);
                }
            }
        }
        Arc::new(MemoryDataProvider::new())
    }

    pub fn with_provider(provider: Arc<dyn DataProvider>, cache: TieredCache) -> Self {
        Self {
            provider,
            cache,
            compute: Self::compute_engine(),
        }
    }

    pub async fn create_satellite(&self, dto: CreateSatelliteDto) -> Result<Satellite, AppError> {
        let sat = self.provider.create_satellite(dto).await?;
        self.cache.invalidate("satellites:list").await;
        Ok(sat)
    }

    pub async fn list_satellites(&self) -> Result<Vec<Satellite>, AppError> {
        let provider = self.provider.clone();
        self.cache
            .get_or_insert_with("satellites:list", || async move {
                provider.list_satellites().await.map_err(|e| e.to_string())
            })
            .await
            .map_err(AppError::InternalServerError)
    }

    pub async fn list_satellites_paginated(
        &self,
        query: PaginationQuery,
    ) -> Result<PaginatedResponse<Satellite>, AppError> {
        let limit = query.limit(20, 100);
        let cursor_filter =
            match &query.cursor {
                Some(c) => Some(CheckpointCursor::decode(c).map_err(|e| {
                    AppError::BadRequest(format!("Invalid pagination cursor: {}", e))
                })?),
                None => None,
            };

        let mut all_satellites = self.list_satellites().await?;
        all_satellites.sort_by(|a, b| {
            a.checkpoint_timestamp()
                .cmp(&b.checkpoint_timestamp())
                .then_with(|| a.checkpoint_id().cmp(&b.checkpoint_id()))
        });

        let total_count = all_satellites.len();

        let filtered: Vec<Satellite> = if let Some(cursor) = cursor_filter {
            all_satellites
                .into_iter()
                .skip_while(|s| {
                    let ts = s.checkpoint_timestamp();
                    let id = s.checkpoint_id();
                    ts < cursor.checkpoint_timestamp
                        || (ts == cursor.checkpoint_timestamp && id <= cursor.last_id)
                })
                .collect()
        } else {
            all_satellites
        };

        let has_more = filtered.len() > limit;
        let page_data: Vec<Satellite> = filtered.into_iter().take(limit).collect();

        let next_cursor = if has_more && !page_data.is_empty() {
            let last_item = page_data.last().unwrap();
            let checkpoint =
                CheckpointCursor::new(last_item.checkpoint_id(), last_item.checkpoint_timestamp());
            checkpoint.encode().ok()
        } else {
            None
        };

        Ok(PaginatedResponse {
            data: page_data,
            pagination: PaginationMeta {
                next_cursor,
                has_more,
                limit,
                total_count,
            },
        })
    }

    pub async fn get_satellite_by_id(&self, id: Uuid) -> Result<Satellite, AppError> {
        let cache_key = format!("satellite:{}", id);
        let provider = self.provider.clone();

        self.cache
            .get_or_insert_with(&cache_key, || async move {
                provider.get_satellite(id).await.map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| {
                if e.contains("NotFound") || e.contains("not found") {
                    AppError::NotFound
                } else {
                    AppError::InternalServerError(e)
                }
            })
    }

    pub async fn update_satellite_by_id(
        &self,
        id: Uuid,
        dto: UpdateSatelliteDto,
    ) -> Result<Satellite, AppError> {
        let updated = self.provider.update_satellite(id, dto).await?;
        self.cache.invalidate("satellites:list").await;
        self.cache.invalidate(&format!("satellite:{}", id)).await;
        Ok(updated)
    }

    pub async fn delete_satellite_by_id(&self, id: Uuid) -> Result<Satellite, AppError> {
        let sat = self.get_satellite_by_id(id).await?;
        self.provider.delete_satellite(id).await?;
        self.cache.invalidate("satellites:list").await;
        self.cache.invalidate(&format!("satellite:{}", id)).await;
        Ok(sat)
    }

    pub async fn list_tle_history(&self, satellite_id: Uuid) -> Result<Vec<Tle>, AppError> {
        self.provider.list_tle_history(satellite_id).await
    }

    pub async fn add_tle_history(&self, satellite_id: Uuid, tle: Tle) -> Result<(), AppError> {
        self.provider.add_tle_history(satellite_id, tle).await
    }

    pub async fn batch_upsert_satellites(
        &self,
        dtos: Vec<CreateSatelliteDto>,
        chunk_size: usize,
    ) -> Result<usize, AppError> {
        let mut total = 0;
        let batch_size = if chunk_size == 0 { 500 } else { chunk_size };
        for chunk in dtos.chunks(batch_size) {
            let count = self
                .provider
                .batch_upsert_satellites(chunk.to_vec())
                .await?;
            total += count;
            // Cache invalidation takes place AFTER successful transaction commit per batch chunk
            self.cache.invalidate("satellites:list").await;
        }
        Ok(total)
    }

    pub async fn get_pipeline_last_sync(
        &self,
        group: &str,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        let cache_key = format!("pipeline:last_sync:{}", group);
        if let Some(ts_str) = self.cache.get_raw(&cache_key).await {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&ts_str) {
                return Some(dt.with_timezone(&chrono::Utc));
            }
        }
        if let Ok(Some(dt)) = self.provider.get_pipeline_sync_time(group).await {
            let ts_str = dt.to_rfc3339();
            self.cache
                .set_raw(&cache_key, ts_str, Duration::from_secs(86400 * 30))
                .await;
            return Some(dt);
        }
        None
    }

    pub async fn record_pipeline_sync(&self, group: &str, count: usize) -> Result<(), AppError> {
        let now = chrono::Utc::now();
        let cache_key = format!("pipeline:last_sync:{}", group);
        self.cache
            .set_raw(
                &cache_key,
                now.to_rfc3339(),
                Duration::from_secs(86400 * 30),
            )
            .await;
        let _ = self.provider.record_pipeline_sync_time(group, count).await;
        Ok(())
    }
}
