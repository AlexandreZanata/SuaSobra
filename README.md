# Sua Sobra

Sua Sobra is a hyperlocal marketplace for genuine surplus, local pickup, donations, and reuse.

## Initial stack

**Rust 1.99.0 stable** (production release builds), **PostgreSQL 18.6** (native UUIDv7), **TypeScript 7.0.2**, native **HTML** and **CSS**. The frontend uses no frameworks or third-party application dependencies. See [the canonical stack](docs/stack.md).

## Project documentation

Start at the [documentation entry point](docs/README.md). The [domain index](docs/domain/README.md) routes to all 17 canonical business parts, including 21 use cases, 66 invariants, and 60 edge cases.

All maintained documentation is in English. Business rules are separate from the [minimal architecture](docs/engineering/architecture.md), [agent workflow](docs/engineering/agent-workflow.md), and [verification strategy](docs/quality/test-strategy.md). The generated foundation and supplier backend now run in Rust (Axum/Tokio/SQLx), with PostgreSQL 18.6 generating persistent UUIDv7 identifiers. Offers, reservations and minimal web remain subsequent tasks.

## Private implementation plan

The complete owner-specific plan is in `.local/plan/README.md`. It contains small ordered tasks for a free, simple marketplace backend, low-cost infrastructure, and a minimal web test interface. Mobile development is deferred.

`.local/` is ignored by Git and must not be pushed. Application code belongs in versioned `backend/`, `web/` and `infra/`; repository tooling belongs in `tools/` and `.github/`. GitHub checkouts do not include the private plan; the owner must supply the assigned task or a separate private copy.

Agents begin with `AGENTS.md`. Check documentation with `make docs`.

Current scope follows [ADR-002](docs/decisions/0002-free-simple-launch.md): no online payments, Pix collection or refund processing. Seller tools, consented leads and advertising are future monetization. CI runs only at integration into `main`; phase work is checked locally and pushed normally.

Run the backend with [the local development instructions](docs/engineering/local-development.md). `make verify` runs documentation, formatting, Clippy and real PostgreSQL tests; `make build` creates the release executable. Existing data adoption follows [the Rust migration runbook](docs/engineering/rust-migration.md).
