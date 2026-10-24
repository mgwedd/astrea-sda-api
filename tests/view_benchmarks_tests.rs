use astrea_sda_api::{
    config::ServerConfig,
    models::{Satellite, Tle},
    services::{
        astrodynamics::find_pass_schedule,
        compute::AstreaComputeEngine,
        ephemeris::{cancelled, Ephemeris},
    },
};
use chrono::{DateTime, TimeZone, Utc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn make_sat(name: &str, line1: &str, line2: &str) -> Satellite {
    Satellite {
        id: Uuid::new_v4(),
        name: name.to_string(),
        tle: Tle {
            line_one: line1.to_string(),
            line_two: line2.to_string(),
        },
        created_date: Utc::now(),
        last_modified_date: Utc::now(),
    }
}

/// 1. LEO (ISS): Low Earth Orbit, period ~93 min, near-Earth SGP4.
pub fn sat_leo_iss() -> Satellite {
    make_sat(
        "ISS (ZARYA)",
        "1 25544U 98067A   24083.89679124  .00014815  00000+0  26815-3 0  9996",
        "2 25544  51.6416 195.9189 0004543  98.7845 261.3938 15.49814442445012",
    )
}

/// 2. SSO: Sun-Synchronous Orbit, period ~100 min, near-Earth SGP4.
pub fn sat_sso() -> Satellite {
    make_sat(
        "SENTINEL-2A (SSO)",
        "1 40697U 15028A   24083.51474257  .00000134  00000+0  62489-4 0  9994",
        "2 40697  98.5701 142.1462 0001257  84.5815 275.5539 14.30794553457197",
    )
}

/// 3. MEO: Medium Earth Orbit, period ~718 min (12 h), SDP4 semi-synchronous resonance.
pub fn sat_meo_gps() -> Satellite {
    make_sat(
        "NAVSTAR 65 (USA 203 / GPS)",
        "1 34661U 09014A   24083.56730706 -.00000058  00000+0  00000+0 0  9997",
        "2 34661  55.4217 323.0118 0077894 227.1822 132.3218  2.00554865109402",
    )
}

/// 4. GEO: Geostationary Earth Orbit, period ~1436 min (24 h), SDP4 synchronous resonance.
pub fn sat_geo() -> Satellite {
    make_sat(
        "GOES 16 (GEO)",
        "1 41866U 16071A   24083.84412037  .00000085  00000+0  00000+0 0  9997",
        "2 41866   0.0252 267.7554 0001306  46.4253 286.0718  1.00271925 26867",
    )
}

/// 5. Molniya / HEO: High Eccentricity Orbit (e ~ 0.716, i ~ 63.4 deg, period ~718 min), SDP4.
pub fn sat_molniya_heo() -> Satellite {
    make_sat(
        "MOLNIYA 1-92 (HEO)",
        "1 27707U 03011A   24083.37682870  .00000143  00000+0  00000+0 0  9992",
        "2 27707  63.1492 144.3807 7159780 281.3323  16.3243  2.00593718153403",
    )
}

/// 6. Deep Space: Period > 225 min with dominant lunar-solar perturbations and resonance (period ~3840 min / 64 h).
pub fn sat_deep_space() -> Satellite {
    make_sat(
        "CHANDRA (DEEP SPACE SDP4)",
        "1 25867U 99040B   24083.17708333  .00000300  00000+0  00000+0 0  9990",
        "2 25867  26.0123  75.4321 7981234 270.1234  89.4321  0.37512345 12346",
    )
}

fn sample_epoch() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 3, 24, 12, 0, 0).unwrap()
}

// Observer ground station: lat 40.0 deg N, lon -75.0 deg W, alt 100 m
const GS_LAT: f64 = 40.0;
const GS_LON: f64 = -75.0;
const GS_ALT_M: f64 = 100.0;
const MIN_ELEVATION_DEG: f64 = 10.0;

#[derive(Debug, Clone)]
pub struct TimingStats {
    pub min_us: f64,
    pub p50_us: f64,
    pub p95_us: f64,
    pub max_us: f64,
    pub mean_us: f64,
}

impl TimingStats {
    pub fn compute(mut durations_us: Vec<f64>) -> Self {
        assert!(!durations_us.is_empty());
        durations_us.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let count = durations_us.len();
        let min_us = durations_us[0];
        let max_us = durations_us[count - 1];
        let sum: f64 = durations_us.iter().sum();
        let mean_us = sum / count as f64;

        let p50_idx = ((count as f64 * 0.50).round() as usize).min(count - 1);
        let p95_idx = ((count as f64 * 0.95).round() as usize).min(count - 1);

        Self {
            min_us,
            p50_us: durations_us[p50_idx],
            p95_us: durations_us[p95_idx],
            max_us,
            mean_us,
        }
    }
}

// =========================================================================
// Fast offline CI tests (run by `cargo test --offline` in < 50ms)
// =========================================================================

#[test]
fn test_orbit_regimes_ephemeris_creation_and_sampling() {
    let regimes = [
        ("LEO (ISS)", sat_leo_iss()),
        ("SSO", sat_sso()),
        ("MEO (GPS)", sat_meo_gps()),
        ("GEO", sat_geo()),
        ("Molniya/HEO", sat_molniya_heo()),
        ("Deep Space", sat_deep_space()),
    ];

    let t0 = sample_epoch();
    let cancel = CancellationToken::new();

    for (name, sat) in &regimes {
        let eph = Ephemeris::from_satellite(sat)
            .unwrap_or_else(|e| panic!("Failed to build ephemeris for {name}: {e}"));

        // Sample 11 points (0..=600s every 60s)
        let samples = eph
            .range(t0, 600, 60, &cancel)
            .unwrap_or_else(|e| panic!("Failed to sample range for {name}: {e}"));
        assert_eq!(samples.points.len(), 11, "Point count mismatch for {name}");
        assert_eq!(samples.dropped, 0, "Unexpected dropped points for {name}");
    }
}

#[test]
fn test_orbit_regimes_find_pass_schedule() {
    let regimes = [
        ("LEO (ISS)", sat_leo_iss()),
        ("SSO", sat_sso()),
        ("MEO (GPS)", sat_meo_gps()),
        ("GEO", sat_geo()),
        ("Molniya/HEO", sat_molniya_heo()),
        ("Deep Space", sat_deep_space()),
    ];

    let t0 = sample_epoch();
    let cancel = CancellationToken::new();

    for (name, sat) in &regimes {
        let sched = find_pass_schedule(
            sat,
            GS_LAT,
            GS_LON,
            GS_ALT_M,
            t0,
            MIN_ELEVATION_DEG,
            1,
            false,
            &cancel,
        )
        .unwrap_or_else(|e| panic!("Pass schedule failed for {name}: {e}"));

        assert_eq!(sched.forecast_days, 1);
    }
}

#[test]
fn test_cost_constant_validation_against_regimes() {
    let regimes = [
        ("LEO (ISS)", sat_leo_iss()),
        ("SSO", sat_sso()),
        ("MEO (GPS)", sat_meo_gps()),
        ("GEO", sat_geo()),
        ("Molniya/HEO", sat_molniya_heo()),
        ("Deep Space", sat_deep_space()),
    ];
    let t0 = sample_epoch();
    let cancel = CancellationToken::new();

    for (name, sat) in &regimes {
        let eph = Ephemeris::from_satellite(sat).unwrap();
        let samples = eph.range(t0, 600, 60, &cancel).unwrap();
        assert_eq!(samples.points.len(), 11, "Validation failed for {name}");
    }
}

#[tokio::test]
async fn test_cancellation_reactivity_in_compute_engine() {
    let config = ServerConfig::from_env();
    let engine = AstreaComputeEngine::new(&config);

    // Test cancellation in Ephemeris::range via token drop
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let cancelled_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = cancelled_flag.clone();

    let sat = sat_leo_iss();
    let t0 = sample_epoch();
    let handle = tokio::spawn(async move {
        let _ = engine
            .run(10, "test_user".into(), "viewer".into(), move |cancel| {
                let eph = Ephemeris::from_satellite(&sat).unwrap();
                let _ = started_tx.send(());
                // Ask for maximum samples (3600), checking cancellation
                let res = eph.range(t0, 3600 * 60, 60, &cancel);
                if res.is_err() {
                    flag_clone.store(true, Ordering::SeqCst);
                }
                res
            })
            .await;
    });

    let _ = started_rx.await;
    // Abort the task to trigger drop guard cancellation
    handle.abort();
    tokio::time::sleep(Duration::from_millis(15)).await;
    assert!(cancelled_flag.load(Ordering::SeqCst));
}

// =========================================================================
// Benchmark suite (marked with `#[ignore]` so offline CI stays fast)
// Run with: `cargo test --test view_benchmarks_tests --release -- --nocapture --ignored`
// =========================================================================

#[test]
#[ignore = "Performance benchmark suite; run with --release -- --ignored"]
fn bench_orbit_regimes_ephemeris_range() {
    let regimes = [
        ("LEO (ISS)", sat_leo_iss()),
        ("SSO (Sentinel-2A)", sat_sso()),
        ("MEO (GPS NAVSTAR)", sat_meo_gps()),
        ("GEO (GOES-16)", sat_geo()),
        ("Molniya/HEO (Molniya 1-92)", sat_molniya_heo()),
        ("Deep Space (Chandra)", sat_deep_space()),
    ];

    let t0 = sample_epoch();
    let cancel = CancellationToken::new();
    let iterations = 100;
    // (sample_target, span_secs, step_secs, expected_pts) - staying within MAX_SPAN_SECS = 86400 and MAX_SAMPLES = 2000
    let test_cases = [
        ("90 pts (90m/60s)", 5340, 60, 90),
        ("720 pts (12h/60s)", 43140, 60, 720),
        ("1440 pts (24h/60s)", 86340, 60, 1440),
        ("2000 pts (cap/43s)", 85957, 43, 2000),
    ];

    println!(
        "\n=== BENCHMARK 1: Ephemeris::range across Orbit Regimes (N = {} iterations) ===",
        iterations
    );
    println!(
        "{:<28} | {:<20} | {:<10} | {:<10} | {:<10} | {:<10} | {:<12}",
        "Regime",
        "Case",
        "p50 (µs/pt)",
        "p95 (µs/pt)",
        "max (µs/pt)",
        "mean (µs/pt)",
        "Total p50 (ms)"
    );
    println!(
        "{:-<28}-|-{:-<20}-|-{:-<10}-|-{:-<10}-|-{:-<10}-|-{:-<10}-|-{:-<12}",
        "", "", "", "", "", "", ""
    );

    for (name, sat) in &regimes {
        let eph = Ephemeris::from_satellite(sat).unwrap();

        for &(case_label, span, step, expected_pts) in &test_cases {
            let mut per_point_us = Vec::with_capacity(iterations);
            let mut total_ms = Vec::with_capacity(iterations);

            // Warm-up
            for _ in 0..5 {
                let _ = eph.range(t0, span, step, &cancel).unwrap();
            }

            for _ in 0..iterations {
                let start = Instant::now();
                let samples = eph.range(t0, span, step, &cancel).unwrap();
                let elapsed = start.elapsed();
                assert_eq!(samples.points.len(), expected_pts);

                let elapsed_us = elapsed.as_nanos() as f64 / 1_000.0;
                per_point_us.push(elapsed_us / expected_pts as f64);
                total_ms.push(elapsed.as_nanos() as f64 / 1_000_000.0);
            }

            let pt_stats = TimingStats::compute(per_point_us);
            let tot_stats = TimingStats::compute(total_ms);

            println!(
                "{:<28} | {:<20} | {:<10.3} | {:<10.3} | {:<10.3} | {:<10.3} | {:<12.3}",
                name,
                case_label,
                pt_stats.p50_us,
                pt_stats.p95_us,
                pt_stats.max_us,
                pt_stats.mean_us,
                tot_stats.p50_us
            );
        }
    }
}

#[test]
#[ignore = "Performance benchmark suite; run with --release -- --ignored"]
fn bench_orbit_regimes_find_pass_schedule() {
    let regimes = [
        ("LEO (ISS)", sat_leo_iss()),
        ("SSO (Sentinel-2A)", sat_sso()),
        ("MEO (GPS NAVSTAR)", sat_meo_gps()),
        ("GEO (GOES-16)", sat_geo()),
        ("Molniya/HEO (Molniya 1-92)", sat_molniya_heo()),
        ("Deep Space (Chandra)", sat_deep_space()),
    ];

    let t0 = sample_epoch();
    let cancel = CancellationToken::new();
    let day_cases = [1, 3, 14];
    let iterations = 20;

    println!(
        "\n=== BENCHMARK 2: find_pass_schedule across Orbit Regimes (N = {} iterations) ===",
        iterations
    );
    println!(
        "{:<28} | {:<5} | {:<10} | {:<10} | {:<10} | {:<10} | {:<12}",
        "Regime", "Days", "p50 (ms)", "p95 (ms)", "max (ms)", "mean (ms)", "ms / day (p50)"
    );
    println!(
        "{:-<28}-|-{:-<5}-|-{:-<10}-|-{:-<10}-|-{:-<10}-|-{:-<10}-|-{:-<12}",
        "", "", "", "", "", "", ""
    );

    for (name, sat) in &regimes {
        for &days in &day_cases {
            let mut total_ms = Vec::with_capacity(iterations);

            // Warm-up
            for _ in 0..2 {
                let _ = find_pass_schedule(
                    sat,
                    GS_LAT,
                    GS_LON,
                    GS_ALT_M,
                    t0,
                    MIN_ELEVATION_DEG,
                    days,
                    false,
                    &cancel,
                )
                .unwrap();
            }

            for _ in 0..iterations {
                let start = Instant::now();
                let _res = find_pass_schedule(
                    sat,
                    GS_LAT,
                    GS_LON,
                    GS_ALT_M,
                    t0,
                    MIN_ELEVATION_DEG,
                    days,
                    false,
                    &cancel,
                )
                .unwrap();
                let elapsed = start.elapsed();
                total_ms.push(elapsed.as_nanos() as f64 / 1_000_000.0);
            }

            let stats = TimingStats::compute(total_ms);
            let ms_per_day = stats.p50_us / days as f64;

            println!(
                "{:<28} | {:<5} | {:<10.2} | {:<10.2} | {:<10.2} | {:<10.2} | {:<12.3}",
                name, days, stats.p50_us, stats.p95_us, stats.max_us, stats.mean_us, ms_per_day
            );
        }
    }
}

#[tokio::test]
#[ignore = "Performance benchmark suite; run with --release -- --ignored"]
async fn bench_compute_engine_concurrency_and_cancellation_latency() {
    let config = ServerConfig::from_env();
    let engine = Arc::new(AstreaComputeEngine::new(&config));

    println!("\n=== BENCHMARK 3: AstreaComputeEngine Throughput & Cancellation Latency ===");

    // Part A: Throughput under Concurrency
    // Simulate concurrent view requests (LEO 1440 samples) across varying concurrency levels
    let sat = Arc::new(sat_leo_iss());
    let t0 = sample_epoch();
    let concurrency_levels = [1, 3, 5, 10, 20];
    let jobs_per_client = 10;

    println!("\n--- Part A: Concurrent Throughput (Express Pool: 3 cores) ---");
    println!(
        "{:<14} | {:<12} | {:<14} | {:<16}",
        "Concurrency", "Total Jobs", "Elapsed (ms)", "Throughput (jobs/s)"
    );
    println!("{:-<14}-|-{:-<12}-|-{:-<14}-|-{:-<16}", "", "", "", "");

    for &concurrency in &concurrency_levels {
        let start = Instant::now();
        let mut handles = Vec::new();

        for c in 0..concurrency {
            let engine_ref = engine.clone();
            let sat_ref = sat.clone();
            let user_id = format!("user_{}", c);

            handles.push(tokio::spawn(async move {
                for _ in 0..jobs_per_client {
                    let s = sat_ref.clone();
                    let res = engine_ref
                        .run(2, user_id.clone(), "admin".into(), move |cancel| {
                            let eph = Ephemeris::from_satellite(&s).unwrap();
                            eph.range(t0, 1439 * 60, 60, &cancel)
                        })
                        .await;
                    assert!(res.is_ok());
                    assert!(res.unwrap().is_ok());
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let elapsed = start.elapsed();
        let total_jobs = concurrency * jobs_per_client;
        let elapsed_sec = elapsed.as_secs_f64();
        let throughput = total_jobs as f64 / elapsed_sec;

        println!(
            "{:<14} | {:<12} | {:<14.2} | {:<16.2}",
            concurrency,
            total_jobs,
            elapsed_sec * 1000.0,
            throughput
        );
    }

    // Part B: Cancellation Latency
    // Measure time from caller abort/cancellation to closure loop termination
    println!("\n--- Part B: Cancellation Latency (Time from caller cancel to worker exit) ---");
    let cancel_trials = 50;
    let mut cancellation_latencies_us = Vec::with_capacity(cancel_trials);

    for _ in 0..cancel_trials {
        let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
        let (cancelled_tx, cancelled_rx) = tokio::sync::oneshot::channel::<Instant>();
        let sat_ref = sat.clone();
        let engine_ref = engine.clone();

        let handle = tokio::spawn(async move {
            let _ = engine_ref
                .run(50, "cancel_user".into(), "admin".into(), move |cancel| {
                    let eph = Ephemeris::from_satellite(&sat_ref).unwrap();
                    let _ = started_tx.send(());

                    // Spin in long sampling
                    let n = 3600;
                    for i in 0..n {
                        if cancel.is_cancelled() {
                            let _ = cancelled_tx.send(Instant::now());
                            return Err(cancelled());
                        }
                        let _ = eph.at(t0 + chrono::Duration::seconds((i * 60) as i64));
                    }
                    Ok(())
                })
                .await;
        });

        // Wait until task is actively executing
        let _ = started_rx.await;

        // Initiate cancellation and record timestamp
        let cancel_start = Instant::now();
        handle.abort();

        // Worker detects cancel and sends Instant::now()
        if let Ok(worker_exit_time) = cancelled_rx.await {
            let latency = worker_exit_time.saturating_duration_since(cancel_start);
            cancellation_latencies_us.push(latency.as_nanos() as f64 / 1_000.0);
        }
    }

    let cancel_stats = TimingStats::compute(cancellation_latencies_us);
    println!(
        "{:<10} | {:<12} | {:<12} | {:<12} | {:<12}",
        "Trials", "p50 (µs)", "p95 (µs)", "max (µs)", "mean (µs)"
    );
    println!(
        "{:-<10}-|-{:-<12}-|-{:-<12}-|-{:-<12}-|-{:-<12}",
        "", "", "", "", ""
    );
    println!(
        "{:<10} | {:<12.2} | {:<12.2} | {:<12.2} | {:<12.2}",
        cancel_trials,
        cancel_stats.p50_us,
        cancel_stats.p95_us,
        cancel_stats.max_us,
        cancel_stats.mean_us
    );
}
