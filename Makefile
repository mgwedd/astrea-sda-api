# ==============================================================================
# Astrea Space Domain Awareness (SDA) API - Self-Documenting Developer Makefile
# ==============================================================================

USER ?= admin_user
ROLE ?= admin

.DEFAULT_GOAL := help

.PHONY: help build dev-hot dev prod prod-run prod-up check fmt lint test test-watch keys jwt qa smoke ui openapi sdk sdk-all sdk-ts sdk-typescript sdk-py sdk-python sdk-go sdk-java sdk-rust hooks install install-dev logs stop clean build-dev-hot build-dev build-prod urls

help: ## Display this self-documenting developer help menu
	@echo "=============================================================================="
	@echo "🛰️  Astrea SDA API Developer Makefile"
	@echo "=============================================================================="
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)
	@echo ""

##@ Build & Containers

build: ## Build production container image (alias for make prod)
	@if [ "$(filter-out build,$(MAKECMDGOALS))" = "" ]; then \
		./scripts/docker-build.sh; \
	fi

dev-hot: ## Run host app + Docker dev stack (Postgres + Redis + Caddy) with sub-second hot-reload
	@./scripts/docker-dev-hot.sh

dev: ## Run containerized local dev stack (cargo-watch + Postgres + Redis + Caddy)
	@./scripts/docker-dev.sh

prod-run: ## Run production multi-container stack locally (api + Postgres + Redis + Caddy)
	@./scripts/docker-prod.sh

prod-up: prod-run ## Alias for make prod-run

prod: ## Build optimized production container image with build verification tests
	@./scripts/docker-build.sh

build-dev-hot: dev-hot
build-dev: dev
build-prod: prod



##@ Installation & Setup

install: ## Install compiled binary locally into ~/.cargo/bin
	@echo "Installing astrea-sda-api binary locally..."
	cargo install --path .
	@echo "✅ Installed astrea-sda-api binary into ~/.cargo/bin/astrea-sda-api"

install-dev: keys hooks install ## Full developer onboarding (keys, hooks, cargo-watch, fern, binary)
	@echo "=========================================================="
	@echo "🛠️ Installing Local Developer Tooling & Dependencies..."
	@echo "=========================================================="
	@if ! command -v cargo-watch &> /dev/null; then \
		echo "📦 Installing cargo-watch for hot reloading..."; \
		cargo install cargo-watch; \
	else \
		echo "✅ cargo-watch is already installed"; \
	fi
	@if ! command -v fern &> /dev/null && command -v npm &> /dev/null; then \
		echo "📦 Installing fern-api CLI for SDK generation..."; \
		npm install -g fern-api || true; \
	fi
	@if command -v npm &> /dev/null; then \
		echo "📦 Installing commitlint dependencies..."; \
		npm install --silent; \
	fi
	@echo "=========================================================="
	@echo "✅ Developer environment fully initialized!"
	@echo "=========================================================="

keys: ## Auto-generate local RSA 2048 keypair in .keys/ (TLS managed by Caddy)
	@./scripts/setup-keys.sh

hooks: ## Install Git quality hooks (pre-commit fmt/lint & commit-msg commitlint)
	@./scripts/setup-hooks.sh

##@ Code Quality & Testing

check: ## Fast offline compilation check without generating binaries
	cargo check --offline

fmt: ## Format Rust codebase using cargo fmt
	cargo fmt

lint: ## Run cargo clippy lints with strict warning enforcement (-D warnings)
	cargo clippy --all-targets --all-features --offline -- -D warnings

test: ## Run full workspace test suite offline
	cargo test --offline

test-watch: ## Continuously run workspace test suite on source file changes
	cargo watch -x 'test --offline'

##@ Authentication & Testing

jwt: keys ## Generate signed RS256 Bearer JWT test token (usage: make jwt USER=admin_user ROLE=admin)
	@./scripts/make-jwt.sh $(USER) $(ROLE)

qa: ## Run comprehensive end-to-end QA suite across In-Memory and Distributed stacks
	@python3 scripts/run_e2e_qa.py

smoke: ## Run 10-step live smoke test against running API server (default: http://localhost:8080)
	@./scripts/qa-smoke-test.sh

##@ OpenAPI & SDK Generation

ui: ## Launch interactive OpenAPI UI documentation selector
	@./scripts/dev-ui.sh

openapi: ## Verify OpenAPI 3.0 schema contract stability and JSON sync
	cargo test --test openapi_contract_tests

sdk: sdk-all ## Generate all client SDKs (TypeScript, Python, Go, Java, Rust)

sdk-all: ## Generate all SDKs via Fern
	@if command -v fern &> /dev/null; then \
		fern generate --group local; \
	else \
		npx fern generate --group local; \
	fi

sdk-ts: sdk-typescript ## Shortcut for make sdk-typescript
sdk-typescript: ## Generate TypeScript / Node.js SDK
	@if command -v fern &> /dev/null; then \
		fern generate --group ts; \
	else \
		npx fern generate --group ts; \
	fi

sdk-py: sdk-python ## Shortcut for make sdk-python
sdk-python: ## Generate Python SDK
	@if command -v fern &> /dev/null; then \
		fern generate --group python; \
	else \
		npx fern generate --group python; \
	fi

sdk-go: ## Generate Go SDK
	@if command -v fern &> /dev/null; then \
		fern generate --group go; \
	else \
		npx fern generate --group go; \
	fi

sdk-java: ## Generate Java SDK
	@if command -v fern &> /dev/null; then \
		fern generate --group java; \
	else \
		npx fern generate --group java; \
	fi

sdk-rust: ## Generate Rust SDK
	@if command -v fern &> /dev/null; then \
		fern generate --group rust; \
	else \
		npx fern generate --group rust; \
	fi

##@ Docker Operations & Gateway
logs: ## Stream Docker container logs
	docker compose logs -f

urls: ## Print all accessible Caddy HTTPS and HTTP URLs
	@echo "=========================================================="
	@echo "⚡ Astrea SDA API - Caddy Gateway Endpoints"
	@echo "=========================================================="
	@echo "   - HTTPS (Automatic TLS) : https://sda.localtest.me:8443 (or https://localhost:8443)"
	@echo "   - Interactive Swagger UI: https://sda.localtest.me:8443/ (or /swagger-ui/)"
	@echo "   - ReDoc Interactive Docs: https://sda.localtest.me:8443/docs"
	@echo "   - Direct API Endpoints  : https://sda.localtest.me:8443/v1/... (or /api/v1/...)"
	@echo "   - HTTP Local Gateway    : http://sda.localtest.me:8888 (or http://localhost:8888)"
	@echo "   - Direct Container Port : http://localhost:8880"
	@echo "   (Zero accounts, zero signups, zero /etc/hosts edits required)"
	@echo "=========================================================="

stop: ## Stop all running Docker containers across environments
	-docker compose down
	-docker compose -f docker-compose.dev.yml down
	-docker compose -f docker-compose.dev-hot.yml down

clean: stop ## Stop Docker containers and clean target build directory
	cargo clean


