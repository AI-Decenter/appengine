# Contributing Guide

Thank you for your interest in improving AetherEngine. This document outlines the lightweight
process for contributing during the MVP phase. As the platform matures we will expand and
formalize governance and engineering policies.

## Principles

1. Security & Reproducibility First – Changes must build deterministically using the pinned
   Rust toolchain and produce no new critical `cargo-deny` violations.
2. Small, Reviewable Changes – Prefer a sequence of incremental pull requests over a large
   monolith.
3. Test What You Touch – For any user‑facing logic or new data model, add or extend tests.
4. Explicit Ownership – Each PR must list at least one reviewer from `@aether/platform-team`.

## Development Quick Start

```bash
./dev.sh bootstrap      # install toolchain & local services
./dev.sh verify         # quick smoke validation
make test               # run workspace tests
```

The control plane expects Postgres at the URL printed by `dev.sh bootstrap`. Override with:

```bash
export DATABASE_URL=postgres://aether:postgres@localhost:5432/aether_dev
```

## Branching & Commits

- Default branch: `main`
- Feature branches: `feat/<short-topic>` (e.g. `feat/deploy-endpoint`)
- Bugfix branches: `fix/<issue-or-symptom>`
- Commit messages: Conventional prefix recommended (e.g. `feat: add /deployments POST handler`).

## Pull Request Checklist

Before requesting review ensure:

- [ ] `cargo build --workspace --all-targets` succeeds
- [ ] `cargo test --workspace` is green
- [ ] `cargo fmt -- --check` has no diff
- [ ] `cargo clippy -- -D warnings` is clean (use `allow` attributes sparingly)
- [ ] `cargo deny check` passes
- [ ] Added / updated tests & docs for any new behavior
- [ ] Generated artifacts (e.g. CRD YAML) updated if schema changed (`make crd`)

## SQLx Offline Metadata

We use SQLx offline mode. When you add or modify queries:

```bash
docker run --rm -p 5433:5432 -e POSTGRES_PASSWORD=postgres postgres:15 &
# Wait a few seconds for readiness or use pg_isready
export DATABASE_URL=postgres://postgres:postgres@localhost:5433/postgres
cargo sqlx migrate run
cargo sqlx prepare --workspace --check -- --all-features
```

Commit the updated `sqlx-data.json` at workspace root if changed.

## CRD Updates

If you change the `AetherApp` Rust definition regenerate the CRD manifest:

```bash
make crd
```

Commit the resulting `k8s/aetherapp-crd.yaml`.

## Coding Conventions

- Use ` anyhow::Result<T>` for fallible public async functions in binaries.
- Map domain errors with `thiserror` enums where appropriate (avoid stringly typed errors).
- Keep modules focused; prefer a short file over a megafile.
- Avoid premature abstraction; duplicate once, abstract the third time.

## Observability

- Use structured logs: `info!(app_id=%id, "deployed")` style.
- Avoid logging secrets or personally identifiable information.

## License & Ownership

Source code is proprietary (see `LICENSE`). All contributors must have signed the internal
contributor agreement (handled out-of-band) before merge.

## Reporting Issues

File issues in the repository with a clear description, reproduction steps, and expected vs actual
behavior. Tag with `bug`, `enhancement`, or `question`.

## Roadmap Alignment

Large changes (schema, public API, multi-component refactors) require a short design proposal
(1–2 pages, problem statement, options, decision) added under `docs/` and linked in the PR.

---
Thank you for helping build AetherEngine!
