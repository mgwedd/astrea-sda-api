# 🤖 Astrea SDA API — Agent Operating Directives

## 1. 🛡️ Verification Gate & PR Rules

- **Zero-Warning Verification**: Before submitting or declaring a task complete, run:
  ```bash
  make fmt && make lint && make test
  ```
  Tests run offline (`cargo test --offline`). Fix root causes — never silence lints with `#[allow(...)]` or delete assertions to pass.
- **Contract Integrity**: Any API or schema change must pass `make openapi` (`cargo test --test openapi_contract_tests`).
- **PR Authorization Rule**: NEVER merge a PR. Push to a feature branch, open a PR via `gh pr create`, summarize cleanly, and **STOP** to await explicit human authorization.
- **Clean Communication**: Do not dump raw CLI test output walls into PRs or user messages. Summarize concisely in natural language.

---

## 2. 🛰️ Astrodynamics & Physics Invariants

- **Zero Self-Referential Tests**: Never test physics calculations using the same code/formula being tested. Validate against **independent ground-truth oracles** (Vallado *Fundamentals of Astrodynamics*, Skyfield ephemerides, or closed-form geometry).
- **Coordinate Frame Discipline**:
  - SGP4 propagates orbital elements directly into **TEME** (True Equator, Mean Equinox).
  - Observer altitude is in **meters**; satellite distance/slant range in **kilometers** or **meters** per OpenAPI schema contract.
  - Azimuth convention: 0° is True North, 90° East (measured clockwise).
  - All vectors in conjunction, look angle, or dot/cross products must be transformed into the **same frame** at the **same epoch** before calculation.
- **No Synthetic Physics Data**: Never mock confidence scores, delta-V residuals, or orbital states in production engine paths. If multi-epoch TLE data is unavailable, return an explicit error.
- **Runtime Safety**: Never block the Tokio async reactor with SGP4 loops. Offload CPU-heavy propagation to Rayon threadpools (`MAX_EXPRESS_CORES` / `MAX_HEAVY_CORES`).
- **Mathematical Specification Maintenance**: [`ASTRODYNAMICS_FORMULAS.md`](ASTRODYNAMICS_FORMULAS.md) is the single source of mathematical truth for all astrodynamical calculations in this repository. It **must** be maintained in exact synchronization whenever astrodynamic algorithms in `src/services/` (or related models and handlers) are maintained, fixed, refactored, or added. Any new mathematical endpoint or physics routine requires full documentation of its formulas, coordinate frames, and numerical tolerances in `ASTRODYNAMICS_FORMULAS.md`.

---

## 3. 🌐 Architecture & Networking Constraints

- **Decoupled 12-Factor Production Container**:
  - `Dockerfile` produces a minimal, unprivileged Alpine container running **only** `/app/astrea-sda-api` on port `8080`.
  - **Never bake Caddy, Nginx, or any reverse proxy into the container image.** In production, cloud load balancers (AWS ALB, GCP LB, Cloudflare, Ingress) terminate TLS.
- **Workstation Gateway**:
  - Local TLS and domain routing run via standalone Caddy (`caddy:2-alpine`) on `https://sda.localtest.me:8443`.
  - Zero `/etc/hosts` modifications; `*.localtest.me` resolves to `127.0.0.1` via public DNS.
- **Authentication**:
  - Uses RS256 JWTs. Local keypairs live in `.keys/` (`make keys` / `./scripts/setup-keys.sh`).
  - Supports RFC 7523 M2M client assertions and RFC 8705 mTLS certificate-bound tokens (`cnf.x5t#S256`).

---

## 4. 📑 OpenAPI & SDK Workflow

When altering endpoints or data models:
1. Annotate handlers and structs using `utoipa` macros.
2. Register new paths and schemas in [`src/api_docs.rs`](src/api_docs.rs).
3. Validate contract stability with `make openapi`.
4. Rebuild Fern client SDKs when public contracts change (`make sdk`).
