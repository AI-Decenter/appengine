SHELL := /bin/bash
ROOT := $(shell pwd)

RUST_TOOLCHAIN := 1.90.0
SQLX_FEATURES := postgres
DATABASE_URL ?= postgres://aether:postgres@localhost:5432/aether_dev
TEST_DATABASE_URL ?= postgres://aether:postgres@localhost:5432/aether_test
PG_CONTAINER_NAME ?= aether-pg-test
PG_IMAGE ?= postgres:15
SQLX ?= sqlx

.PHONY: all build fmt lint test clean sqlx-prepare crd db-start test-no-db test-db helm-lint helm-template test-ci
.PHONY: base-image-build base-image-scan base-image-sbom base-image-push

all: build

build:
	cargo build --workspace --all-targets

fmt:
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

test-no-db:
	cargo test --workspace --all-features -- --nocapture

db-start: ensure-postgres ## Start (or ensure) local Postgres container based on DATABASE_URL
	@echo "[db-start] Postgres ready at $(DATABASE_URL)"

test: ensure-postgres ## Run full test suite after ensuring Postgres is up
	DATABASE_URL=$(DATABASE_URL) cargo test --workspace --all-features -- --nocapture

test-db: ensure-postgres ## Initialize dedicated test database and run migrations
	@echo "[test-db] Using test database URL=$(TEST_DATABASE_URL)"; \
	if ! command -v psql >/dev/null 2>&1; then echo "[test-db] ERROR: psql not found in PATH"; exit 1; fi; \
	BASE_URL=$$(echo $(TEST_DATABASE_URL) | sed -E 's#/[^/]+$#/postgres#'); \
	DB_NAME=$$(echo $(TEST_DATABASE_URL) | sed -E 's#.*/([^/?]+)(\?.*)?$#\1#'); \
	USER_PART=$$(echo $(TEST_DATABASE_URL) | sed -E 's#postgres://([^:@/]+).*#\1#'); \
	PASS_PART=$$(echo $(TEST_DATABASE_URL) | sed -nE 's#postgres://[^:]+:([^@]+)@.*#\1#p'); \
	if [ -n "$$PASS_PART" ]; then export PGPASSWORD="$$PASS_PART"; fi; \
	echo "[test-db] Ensuring database $$DB_NAME exists..."; \
	psql "$$BASE_URL" -tc "SELECT 1 FROM pg_database WHERE datname='$$DB_NAME'" | grep -q 1 || psql "$$BASE_URL" -c "CREATE DATABASE $$DB_NAME"; \
	echo "[test-db] Running migrations (control-plane)..."; \
	if ! command -v sqlx >/dev/null 2>&1; then cargo install sqlx-cli --no-default-features --features native-tls,postgres >/dev/null; fi; \
	(cd crates/control-plane && DATABASE_URL=$(TEST_DATABASE_URL) sqlx migrate run >/dev/null); \
	echo "[test-db] Done. You can now run: DATABASE_URL=$(TEST_DATABASE_URL) cargo test -p control-plane"

clean:
	cargo clean

helm-lint:
	@echo "[helm-lint] Linting charts/control-plane (if helm installed)"; \
	if command -v helm >/dev/null 2>&1; then \
	  helm lint charts/control-plane; \
	else \
	  echo "helm not installed; skipping lint"; \
	fi

helm-template:
	@echo "[helm-template] Rendering chart to stdout (if helm installed)"; \
	if command -v helm >/dev/null 2>&1; then \
	  helm template test charts/control-plane --set env.DATABASE_URL=postgres://user:pass@host:5432/db --set env.TOKENS=t_admin:admin:alice; \
	else \
	  echo "helm not installed; skipping template"; \
	fi

# CI-friendly test runner that selects DB strategy:
# - If Docker is available: use testcontainers (unset DATABASE_URL, force harness path)
# - Else: start a managed Postgres service and use DATABASE_URL
test-ci:
	@echo "[test-ci] Selecting DB strategy..."; \
	if command -v docker >/dev/null 2>&1; then \
	  echo "[test-ci] Docker detected -> using testcontainers"; \
	  unset DATABASE_URL; \
	  AETHER_FORCE_TESTCONTAINERS=1 AETHER_TEST_SHARED_POOL=0 AETHER_FAST_TEST=1 \
	  cargo test -p control-plane -- --nocapture; \
	else \
	  echo "[test-ci] Docker not available -> using managed Postgres service"; \
	  $(MAKE) ensure-postgres; \
	  DATABASE_URL=$(DATABASE_URL) AETHER_TEST_SHARED_POOL=0 AETHER_FAST_TEST=1 \
	  cargo test -p control-plane -- --nocapture; \
	fi

sqlx-prepare:
	DATABASE_URL=$(DATABASE_URL) cargo sqlx prepare --workspace -- --all-targets

.PHONY: test-full ensure-postgres schema-drift

ensure-postgres:
	@echo "[ensure-postgres] Checking database connectivity..."; \
	if ! PGPASSWORD=$$(echo $(DATABASE_URL) | sed -E 's#.*/([^:]+):([^@]+)@.*#\2#') psql "$(DATABASE_URL)" -c 'SELECT 1' >/dev/null 2>&1; then \
	  echo "[ensure-postgres] No reachable Postgres at $(DATABASE_URL). Attempting to start container $(PG_CONTAINER_NAME)..."; \
	  if command -v docker >/dev/null 2>&1; then \
	    if docker ps -a --format '{{.Names}}' | grep -q '^$(PG_CONTAINER_NAME)$$'; then \
	      docker start $(PG_CONTAINER_NAME) >/dev/null; \
	    else \
	      docker run -d --name $(PG_CONTAINER_NAME) -e POSTGRES_USER=aether -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=aether_dev -p 5432:5432 $(PG_IMAGE) >/dev/null; \
	    fi; \
	  elif command -v podman >/dev/null 2>&1; then \
	    if podman ps -a --format '{{.Names}}' | grep -q '^$(PG_CONTAINER_NAME)$$'; then \
	      podman start $(PG_CONTAINER_NAME) >/dev/null; \
	    else \
	      podman run -d --name $(PG_CONTAINER_NAME) -e POSTGRES_USER=aether -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=aether_dev -p 5432:5432 $(PG_IMAGE) >/dev/null; \
	    fi; \
	  else \
	    echo "[ensure-postgres] ERROR: docker or podman not found; please start Postgres manually."; exit 1; \
	  fi; \
	  echo "[ensure-postgres] Waiting for Postgres to become ready..."; \
	  for i in $$(seq 1 30); do \
	    if PGPASSWORD=postgres psql "$(DATABASE_URL)" -c 'SELECT 1' >/dev/null 2>&1; then echo "[ensure-postgres] Postgres is ready."; break; fi; \
	    sleep 1; \
	    if [ $$i -eq 30 ]; then echo "[ensure-postgres] Timed out waiting for Postgres."; exit 1; fi; \
	  done; \
	else \
	  echo "[ensure-postgres] Existing Postgres reachable."; \
	fi

test-full: ensure-postgres
	@echo "[test-full] Ensuring sqlx-cli installed..."; \
	if ! command -v $(SQLX) >/dev/null 2>&1; then cargo install sqlx-cli --no-default-features --features native-tls,postgres; fi; \
	echo "[test-full] Running migrations..."; \
	(cd crates/control-plane && DATABASE_URL=$(DATABASE_URL) $(SQLX) migrate run); \
	echo "[test-full] Running full workspace tests..."; \
	DATABASE_URL=$(DATABASE_URL) cargo test --workspace --all-features -- --nocapture

# Schema drift detection: regenerates sqlx-data.json and fails if it changes
schema-drift: ensure-postgres
	@echo "[schema-drift] Checking for schema drift against live DB..."; \
	if ! command -v $(SQLX) >/dev/null 2>&1; then cargo install sqlx-cli --no-default-features --features native-tls,postgres; fi; \
	tmp=$$(mktemp); \
	[ -f sqlx-data.json ] && cp sqlx-data.json $$tmp || true; \
	DATABASE_URL=$(DATABASE_URL) cargo sqlx prepare --workspace -- --all-targets >/dev/null 2>&1 || { echo "[schema-drift] prepare failed"; exit 1; }; \
	if [ -f $$tmp ]; then \
	  if diff -q $$tmp sqlx-data.json >/dev/null; then \
	    echo "[schema-drift] No drift detected."; \
	    rm -f $$tmp; \
	  else \
	    echo "[schema-drift] Drift detected. Updated sqlx-data.json differs from committed version."; \
	    echo "[schema-drift] Please review and commit the new sqlx-data.json."; \
	    exit 1; \
	  fi; \
	else \
	  echo "[schema-drift] Baseline sqlx-data.json missing; generated a new one. Commit it."; \
	  exit 1; \
	fi

crd:
	cargo run -p aether-operator --bin crd-gen > k8s/aetherapp-crd.yaml

# ------------------------
# Base image: aether-nodejs:20-slim
# ------------------------
REGISTRY ?= ghcr.io
IMAGE_NAME ?= aether-nodejs
IMAGE_TAG ?= 20-slim
# OWNER should be lowercased (GHCR requires lowercase org/user)
OWNER ?= $(shell echo "$${GITHUB_REPOSITORY_OWNER:-askernqk}" | tr 'A-Z' 'a-z')
IMG_DIR := images/aether-nodejs/20-slim
IMAGE := $(REGISTRY)/$(OWNER)/$(IMAGE_NAME):$(IMAGE_TAG)

base-image-build: ## Build the base image locally
	@echo "[base-image-build] Building $(IMAGE) from $(IMG_DIR)"; \
	docker build -t $(IMAGE) -f $(IMG_DIR)/Dockerfile $(IMG_DIR)

base-image-scan: ## Run local scans (Trivy/Grype) against the built image
	@echo "[base-image-scan] Scanning $(IMAGE)"; \
	if command -v trivy >/dev/null 2>&1; then \
	  trivy image --severity CRITICAL,HIGH --ignore-unfixed --exit-code 0 $(IMAGE); \
	else echo "[base-image-scan] trivy not found, skipping"; fi; \
	if command -v grype >/dev/null 2>&1; then \
	  grype $(IMAGE) || true; \
	else echo "[base-image-scan] grype not found, skipping"; fi

base-image-sbom: ## Generate SBOM (CycloneDX) if syft or docker sbom are available
	@echo "[base-image-sbom] Generating SBOM for $(IMAGE)"; \
	if command -v syft >/dev/null 2>&1; then \
	  syft $(IMAGE) -o cyclonedx-json > sbom-$(IMAGE_NAME)-$(IMAGE_TAG).cdx.json; \
	elif command -v docker >/dev/null 2>&1 && docker sbom --help >/dev/null 2>&1; then \
	  docker sbom --format cyclonedx-json $(IMAGE) > sbom-$(IMAGE_NAME)-$(IMAGE_TAG).cdx.json; \
	else \
	  echo "[base-image-sbom] syft or docker sbom not found; skipping"; \
	fi; \
	[ -f sbom-$(IMAGE_NAME)-$(IMAGE_TAG).cdx.json ] && echo "[base-image-sbom] SBOM: sbom-$(IMAGE_NAME)-$(IMAGE_TAG).cdx.json" || true

base-image-push: ## Push the base image to registry (requires login)
	@echo "[base-image-push] Pushing $(IMAGE)"; \
	docker push $(IMAGE)
