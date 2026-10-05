use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub express_cores: usize,
    pub heavy_cores: usize,
    /// Concurrent compute jobs a single user may have in flight, by role.
    pub compute_job_quota: RoleQuota,
    pub redis_url: Option<String>,
}

/// Used when `COMPUTE_JOB_QUOTA_BY_ROLE` is unset.
const DEFAULT_COMPUTE_JOB_QUOTA: &str = "default=5,admin=50";

/// Positive per-role limits; roles not listed get the `default` entry.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleQuota {
    by_role: HashMap<String, usize>,
    default: usize,
}

impl RoleQuota {
    /// Parses `role=limit` pairs, e.g. `default=5,editor=10,admin=50`. Malformed or zero
    /// entries are skipped with a warning (a typo must not reject every request), and a
    /// missing `default` falls back to 5.
    pub fn parse(spec: &str) -> Self {
        let mut by_role = HashMap::new();
        for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            match entry.split_once('=') {
                Some((role, n)) => match n.trim().parse::<usize>() {
                    Ok(n) if n > 0 && !role.trim().is_empty() => {
                        by_role.insert(role.trim().to_ascii_lowercase(), n);
                    }
                    _ => tracing::warn!(
                        "ignoring compute quota entry {entry:?}: needs a role and a positive integer limit"
                    ),
                },
                None => {
                    tracing::warn!("ignoring compute quota entry {entry:?}: expected role=limit")
                }
            }
        }
        let default = by_role.remove("default").unwrap_or(5);
        Self { by_role, default }
    }

    pub fn for_role(&self, role: &str) -> usize {
        self.by_role
            .get(&role.to_ascii_lowercase())
            .copied()
            .unwrap_or(self.default)
    }
}

pub type Config = ServerConfig;

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000),
            express_cores: env::var("MAX_EXPRESS_CORES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
            heavy_cores: env::var("MAX_HEAVY_CORES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1),
            compute_job_quota: RoleQuota::parse(
                &env::var("COMPUTE_JOB_QUOTA_BY_ROLE")
                    .unwrap_or_else(|_| DEFAULT_COMPUTE_JOB_QUOTA.to_string()),
            ),
            redis_url: env::var("REDIS_URL").ok(),
        }
    }

    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("Invalid HOST or PORT configuration")
    }
}
