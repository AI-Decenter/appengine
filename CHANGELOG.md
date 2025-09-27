# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]
### Added
- OpenAPI specification generation (`/openapi.json`) and custom Swagger UI at `/swagger`.
- Prometheus metrics endpoint `/metrics` with HTTP request counter (labels: method, path, status).
- Service layer modules for applications & deployments.
- Request body size limit (1MB) and basic graceful shutdown (Ctrl+C drain 200ms).

### Changed
- `AppState.db` is now mandatory (removed `Option`).
- Refactored handlers to use service layer.

### Security
- Bumped `prometheus` to 0.14 which upgrades `protobuf` to 3.7.2 resolving RUSTSEC-2024-0437 (uncontrolled recursion / stack overflow vulnerability).

### Fixed
- Clippy warning (identity_op) in body limit configuration by extracting constant `MAX_BODY_BYTES`.

## [0.1.0] - Initial internal baseline
- Initial control-plane crate, basic CRUD for apps & deployments, error envelope, tracing, CI workflows.

