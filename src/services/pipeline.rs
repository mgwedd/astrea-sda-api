use crate::models::CreateSatelliteDto;
use crate::repository::SatelliteRepository;
use std::time::Duration;
use tracing::{error, info};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CelesTrakGroup {
    Stations,
    Visual,
    Starlink,
    Weather,
    Last30Days,
    Active,
}

impl CelesTrakGroup {
    pub const ALL: &'static [CelesTrakGroup] = &[
        CelesTrakGroup::Stations,
        CelesTrakGroup::Visual,
        CelesTrakGroup::Starlink,
        CelesTrakGroup::Weather,
        CelesTrakGroup::Last30Days,
        CelesTrakGroup::Active,
    ];

    pub const CURATED: &'static [CelesTrakGroup] = &[
        CelesTrakGroup::Stations,
        CelesTrakGroup::Visual,
        CelesTrakGroup::Last30Days,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            CelesTrakGroup::Stations => "stations",
            CelesTrakGroup::Visual => "visual",
            CelesTrakGroup::Starlink => "starlink",
            CelesTrakGroup::Weather => "weather",
            CelesTrakGroup::Last30Days => "last-30-days",
            CelesTrakGroup::Active => "active",
        }
    }

    pub fn all_names() -> Vec<&'static str> {
        Self::ALL.iter().map(|g| g.as_str()).collect()
    }

    pub fn from_str_strict(s: &str) -> Option<CelesTrakGroup> {
        match s.trim().to_lowercase().as_str() {
            "stations" | "station" => Some(CelesTrakGroup::Stations),
            "visual" | "brightest" => Some(CelesTrakGroup::Visual),
            "starlink" => Some(CelesTrakGroup::Starlink),
            "weather" => Some(CelesTrakGroup::Weather),
            "last-30-days" | "last_30_days" | "last30days" | "recent" => {
                Some(CelesTrakGroup::Last30Days)
            }
            "active" => Some(CelesTrakGroup::Active),
            _ => None,
        }
    }

    pub fn parse_groups(input: &str) -> Result<Vec<CelesTrakGroup>, String> {
        let trimmed = input.trim();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("curated")
            || trimmed.eq_ignore_ascii_case("default")
        {
            return Ok(Self::CURATED.to_vec());
        }
        if trimmed.eq_ignore_ascii_case("all") {
            return Ok(Self::ALL.to_vec());
        }

        let mut groups = Vec::new();
        let mut unknown = Vec::new();

        for part in trimmed.split(',') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            if p.eq_ignore_ascii_case("curated") || p.eq_ignore_ascii_case("default") {
                for g in Self::CURATED {
                    if !groups.contains(g) {
                        groups.push(*g);
                    }
                }
            } else if p.eq_ignore_ascii_case("all") {
                for g in Self::ALL {
                    if !groups.contains(g) {
                        groups.push(*g);
                    }
                }
            } else if let Some(g) = Self::from_str_strict(p) {
                if !groups.contains(&g) {
                    groups.push(g);
                }
            } else {
                unknown.push(p.to_string());
            }
        }

        if !unknown.is_empty() {
            return Err(format!(
                "Unknown satellite group(s): '{}'. Supported groups: {}, or 'curated', 'all'",
                unknown.join(", "),
                Self::all_names().join(", ")
            ));
        }

        if groups.is_empty() {
            return Ok(Self::CURATED.to_vec());
        }

        Ok(groups)
    }
}

pub struct DiscoveryPipeline;

use chrono::{DateTime, Datelike, Timelike};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OmmRecord {
    #[serde(rename = "OBJECT_NAME")]
    pub object_name: String,
    #[serde(rename = "OBJECT_ID")]
    pub object_id: Option<String>,
    #[serde(rename = "NORAD_CAT_ID")]
    pub norad_cat_id: Option<u64>,
    #[serde(rename = "CLASSIFICATION_TYPE")]
    pub classification_type: Option<String>,
    #[serde(rename = "EPOCH")]
    pub epoch: String,
    #[serde(rename = "MEAN_MOTION")]
    pub mean_motion: f64,
    #[serde(rename = "ECCENTRICITY")]
    pub eccentricity: f64,
    #[serde(rename = "INCLINATION")]
    pub inclination: f64,
    #[serde(rename = "RA_OF_ASC_NODE")]
    pub ra_of_asc_node: f64,
    #[serde(rename = "ARG_OF_PERICENTER")]
    pub arg_of_pericenter: f64,
    #[serde(rename = "MEAN_ANOMALY")]
    pub mean_anomaly: f64,
    #[serde(rename = "EPHEMERIS_TYPE")]
    pub ephemeris_type: Option<u32>,
    #[serde(rename = "ELEMENT_SET_NO")]
    pub element_set_no: Option<u32>,
    #[serde(rename = "REV_AT_EPOCH")]
    pub rev_at_epoch: Option<u64>,
    #[serde(rename = "BSTAR")]
    pub bstar: Option<f64>,
    #[serde(rename = "MEAN_MOTION_DOT")]
    pub mean_motion_dot: Option<f64>,
    #[serde(rename = "MEAN_MOTION_DDOT")]
    pub mean_motion_ddot: Option<f64>,
}

fn format_cat_id(catnr: u64) -> String {
    format!("{:05}", catnr % 100000)
}

fn format_object_id(obj_id: Option<&str>) -> String {
    if let Some(id) = obj_id {
        let clean = id.replace('-', "");
        if clean.len() >= 4 {
            let year_suffix = &clean[2..4];
            let rest = &clean[4..];
            return format!("{:<8}", format!("{}{}", year_suffix, rest));
        }
    }
    "00000A  ".to_string()
}

fn format_tle_epoch(epoch_str: &str) -> String {
    let dt_opt = DateTime::parse_from_rfc3339(epoch_str)
        .map(|d| d.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(epoch_str, "%Y-%m-%dT%H:%M:%S%.f")
                .map(|ndt| DateTime::<chrono::Utc>::from_naive_utc_and_offset(ndt, chrono::Utc))
        });

    if let Ok(dt) = dt_opt {
        let year_two = dt.year() % 100;
        let day_of_year = dt.ordinal();
        let seconds_in_day =
            dt.num_seconds_from_midnight() as f64 + dt.nanosecond() as f64 / 1_000_000_000.0;
        let day_frac = day_of_year as f64 + (seconds_in_day / 86400.0);
        format!("{:02}{:012.8}", year_two, day_frac)
    } else {
        "26000.00000000".to_string()
    }
}

fn format_mean_motion_dot(val: f64) -> String {
    let sign = if val < 0.0 { "-" } else { " " };
    let abs_val = val.abs();
    let digits = (abs_val * 100_000_000.0).round() as u64;
    format!("{}.{:08}", sign, digits % 100_000_000)
}

fn format_tle_exp(val: f64) -> String {
    if val.abs() < 1e-15 {
        return " 00000-0".to_string();
    }
    let sign = if val < 0.0 { "-" } else { " " };
    let abs_val = val.abs();
    let exp = abs_val.log10().floor() as i32 + 1;
    let mantissa = (abs_val / 10.0f64.powi(exp) * 100000.0).round() as u64;
    let exp_sign = if exp <= 0 { "-" } else { "+" };
    format!(
        "{}{:05}{}{}",
        sign,
        mantissa % 100000,
        exp_sign,
        exp.abs() % 10
    )
}

fn format_eccentricity(ecc: f64) -> String {
    let digits = (ecc.abs() * 10_000_000.0).round() as u64;
    format!("{:07}", digits % 10_000_000)
}

pub fn calculate_tle_checksum(line: &str) -> char {
    let sum: u32 = line
        .chars()
        .take(68)
        .map(|c| match c {
            '0'..='9' => c.to_digit(10).unwrap(),
            '-' => 1,
            _ => 0,
        })
        .sum();
    std::char::from_digit(sum % 10, 10).unwrap_or('0')
}

pub fn omm_record_to_tle_lines(record: &OmmRecord) -> (String, String) {
    let cat_nr = record.norad_cat_id.unwrap_or(0);
    let cat_str = format_cat_id(cat_nr);
    let class_char = record
        .classification_type
        .as_deref()
        .unwrap_or("U")
        .chars()
        .next()
        .unwrap_or('U');
    let obj_id_str = format_object_id(record.object_id.as_deref());
    let epoch_str = format_tle_epoch(&record.epoch);
    let ndot_str = format_mean_motion_dot(record.mean_motion_dot.unwrap_or(0.0));
    let nddot_str = format_tle_exp(record.mean_motion_ddot.unwrap_or(0.0));
    let bstar_str = format_tle_exp(record.bstar.unwrap_or(0.0));
    let eph_type = record.ephemeris_type.unwrap_or(0);
    let elem_set = record.element_set_no.unwrap_or(999);

    let line1_raw = format!(
        "1 {}{} {} {} {} {} {} {} {:>4}",
        cat_str,
        class_char,
        obj_id_str,
        epoch_str,
        ndot_str,
        nddot_str,
        bstar_str,
        eph_type,
        elem_set % 10000
    );
    let c1 = calculate_tle_checksum(&line1_raw);
    let line1_padded = format!("{:<68}", line1_raw);
    let line1 = format!("{}{}", &line1_padded[..68], c1);

    let inc = record.inclination;
    let raan = record.ra_of_asc_node;
    let ecc_str = format_eccentricity(record.eccentricity);
    let argp = record.arg_of_pericenter;
    let ma = record.mean_anomaly;
    let mm = record.mean_motion;
    let rev = record.rev_at_epoch.unwrap_or(0) % 100000;

    let line2_raw = format!(
        "2 {} {:8.4} {:8.4} {} {:8.4} {:8.4} {:11.8}{:5}",
        cat_str, inc, raan, ecc_str, argp, ma, mm, rev
    );
    let c2 = calculate_tle_checksum(&line2_raw);
    let line2_padded = format!("{:<68}", line2_raw);
    let line2 = format!("{}{}", &line2_padded[..68], c2);

    (line1, line2)
}

impl DiscoveryPipeline {
    /// Fetches TLE/OMM data from CelesTrak for a given group using FORMAT=json
    pub async fn fetch_group_tle(group: CelesTrakGroup) -> Result<Vec<CreateSatelliteDto>, String> {
        let url = format!(
            "https://celestrak.org/NORAD/elements/gp.php?GROUP={}&FORMAT=json",
            group.as_str()
        );

        info!("Fetching discovery feed from {}", url);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;

        let response = client
            .get(&url)
            .header("User-Agent", "astrea-sda-api/0.1.0")
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "CelesTrak API returned status {}",
                response.status()
            ));
        }

        let body_text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read text body: {}", e))?;

        Ok(Self::parse_discovery_response(&body_text))
    }

    /// Parses discovery API response text (either OMM JSON array or legacy 3-line TLE)
    pub fn parse_discovery_response(text: &str) -> Vec<CreateSatelliteDto> {
        let trimmed = text.trim();
        if trimmed.starts_with('[') || trimmed.starts_with('{') {
            if let Ok(records) = serde_json::from_str::<Vec<OmmRecord>>(trimmed) {
                return records
                    .into_iter()
                    .map(|r| {
                        let name = r.object_name.clone();
                        let (line_one, line_two) = omm_record_to_tle_lines(&r);
                        CreateSatelliteDto {
                            name,
                            line_one,
                            line_two,
                        }
                    })
                    .collect();
            }
        }
        Self::parse_tle_text(text)
    }

    /// Parses raw 3-line TLE text into a list of CreateSatelliteDto
    pub fn parse_tle_text(text: &str) -> Vec<CreateSatelliteDto> {
        let lines: Vec<&str> = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        let mut dtos = Vec::new();
        let mut idx = 0;

        while idx + 2 < lines.len() {
            let line0 = lines[idx];
            let line1 = lines[idx + 1];
            let line2 = lines[idx + 2];

            if line1.starts_with('1') && line2.starts_with('2') {
                dtos.push(CreateSatelliteDto {
                    name: line0.to_string(),
                    line_one: line1.to_string(),
                    line_two: line2.to_string(),
                });
                idx += 3;
            } else {
                idx += 1;
            }
        }

        dtos
    }

    /// Performs sync for a group and upserts results into SatelliteRepository in 500-record transaction chunks
    pub async fn sync_group(
        repo: &SatelliteRepository,
        group: CelesTrakGroup,
    ) -> Result<usize, String> {
        let dtos = Self::fetch_group_tle(group).await?;
        let total = dtos.len();

        // Perform batch upsert in 500-record transaction chunks with post-commit cache invalidation
        let processed = repo
            .batch_upsert_satellites(dtos, 500)
            .await
            .map_err(|e| format!("Batch upsert failed for group {}: {}", group.as_str(), e))?;

        info!(
            "Successfully synced {} satellites for group {} in 500-record transaction batches",
            processed,
            group.as_str()
        );
        Ok(total)
    }

    /// Starts a background worker on a dedicated low-priority OS thread with an isolated Tokio runtime
    pub fn start_background_sync(repo: SatelliteRepository, interval_hours: u64) {
        std::thread::Builder::new()
            .name("ingestion-pipeline-worker".to_string())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .thread_name("ingestion-worker")
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        error!("Failed to build ingestion pipeline Tokio runtime: {}", e);
                        return;
                    }
                };

                rt.block_on(async move {
                    let mut interval =
                        tokio::time::interval(Duration::from_secs(interval_hours * 3600));
                    let groups_env = std::env::var("DISCOVERY_SYNC_GROUPS")
                        .unwrap_or_else(|_| "curated".to_string());
                    let groups = CelesTrakGroup::parse_groups(&groups_env).unwrap_or_else(|e| {
                        error!(
                            "Invalid DISCOVERY_SYNC_GROUPS ('{}'): {}. Falling back to 'curated'",
                            groups_env, e
                        );
                        CelesTrakGroup::CURATED.to_vec()
                    });
                    let group_names: Vec<&str> = groups.iter().map(|g| g.as_str()).collect();
                    info!(
                        "🚀 Configured background CelesTrak discovery pipeline for groups: [{}] (every {}h)",
                        group_names.join(", "),
                        interval_hours
                    );

                    loop {
                        info!(
                            "Starting scheduled CelesTrak discovery pipeline sync for groups: [{}] on dedicated worker thread...",
                            group_names.join(", ")
                        );

                        for &group in &groups {
                            if let Err(e) = Self::sync_group(&repo, group).await {
                                error!("Failed discovery sync for group {}: {}", group.as_str(), e);
                            }
                        }

                        interval.tick().await;
                    }
                });
            })
            .expect("Failed to spawn ingestion pipeline background worker thread");
    }
}
