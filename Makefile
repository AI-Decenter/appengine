SHELL := /bin/bash
ROOT := $(shell pwd)

RUST_TOOLCHAIN := 1.90.0
SQLX_FEATURES := postgres
DATABASE_URL ?= postgres://aether:postgres@localhost:5432/aether_dev
PG_CONTAINER_NAME ?= aether-pg-test
PG_IMAGE ?= postgres:15
SQLX ?= sqlx

.PHONY: all build fmt lint test clean sqlx-prepare crd

all: build

build:
	cargo build --workspace --all-targets

fmt:
	cargo fmt --all

lint:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
	cargo test --workspace --all-features -- --nocapture

clean:
	cargo clean

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
