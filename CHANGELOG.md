# Changelog

All notable changes to the **Astrea SDA API** and **Fern SDKs** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **CelesTrak Anti-Spam & Timestamp Freshness Gating**: Implemented `max_age_hours` (configurable limit e.g. 6h, 12h, 7 days) and `force` flags on `POST /v1/pipelines/sync` with persistent sync timestamp tracking in Postgres (`pipeline_sync_history`), in-memory data provider, and Redis/TieredCache. Outbound CelesTrak requests are automatically skipped if local data was synced within the freshness window, protecting upstream CelesTrak infrastructure from rate-limiting and redundant polling.
- **Intuitive Pipeline Sync Defaults & Multi-Group Support**: Enhanced `POST /v1/pipelines/sync` to default to `curated` (syncing `stations`, `visual`, and `last-30-days`), support `all` or comma-separated groups (`?group=stations,visual`), return per-group sync breakdowns with available group listings, and dynamically configure background ingestion via `DISCOVERY_SYNC_GROUPS`.
- **Strict Pipeline Group Validation**: Added 400 Bad Request error handling with helpful error messaging when unsupported or misspelled group names are provided (preventing silent fallback to space stations).
- **Manual Release Dispatch (`workflow_dispatch`)**: Added manual release workflow in GitHub Actions with mandatory explicit consent checkbox for Major breaking releases.
- **Selective SDK Generation**: Automatically skips Fern SDK compilation and distribution on purely `chore:` or `docs:` releases to reduce CI compute and artifact noise.
- **Pre-EA Risk Test Suite**: Comprehensive automated test suite in `tests/pre_ea_risk_remediation_tests.rs` auditing all 5 critical risk remediations.
- **Root CHANGELOG.md**: Automated changelog synchronization tracking all SemVer API and SDK releases.
- **Automated Commitlint & PR Title Validation**: Enforced Conventional Commits on git commits and pull request titles with automated GitHub Actions verification.

### Fixed
- **PostgreSQL Multi-Statement Query Execution**: Isolated schema migrations in `PostgresDataProvider::connect` into discrete DDL commands to prevent prepared statement protocol errors.
- **M2M PK JWTCA Assertion Deserialization**: Added `#[serde(default)]` to `role` on `Claims` so standard RFC 7523 client assertions without a `role` field decode cleanly and default to `editor`.
- **Tooling & Script Sanitation**: Added `-q` quiet flag and stderr log routing in `scripts/make-jwt.sh` for clean 3-part RS256 token capture.
- **HTTP Error Status Mapping**: Standardized SGP4 parsing errors strictly to 422 Unprocessable Entity and missing lookups to 404 Not Found.

### Security
- **Strict Defense-in-Depth Auth**: Enforced Bearer JWT authentication across all 18 operational satellite, astrodynamics, and pipeline endpoints with granular RBAC enforcement (`viewer` for queries, `editor` for writes, `admin` for deletions and pipeline sync).

---

## [v0.1.1] - 2026-10-02

### Added
- **Distributed Redis Rate Limiter**: Implemented sliding window and token bucket rate limiting algorithms with fallback `NoOpLimiter` when Redis is disconnected.
- **Compute Complexity Gatekeeper**: Added express vs heavy compute swimlanes to prioritize low-latency queries while preventing thread pool exhaustion on long-interval propagation.
- **L1/L2 Tiered Cache**: Integrated single-flight coalescing funnel with in-memory Moka L1 cache and optional Redis L2 cache.
- **Open-Source Caddy Gateway**: Integrated OSS reverse proxy with automatic local HTTPS and TLS termination.
- **End-to-End QA Testing Suite & Plan**: Comprehensive verification suite covering all 20 OpenAPI endpoints, auth flows, and edge cases (`docs/QA_TESTING_PLAN.md`).

### Fixed
- **SQLx 0.8.6 Upgrade**: Upgraded `sqlx` dependency to resolve Rust future incompatibility warnings.

---

## [v0.1.0] - 2026-10-01

### Added
- **High-Performance SGP4 Orbit Propagation Engine**: Pure Rust astrodynamics engine for satellite state vector calculations, look angles, and next-pass visibility.
- **Satellite Ground Track & GeoJSON Streaming**: Endpoints for satellite subpoints, footprints, and Cesium/Leaflet GeoJSON visualizations.
- **RF Doppler Shift & Illumination Analysis**: RF carrier frequency shift estimation and solar eclipse / twilight state detection.
- **Satellite Conjunction & Transit Search**: Orbital conjunction detection (CPA) and solar/lunar transit predictions.
- **Modular Authentication Architecture**: RS256 asymmetric JWT verification, RFC 7523 M2M PK JWTCA assertion exchanges, and RFC 8705 certificate-bound access tokens.
- **PostgreSQL & In-Memory Repositories**: Dual persistence drivers supporting standalone in-memory execution and PostgreSQL / Supabase storage.
- **OpenAPI 3.0 & Fern SDK Generation**: Fully documented OpenAPI 3.0 specification with automated Fern client generator for TypeScript, Python, Go, Java, and Rust.
