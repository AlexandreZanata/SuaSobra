# Minimal Launch Architecture

Read [ADR-002](../decisions/0002-free-simple-launch.md) for launch scope and [ADR-003](../decisions/0003-rust-postgresql18.md) for the accepted Rust/PostgreSQL migration. Full financial designs in other domain parts remain future-only.

## Baseline and navigation

Rust 1.99.0, Axum 0.8.9, Tokio 1.53.2, SQLx 0.8.6, PostgreSQL 18.6. Direct dependency pins and the full Cargo lock are committed. The minimal future web uses TypeScript 7.0.2, native HTML and native CSS with zero third-party application dependencies. [The canonical stack](../stack.md) governs both sides; no mobile application is created.

- `backend/src/main.rs`: executable commands and server lifecycle.
- `backend/src/lib.rs`: shared state, bounded pool, version check and embedded migrations.
- `backend/src/config.rs`: explicit runtime settings and production host policy.
- `backend/src/http.rs`: `/api/v1/` routing, CSRF/origin checks, strict JSON transport, errors and correlation headers.
- `backend/src/identity.rs`: accounts, password compatibility, opaque sessions, scopes, database rate limits and retained token primitives.
- `backend/src/primitives.rs`: checked integer money/units, UTC intervals, versioned launch policies and transaction-local lock order.
- `backend/src/suppliers.rs`: transactional lifecycle/site/location commands and access-checked queries.
- `backend/src/audit.rs`, `replay.rs`: immutable redacted history, durable follow-up primitives and transaction-scoped replay.
- `backend/src/legacy.rs`: deliberate, all-or-nothing legacy adoption and ID mapping.
- `backend/migrations/`: SQLx SQL migrations; application IDs default to native `uuidv7()`.
- `backend/tests/`: unit, integration, contract/security and concurrency suites with isolated real PostgreSQL databases.
- `backend/openapi.json`: versioned API schema, served by `/api/v1/schema/`.
- `infra/`: local PostgreSQL service and example environment. `backend/Dockerfile` builds the non-root backend image.
- `tools/repo-check/`: native Rust documentation and private-plan validation, with its own committed Cargo lock.
- `docs/`: canonical business rules, engineering contracts and decisions. `.local/` is private plan/evidence only.

Create future marketplace/reservation/fulfillment/trust/reporting modules only in their assigned tasks. No payment module, permanent worker, Redis, broker or microservice. Current operations actions use moderator-protected HTTP commands through the same services. A future minimal web can expose those actions.

## Correctness boundary

All supplier writes call `suppliers::execute`. Begin a PostgreSQL transaction, reload/lock the actor, lock supplier authority before point state, check current permissions, acquire scoped replay identity, validate and persist state/history/audit/result atomically. Errors roll everything back. Peer locks use deterministic ordering; async task state must never use thread-local lock tracking.

Money is nonnegative integer BRL cents; sellable quantities are whole units. Use checked arithmetic, UTC timestamps and IANA location zones. Location coordinates are parsed as decimal strings with up to six fractional digits and rounded using decimal half-even rules for public discovery. They are not money calculations. Typed commands reject unknown writable fields and client state overrides.

Every persistent application ID is a UUID returned from the database. References grant no authority. UUID timestamps are not an authorization mechanism or the authoritative business clock.

The remaining reservation implementation must preserve conservation, accepted terms, account-wide caps, exclusive deadlines, one final handover and replay through PostgreSQL transactions/constraints. Free/direct-at-pickup reservations will confirm immediately, without financial obligations or provider receipt claims. These future flows are not implemented by the backend language migration.

## Authentication and cost

Same-origin opaque session cookies are HttpOnly/SameSite=Lax and Secure in production. Random session/CSRF secrets are stored only as SHA-256 hashes. Login/registration require CSRF even anonymously; login rotates both secrets and logout invalidates the session. Origin/Host checks reject cross-site writes. Forwarded IP headers are ignored; the direct peer is authoritative unless a future reviewed proxy trust policy is added.

Argon2id runs in blocking tasks with at most four concurrent password operations per process; saturation returns 429. The pool defaults to ten connections with a five-second acquisition deadline. PostgreSQL holds shared login counters and transaction advisory locks across processes. A request is limited to 64 KiB. These bounds limit resource use; they do not prove capacity for thousands of simultaneous users.

Serve one executable, one database and future static web behind the same-origin reverse proxy. Run `prune-auth` from a host timer to bound expired authentication state. It creates no permanent worker. The outbox is a retained transactional primitive; no launch sender/worker or provider integration is enabled. No paid service or production host is provisioned by this change.
