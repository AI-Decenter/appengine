# Issue 10 — Auth & RBAC Foundation: TDD

## Contracts
- Inputs: HTTP requests with optional `Authorization: Bearer <token>`; env `AETHER_API_TOKENS=token:role[:name],...`; `AETHER_AUTH_REQUIRED=1|0`.
- Outputs: 401 (missing/invalid), 403 (valid but insufficient role), 2xx for allowed.
- Data: Stable `UserContext{user_id(uuid v5-like from sha256), role, name?, token_hash_hex}` via request extensions.

## Test Matrix
1) Unit — env parsing
- Valid CSV → HashMap hashed by sha256(token), role parsed, name optional.
- Invalid entries (empty token, bad role) are skipped.

2) Unit — constant-time compare
- Same bytes → true; different length → false; same length different last byte → false.

3) Integration — A1/A2/A3
- A1: No header on write route → 401 when required.
- A2: Reader token on GET → 200.
- A3: Reader token on POST /deployments → 403; Admin token → 201 for valid body.

4) Bypass
- With `AETHER_AUTH_REQUIRED=0`, all routes behave as before (no 401/403 enforcement).

5) Logging hygiene
- Never log token raw; log only hash prefix (6 chars). Not asserted in tests, but code guarded.

## Edge Cases
- Duplicate tokens with different roles → last wins.
- Very long token (>=4KB) → still hashed; compare by hash only.
- Header schema not `Bearer` → 401.

## Done Criteria
- Tests added: `tests/auth_rbac.rs` with A1–A3; unit helpers in auth.rs indirectly exercised.
- Migration present for `users` table (no mandatory seed).
- Router wired with auth and RBAC layers; order ensures 403 over 401 when token is valid but role insufficient.
