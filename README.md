# 🛰️ Astrea Space Domain Awareness (SDA) API

High-performance, low-latency Rust API for Space Domain Awareness (SDA), real-time satellite tracking, SGP4 orbital propagation, ground station visibility predictions, and automated CelesTrak TLE synchronization.

[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange.svg)](https://www.rust-lang.org/)
[![Axum](https://img.shields.io/badge/Axum-0.7-blue.svg)](https://github.com/tokio-rs/axum)
[![Interactive OpenAPI Docs](https://img.shields.io/badge/Interactive_OpenAPI_Docs-GitHub_Pages-blue?logo=openapi-initiative&logoColor=white)](https://mgwedd.github.io/astrea-sda-api/)
[![OpenAPI Spec](https://img.shields.io/badge/OpenAPI_3.0_Spec-JSON-green?logo=json&logoColor=white)](https://mgwedd.github.io/astrea-sda-api/openapi.json)
[![Fern SDKs](https://img.shields.io/badge/Fern-SDKs-purple.svg)](https://buildwithfern.com/)
[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-brightgreen.svg)](#-license)

---

## ⚡ Quickstart (Zero to Orbit in 30 Seconds)

### 1. One-Step Developer Onboarding
Clone the repository and initialize local keys, git hooks, and dependencies:

```bash
git clone https://github.com/mgwedd/astrea-sda-api.git
cd astrea-sda-api
make install-dev
```

### 2. Launch Local Environment

Choose your preferred development workflow:

| Workflow | Command | Endpoints | Description |
| :--- | :--- | :--- | :--- |
| **Host Hot-Reload** *(Recommended)* | `make dev-hot` | `https://sda.localtest.me:8443` | Sub-second host `cargo-watch` with containerized Postgres, Redis & Caddy TLS gateway. |
| **Full Container Dev** | `make dev` | `https://sda.localtest.me:8443` | Fully containerized Docker dev stack with live file mounts. |
| **Native Rust** | `cargo run` | `http://localhost:8080` | Bare-metal local process (in-memory SQLite fallback). |
| **Local Staging / Prod** | `make prod-run` | `https://sda.localtest.me:8443` | Compiled release container stack with Caddy HTTPS proxy. |

> 💡 **Zero Configuration**: `*.localtest.me` resolves permanently to `127.0.0.1` via public DNS. No `/etc/hosts` edits, accounts, or subscriptions required. Local TLS certificates are managed automatically by Caddy.

### 3. Verify with cURL

```bash
# 1. Obtain a JWT Bearer token
TOKEN=$(curl -s -X POST "https://sda.localtest.me:8443/v1/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"email":"admin@astrea.local","password":"password123"}' | jq -r .token)

# 2. Query the 5 nearest tracked satellites (Requires Bearer Auth)
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://sda.localtest.me:8443/v1/satellites?limit=5" | jq

# 3. Find the satellite highest overhead in San Francisco right now
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://sda.localtest.me:8443/v1/overhead-satellites?lat=37.7749&lon=-122.4194&alt=150" | jq
```

---

## 🌐 Interactive Docs & API Explorer

| Interface | URL | Description |
| :--- | :--- | :--- |
| 🛠️ **Swagger UI** | [`https://sda.localtest.me:8443/`](https://sda.localtest.me:8443/) *(or `/swagger-ui`)* | Live, interactive OpenAPI request sandbox with Bearer auth. |
| 📄 **ReDoc Reference** | [`https://sda.localtest.me:8443/docs`](https://sda.localtest.me:8443/docs) | Clean, searchable reference documentation. |
| 📋 **OpenAPI JSON** | [`https://sda.localtest.me:8443/api-docs/openapi.json`](https://sda.localtest.me:8443/api-docs/openapi.json) | Raw OpenAPI 3.0 contract for code generators and linters. |
| 🌿 **Fern Dev Portal** | `npx fern docs dev` | Multi-language SDK documentation with dynamic snippets. |

---

## 🚀 Key Capabilities

* ⚡ **Sub-Millisecond SGP4 Engine**: High-performance astrodynamics built on Axum 0.7, Tokio, and SGP4 with Rayon multi-core CPU parallelism.
* 📡 **Pass & Visibility Predictions**: Topocentric look angles (Azimuth, Elevation, Range, Range Rate), Doppler shift, and multi-day ground station pass schedules (`/v1/satellites/{id}/passes`).
* 🎯 **RPO & Relative Motion (Hill / LVLH Frame)**: Local-Vertical Local-Horizontal frame tracking (radial, in-track, cross-track) and proximity operations regime classification (`/v1/satellites/{id}/relative-motion`).
* 🧭 **State Vectors & Keplerian Elements**: Instantaneous ECI (TEME), ECEF, Geodetic positions and osculating Keplerian orbital elements (`/v1/satellites/{id}/state`).
* ☄️ **Atmospheric Drag & Orbital Decay Watch**: Perigee-based re-entry regime and risk scoring, and catalog-wide scanning for uncontrolled re-entry hazards (`/v1/decay-risks`).
* 💥 **Foster 2D Conjunction Collision Probability ($P_c$)**: Encounter-plane collision probability calculation using hard-body radius (HBR) and position covariance ellipsoids (`/v1/conjunctions/collision-probability`).
* 🌍 **3D Ground Tracks & CZML**: Generate 3D satellite trajectories, GeoJSON feature collections, and Cesium-compatible CZML streams.
* 🛰️ **Maneuver & Anomaly Detection**: Detect orbital maneuvers and station-keeping delta-V burns via mean-motion and semimajor-axis residual drift across TLE epochs.
* 🏎️ **Tiered L1/L2 Caching**: Sub-millisecond response caching using an in-memory Moka L1 cache paired with a distributed Redis L2 cache and single-flight coalescing.
* 🛡️ **Distributed Rate Limiting**: Redis-backed Sliding Window Counter and Token Bucket algorithms enforcing RFC 6585 HTTP 429 quotas.
* 🔄 **Automated CelesTrak Ingestion**: Background worker syncing active satellite constellations (`stations`, `starlink`, `weather`, `visual`) with freshness gating to prevent spamming.
* 🔐 **Defense-Grade Authentication**: Dynamic RS256 JWT tokens, RFC 7523 M2M Private Key JWT client assertions, and RFC 8705 mTLS certificate-bound tokens.

---

## 🧪 Example API Queries

### 1. Multi-Day Ground Station Pass Schedule
```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://sda.localtest.me:8443/v1/satellites/<SATELLITE_UUID>/passes?lat=30.2672&lon=-97.7431&threshold_deg=10&duration_days=7" | jq
```

### 2. Relative Motion & RPO (Hill / LVLH Frame)
```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://sda.localtest.me:8443/v1/satellites/<PRIMARY_UUID>/relative-motion?target_id=<TARGET_UUID>&duration_minutes=60&step_seconds=60" | jq
```

### 3. State Vector & Osculating Keplerian Elements
```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://sda.localtest.me:8443/v1/satellites/<SATELLITE_UUID>/state" | jq
```

### 4. Atmospheric Drag & Decay Re-Entry Watch
```bash
# Scan catalog for debris and satellites falling below 250 km perigee
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://sda.localtest.me:8443/v1/decay-risks?max_perigee_km=250.0" | jq
```

### 5. Calculate Foster 2D Collision Probability ($P_c$)
```bash
curl -s -X POST "https://sda.localtest.me:8443/v1/conjunctions/collision-probability" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "missDistanceKm": 0.05,
    "relativeVelocityKms": 14.2,
    "hardBodyRadiusM": 10.0,
    "combinedPositionUncertaintyM": 50.0
  }' | jq
```

### 6. Generate 90-Minute 3D GeoJSON Ground Track
```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  "https://sda.localtest.me:8443/v1/satellites/<SATELLITE_UUID>/groundtrack?duration_minutes=90&step_seconds=30&format=geojson" | jq
```

### 7. Authenticate & Issue Scoped Bearer Token
```bash
# Generate a local test token for admin operations
./scripts/make-jwt.sh operator_user admin

# Or exchange an RFC 7523 M2M client assertion
curl -X POST "https://sda.localtest.me:8443/v1/auth/token" \
  -H "Content-Type: application/json" \
  -d '{
    "grantType": "client_credentials",
    "clientAssertionType": "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
    "clientAssertion": "<SIGNED_RSA_JWT_ASSERTION>"
  }'
```

---

## 🛠️ Developer Make Targets

| Target | Command | Purpose |
| :--- | :--- | :--- |
| **Hot-Reload Dev** | `make dev-hot` | Postgres + Redis + Caddy with host `cargo-watch` (instant compilation). |
| **Container Dev** | `make dev` | Full containerized dev stack with live code mounts. |
| **Local Staging** | `make prod-run` | Run compiled production release image locally with Caddy gateway. |
| **Build Prod Image** | `make prod` | Build optimized, unprivileged production container (`astrea-sda-api:latest`). |
| **Run Tests** | `make test` | Execute the full test suite offline (`cargo test --offline`). |
| **Lint & Format** | `make lint && make fmt` | Enforce zero-warning Clippy checks and standard Rust formatting. |
| **Verify OpenAPI** | `make openapi` | Verify OpenAPI schema contract backwards-compatibility. |
| **Generate SDKs** | `make sdk` | Generate TypeScript, Python, Go, Java, and Rust SDKs via Fern. |
| **Show Endpoints** | `make urls` | Print all active gateway URLs and container ports. |
| **Stop Containers** | `make stop` | Tear down all dev, dev-hot, and prod containers. |

---

## ⚙️ Configuration Reference

Customize runtime behavior via `.env` or container environment variables:

| Variable | Default | Description |
| :--- | :--- | :--- |
| `HOST` | `0.0.0.0` | Bind host address. |
| `PORT` | `8080` | Listening HTTP port. |
| `POSTGRES_URI` | *(none / SQLite)* | PostgreSQL connection string (`postgres://user:pass@host:5432/db`). |
| `REDIS_URL` | *(none / in-memory)* | Redis connection string (`redis://127.0.0.1:6379`) for L2 cache & rate limits. |
| `ENABLE_DISCOVERY_PIPELINE` | `true` | Enable background CelesTrak TLE ingestion worker. |
| `MAX_EXPRESS_CORES` | `4` | Rayon thread pool cap for express compute jobs (<5s). |
| `MAX_HEAVY_CORES` | `8` | Rayon thread pool cap for heavy conjunction scans. |
| `RATE_LIMIT_ALGORITHM` | `sliding_window` | Rate limiter algorithm (`sliding_window`, `token_bucket`, `noop`). |
| `RSA_PRIVATE_KEY_FILE` | `.keys/rsa_private.pem` | Path to RSA private key for RS256 token signing. |
| `RSA_PUBLIC_KEY_FILE` | `.keys/rsa_public.pem` | Path to RSA public key for RS256 token verification. |

---

## 📦 Client SDKs

Production-ready SDKs are generated directly from the OpenAPI schema using [Fern](https://buildwithfern.com/):

```bash
# Generate all SDKs locally (or target individually: make sdk-ts, sdk-py, sdk-go, sdk-java, sdk-rust)
make sdk
```

Each language below demonstrates **(1)** dynamic token fetching via RFC 7523 Private Key Client Assertion (PK JWTCA), **(2)** SDK client initialization, and **(3)** an example first call.

#### TypeScript / Node.js
```typescript
import { AstreaSdaApiClient } from "./sdks/typescript";

// 1. Fetch dynamic token via PK JWTCA
const { token } = await fetch("https://sda.localtest.me:8443/v1/auth/token", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    grantType: "client_credentials",
    clientAssertionType: "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
    clientAssertion: process.env.ASTREA_CLIENT_ASSERTION // Signed RS256 assertion
  })
}).then(res => res.json());

// 2. Initialize SDK client & 3. First call
const client = new AstreaSdaApiClient({ environment: "https://sda.localtest.me:8443", token });
const satellites = await client.satellites.listSatellites({ limit: 5 });
```

#### Python
```python
import os, requests
from sdks.python import AstreaSdaApiClient

# 1. Fetch dynamic token via PK JWTCA
res = requests.post("https://sda.localtest.me:8443/v1/auth/token", json={
    "grantType": "client_credentials",
    "clientAssertionType": "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
    "clientAssertion": os.environ["ASTREA_CLIENT_ASSERTION"]
}).json()

# 2. Initialize SDK client & 3. First call
client = AstreaSdaApiClient(base_url="https://sda.localtest.me:8443", token=res["token"])
satellites = client.satellites.list_satellites(limit=5)
```

#### Go
```go
package main

import (
    "bytes"
    "context"
    "encoding/json"
    "net/http"
    "os"
    "sdks/go/client"
)

func main() {
    // 1. Fetch dynamic token via PK JWTCA
    payload, _ := json.Marshal(map[string]string{
        "grantType":           "client_credentials",
        "clientAssertionType": "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
        "clientAssertion":     os.Getenv("ASTREA_CLIENT_ASSERTION"),
    })
    resp, _ := http.Post("https://sda.localtest.me:8443/v1/auth/token", "application/json", bytes.NewBuffer(payload))
    var auth struct{ Token string `json:"token"` }
    json.NewDecoder(resp.Body).Decode(&auth)

    // 2. Initialize SDK client & 3. First call
    sdk := client.NewClient(client.WithBaseURL("https://sda.localtest.me:8443"), client.WithToken(auth.Token))
    limit := 5
    satellites, _ := sdk.Satellites.ListSatellites(context.Background(), &client.ListSatellitesRequest{Limit: &limit})
}
```

#### Java
```java
import com.astrea.sda.AstreaSdaApiClient;
import java.net.URI;
import java.net.http.*;
import com.fasterxml.jackson.databind.ObjectMapper;

public class Main {
    public static void main(String[] args) throws Exception {
        // 1. Fetch dynamic token via PK JWTCA
        String body = """
            {"grantType":"client_credentials","clientAssertionType":"urn:ietf:params:oauth:client-assertion-type:jwt-bearer","clientAssertion":"%s"}
            """.formatted(System.getenv("ASTREA_CLIENT_ASSERTION"));

        HttpClient http = HttpClient.newHttpClient();
        HttpRequest req = HttpRequest.newBuilder().uri(URI.create("https://sda.localtest.me:8443/v1/auth/token"))
            .header("Content-Type", "application/json").POST(HttpRequest.BodyPublishers.ofString(body)).build();
        String token = new ObjectMapper().readTree(http.send(req, HttpResponse.BodyHandlers.ofString()).body()).get("token").asText();

        // 2. Initialize SDK client & 3. First call
        AstreaSdaApiClient client = AstreaSdaApiClient.builder().url("https://sda.localtest.me:8443").token(token).build();
        var satellites = client.satellites().listSatellites();
    }
}
```

#### Rust
```rust
use astrea_sda_api_sdk::AstreaSdaApiClient;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Fetch dynamic token via PK JWTCA
    let auth: Value = reqwest::Client::new()
        .post("https://sda.localtest.me:8443/v1/auth/token")
        .json(&serde_json::json!({
            "grantType": "client_credentials",
            "clientAssertionType": "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            "clientAssertion": std::env::var("ASTREA_CLIENT_ASSERTION")?
        }))
        .send().await?.json().await?;

    // 2. Initialize SDK client & 3. First call
    let client = AstreaSdaApiClient::builder()
        .base_url("https://sda.localtest.me:8443")
        .bearer_token(auth["token"].as_str().ok_or("missing token")?)
        .build()?;
    let satellites = client.satellites().list_satellites().limit(5).send().await?;
    Ok(())
}
```

---

## 🤝 Contributing & Quality Standards

We welcome open-source contributions! Please review our standards before submitting a PR:
- 📖 [**Contributing Guidelines**](CONTRIBUTING.md) — Proof-First engineering and pull request criteria.
- 🐳 [**Docker Architecture Guide**](DOCKER.md) — Multi-container networking, Caddy TLS gateway, and cloud LB separation.
- 🤖 [**Agentic Coding Guidelines**](AGENTS.md) — Operational directives and verification gates for AI pair programmers.
- 📜 [**Code of Conduct**](CODE_OF_CONDUCT.md) — Contributor Covenant 2.1 standards.
- 🔒 [**Security Policy**](SECURITY.md) — Responsible vulnerability disclosure.

---

## 📄 License

Dual-licensed under either:
* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* **MIT License** ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
