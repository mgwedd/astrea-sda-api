# Astrea SDA API & Fern SDK Release Process

This document defines the release lifecycle, versioning policy, and standard operating procedures (SOP) for the **Astrea SDA API** and its generated multi-language **Fern SDKs** (TypeScript, Python, Go, Java, Rust).

---

## 1. Release Philosophy & Governance

Astrea follows [Semantic Versioning 2.0.0 (SemVer)](https://semver.org/):

$$\text{Version} = \text{MAJOR} . \text{MINOR} . \text{PATCH}$$

| Release Level | Trigger Method | Conventional Commit Scope | SDK Build & Distribution | Breaking Changes Allowed? |
|---|---|---|---|---|
| **PATCH** (`vX.Y.Z+1`) | Automated on merge to `main` | `fix`, `perf`, `refactor`, `style`, `build`, `ci`, `test` | **Yes** — Generates & archives all 5 SDKs | ❌ Strictly Blocked |
| **MINOR** (`vX.Y+1.0`) | Automated on merge to `main` | `feat` | **Yes** — Generates & archives all 5 SDKs | ❌ Strictly Blocked |
| **CHORE / DOCS** | Automated on merge to `main` | `chore`, `docs` | **No** — Bumps version tag, skips heavy SDK generation | ❌ Strictly Blocked |
| **MAJOR** (`vX+1.0.0`) | **Manual `workflow_dispatch`** or explicit `git tag` | `feat!`, `fix!`, `BREAKING CHANGE:` | **Yes** — Generates & archives all 5 SDKs | ✅ **Authorized with Explicit Consent** |

> [!IMPORTANT]
> **The Breaking Change Block Gate**: Routine commit merges to `main` automatically block any release that produces a major version bump or contains breaking indicators (`!:` or `BREAKING CHANGE:`). This prevents accidental breaking SDK releases from routine squash merges.

---

## 2. Automated Routine Releases (Minor & Patch)

Continuous Delivery is fully automated for backward-compatible changes:

1. **Pull Request Workflow**:
   - Every PR is verified with `commitlint` and PR title validation (`feat:`, `fix:`, `chore:`, etc.).
   - The test suite (`cargo test`), Clippy (`cargo clippy`), Docker build, and OpenAPI breaking change gate (`pb33f/openapi-changes`) execute.
2. **Merge to `main`**:
   - `mathieudutour/github-tag-action@v7` inspects squashed commit messages.
   - If commits are `feat:` $\rightarrow$ Minor bump (`v0.2.0`). Full SDK build runs.
   - If commits are `fix:`, `perf:` $\rightarrow$ Patch bump (`v0.1.1`). Full SDK build runs.
   - If commits are solely `chore:` or `docs:` $\rightarrow$ Patch bump (`v0.1.2`). Git tag is created; **SDK generation is skipped** to save compute and eliminate no-op artifact noise.
   - Tarballs are packaged into `dist/` and published to GitHub Releases.
   - Production Docker container is automatically deployed to Railway.
   - OpenAPI specifications and Swagger/Redoc bundles are deployed to GitHub Pages.

---

## 3. Major Version Release SOP (Breaking Upgrades)

When releasing a major breaking version (e.g. `v1.0.0` $\rightarrow$ `v2.0.0`), follow this procedure:

### Phase 1: Advance Deprecation in `v1.x`
Before removing or breaking an endpoint or schema:
1. Mark the route/field with `#[deprecated]` and `utoipa(deprecated = true)`.
2. Add RFC 8594 `Deprecation` and `Sunset` HTTP response headers.
3. Allow a minimum 30-day deprecation window for downstream SDK consumers.

### Phase 2: Documentation & Migration Guide
Create `docs/MIGRATION_V1_TO_V2.md` documenting:
- Exact list of removed endpoints, renamed parameters, and altered response types.
- Side-by-side migration code samples in **TypeScript**, **Python**, **Go**, **Java**, and **Rust**.

### Phase 3: Triggering the Major Release
Major version releases can be triggered through either of two authorized paths:

#### Option A: Manual GitHub Actions Cockpit (`workflow_dispatch`)
1. In GitHub, go to **Actions** $\rightarrow$ **CI & SDK Release Pipeline**.
2. Click **Run workflow** on branch `main`.
3. Configure the inputs:
   - **Release semver bump type**: Select `major`.
   - **Optional explicit version**: (e.g. `2.0.0`, or leave blank to increment the current major).
   - **Explicit consent**: ☑ **Check the box** (`REQUIRED for MAJOR releases: I confirm this is an intentional breaking change`).
4. Click **Run workflow**.

*Or trigger via GitHub CLI:*
```bash
gh workflow run ci.yml \
  --ref main \
  -f release_type=major \
  -f confirm_breaking=true
```

> [!WARNING]
> If `release_type=major` is selected without checking `confirm_breaking`, the pipeline will immediately fail with `::error title=Major Release Consent Required::` and abort all operations.

#### Option B: Annotated Git Tag
Push an annotated git tag directly from an authorized repository maintainer machine:
```bash
git checkout main
git pull origin main
git tag -a v2.0.0 -m "Release v2.0.0 - Major Architecture & API Overhaul"
git push origin v2.0.0
```

---

## 4. Multi-Language SDK Artifacts

Every completed SDK release packages the following archives under GitHub Releases:

| Artifact Name | Language | Target Client Ecosystem |
|---|---|---|
| `astrea-sda-api-sdk-typescript.tar.gz` | TypeScript / JavaScript | Node.js 18+, Bun, Deno, Web |
| `astrea-sda-api-sdk-python.tar.gz` | Python | Python 3.8+ (`pip`, `poetry`, `pdm`) |
| `astrea-sda-api-sdk-go.tar.gz` | Go | Go modules (`go get`) |
| `astrea-sda-api-sdk-java.tar.gz` | Java | JVM 11+, Maven, Gradle |
| `astrea-sda-api-sdk-rust.tar.gz` | Rust | Cargo, `crates.io` |

Downstream package maintainers can download and publish these archives directly to npm, PyPI, crates.io, and Maven Central.
