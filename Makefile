SHELL := /bin/bash
ROOT := $(shell pwd)

RUST_TOOLCHAIN := 1.90.0
SQLX_FEATURES := postgres
DATABASE_URL ?= postgres://aether:postgres@localhost:5432/aether_dev

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

crd:
	cargo run -p aether-operator --bin crd-gen > k8s/aetherapp-crd.yaml
