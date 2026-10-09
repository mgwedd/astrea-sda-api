# ==============================================================================
# ASTREA SDA API - Production Multi-Stage Dockerfile
# ==============================================================================
# Stage 1: Build & Verification Stage
FROM rust:1-alpine AS builder

WORKDIR /usr/src/astrea-sda-api

# Install build dependencies and tools
RUN apk add --no-cache build-base musl-dev pkgconfig openssl-dev openssl-libs-static curl

# Copy dependency manifests for layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src/lib.rs and src/main.rs to build dependencies
RUN mkdir -p src && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release --lib --locked && \
    rm -rf src

# Copy real source code, migrations, and test suite
COPY src ./src
COPY tests ./tests
COPY migrations ./migrations

# Touch main.rs and lib.rs to force recompilation of app source
RUN touch src/lib.rs src/main.rs

# Run full automated verification test suite prior to release packaging
RUN cargo test --release --offline --locked || cargo test --release --locked

# Build optimized production release binary
RUN cargo build --release --locked

# ==============================================================================
# Stage 1b: CesiumJS bundle for /viewer (pinned in scripts/fetch-cesium.sh)
FROM node:22-alpine AS cesium
WORKDIR /work
RUN apk add --no-cache tar
COPY scripts/fetch-cesium.sh ./
RUN ./fetch-cesium.sh /cesium

# ==============================================================================
# Stage 2: Minimal Production Runtime Container
FROM alpine:3.20 AS runner

WORKDIR /app

# Install minimal runtime certificates and timezone data
RUN apk add --no-cache ca-certificates tzdata curl && \
    addgroup -S astrea && adduser -S astrea -G astrea

# Copy compiled production binary from builder
COPY --from=builder /usr/src/astrea-sda-api/target/release/astrea-sda-api /app/astrea-sda-api

# Static CesiumJS assets served by the binary at /viewer/cesium (no reverse proxy in the image)
COPY --from=cesium /cesium /app/viewer/cesium

# Set ownership to unprivileged runner user
RUN chown -R astrea:astrea /app

USER astrea

# Production runtime defaults (override via environment variables or docker-compose)
ENV HOST=0.0.0.0
ENV PORT=8080
ENV VIEWER_ASSETS_DIR=/app/viewer/cesium
EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/swagger-ui || exit 1

CMD ["/app/astrea-sda-api"]
