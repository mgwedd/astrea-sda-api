use astrea_sda_api::{config::ServerConfig, services::compute::AstreaComputeEngine};
use axum::http::StatusCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn test_server_config_defaults_and_env_parsing() {
    let config = ServerConfig::from_env();
    // Default safe footprint when ENV vars are omitted (3 express, 1 heavy)
    assert_eq!(config.express_cores, 3);
    assert_eq!(config.heavy_cores, 1);
}

#[test]
fn test_complexity_gatekeeper_rejection_and_estimation() {
    // Normal query: 5 satellites, 1 day, 30s step -> estimated_ms = (10 * 2880 * 0.001) = 28ms
    let est = AstreaComputeEngine::estimate_complexity(5, 1.0, 30.0);
    assert!(est.is_ok());
    assert!(est.unwrap() <= 500); // Route to express pool

    // Massive query: 1000 satellites, 90 days, 1s step -> estimated_ms = (499,500 * 7,776,000 * 0.001) = 3,884,112,000ms > 300,000ms
    let err_res = AstreaComputeEngine::estimate_complexity(1000, 90.0, 1.0);
    assert!(err_res.is_err());
    let (status, msg) = err_res.err().unwrap();
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(msg.contains("Compute complexity limit exceeded"));
}

#[tokio::test]
async fn test_compute_engine_coalescing_swimlane_and_user_quota() {
    let config = ServerConfig::from_env();
    let engine = AstreaComputeEngine::new(&config);

    let counter = Arc::new(AtomicUsize::new(0));

    // 1. Single-Flight L1 Request Coalescing under 20 concurrent tasks
    let mut handles = Vec::new();
    for _ in 0..20 {
        let eng = engine.clone();
        let cnt = counter.clone();
        handles.push(tokio::spawn(async move {
            eng.execute_compute(
                9999, // request_hash
                100,  // estimated_ms <= 500 -> express pool
                "user_123".to_string(),
                "viewer".to_string(),
                move |token| {
                    if token.is_cancelled() {
                        return Err("Cancelled".to_string());
                    }
                    cnt.fetch_add(1, Ordering::SeqCst);
                    Ok("compute_result_data".to_string())
                },
            )
            .await
        }));
    }

    for h in handles {
        let res = h.await.unwrap();
        assert_eq!(res.unwrap(), "compute_result_data");
    }

    // Exactly 1 compute execution despite 20 concurrent requests due to L1 flight_tracker coalescing
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_admin_role_runs_under_its_own_quota() {
    let config = ServerConfig::from_env();
    let engine = AstreaComputeEngine::new(&config);

    // Admin user should succeed without user quota restriction
    let res = engine
        .execute_compute(
            12345,
            50,
            "admin_user_99".to_string(),
            "admin".to_string(),
            |_token| Ok("admin_success".to_string()),
        )
        .await;

    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "admin_success");
}

#[tokio::test]
async fn run_executes_on_rayon_express_pool_and_returns_inner_error_untouched() {
    let engine = AstreaComputeEngine::new(&ServerConfig::from_env());
    let name = engine
        .run(1, "u".into(), "viewer".into(), |_| -> Result<String, ()> {
            Ok(std::thread::current().name().unwrap_or("").to_string())
        })
        .await
        .unwrap()
        .unwrap();
    assert!(name.starts_with("rayon-express-"), "{name}");

    let heavy = engine
        .run(
            10_000,
            "u".into(),
            "viewer".into(),
            |_| -> Result<String, ()> {
                Ok(std::thread::current().name().unwrap_or("").to_string())
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(heavy.starts_with("rayon-heavy-"), "{heavy}");

    let inner = engine
        .run(
            1,
            "u".into(),
            "viewer".into(),
            |_| -> Result<(), &'static str> { Err("bad") },
        )
        .await
        .unwrap();
    assert_eq!(inner, Err("bad"));
}

fn block_until_cancelled(
    started: Arc<AtomicUsize>,
    stopped: Arc<AtomicUsize>,
) -> impl FnOnce(tokio_util::sync::CancellationToken) -> Result<(), ()> + Send + 'static {
    move |token| {
        started.fetch_add(1, Ordering::SeqCst);
        while !token.is_cancelled() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        stopped.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

async fn wait_for(cond: impl Fn() -> bool) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("condition not reached in 2 s");
}

#[tokio::test]
async fn user_quota_rejects_instead_of_queueing_and_evicts_idle_users() {
    // More express slots than the 3 held jobs, so the other callers reach the quota check.
    let engine = AstreaComputeEngine::new(&ServerConfig {
        express_cores: 8,
        ..ServerConfig::from_env()
    });
    let (started, stopped) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));

    // Hold the user's 3 slots with jobs that run until cancelled.
    let held: Vec<_> = (0..3)
        .map(|_| {
            let (e, s, d) = (engine.clone(), started.clone(), stopped.clone());
            tokio::spawn(async move {
                e.run(1, "u".into(), "viewer".into(), block_until_cancelled(s, d))
                    .await
            })
        })
        .collect();
    wait_for(|| started.load(Ordering::SeqCst) == 3).await;

    let over = engine
        .run(1, "u".into(), "viewer".into(), |_| Ok::<_, ()>(()))
        .await;
    assert_eq!(over.err().as_deref(), Some("User quota exceeded"));

    // Another user is unaffected, and the same id as admin has its own, 10x larger quota.
    assert!(engine
        .run(1, "other".into(), "viewer".into(), |_| Ok::<_, ()>(()))
        .await
        .is_ok());
    assert!(engine
        .run(1, "u".into(), "admin".into(), |_| Ok::<_, ()>(()))
        .await
        .is_ok());

    // Dropping the callers cancels the jobs, frees the slots and evicts the idle users.
    for h in &held {
        h.abort();
    }
    wait_for(|| stopped.load(Ordering::SeqCst) == 3).await;
    wait_for(|| engine.user_limits.is_empty()).await;
}

#[tokio::test]
async fn dropping_the_caller_cancels_a_running_job() {
    let engine = AstreaComputeEngine::new(&ServerConfig::from_env());
    let (started, stopped) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let (e, s, d) = (engine.clone(), started.clone(), stopped.clone());
    let h = tokio::spawn(async move {
        e.run(1, "u".into(), "viewer".into(), block_until_cancelled(s, d))
            .await
    });
    wait_for(|| started.load(Ordering::SeqCst) == 1).await;
    assert_eq!(stopped.load(Ordering::SeqCst), 0);
    h.abort();
    wait_for(|| stopped.load(Ordering::SeqCst) == 1).await;
}

#[tokio::test]
async fn admin_quota_is_ten_times_the_user_quota() {
    // Enough express slots that only the per-user quota can reject.
    let engine = AstreaComputeEngine::new(&ServerConfig {
        express_cores: 40,
        ..ServerConfig::from_env()
    });
    let (started, stopped) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let held: Vec<_> = (0..30)
        .map(|_| {
            let (e, s, d) = (engine.clone(), started.clone(), stopped.clone());
            tokio::spawn(async move {
                e.run(1, "a".into(), "admin".into(), block_until_cancelled(s, d))
                    .await
            })
        })
        .collect();
    wait_for(|| started.load(Ordering::SeqCst) == 30).await;

    let over = engine
        .run(1, "a".into(), "admin".into(), |_| Ok::<_, ()>(()))
        .await;
    assert_eq!(over.err().as_deref(), Some("User quota exceeded"));

    for h in &held {
        h.abort();
    }
    wait_for(|| stopped.load(Ordering::SeqCst) == 30).await;
    wait_for(|| engine.user_limits.is_empty()).await;
}
