# ADR-002 — Free, Simple Launch and Main Integration CI

Status: Accepted owner instruction. Date: October 8, 2026. Supersedes the initial full-domain delivery scope and phase-CI requirements of ADR-001.

## Current product scope

Launch a free service for consumers and establishments that connects people with genuine nearby surplus. Prioritize simple account creation, supplier approval, truthful publication, discovery, reservation, local pickup, cancellation, expiry, basic trust and moderation.

Platform access, publication and reservation have no subscription, commission, consumer charge or commercial monthly allowance in this launch. Supplier offers may be free or have a fixed goods price paid directly to the establishment at pickup. Free platform access does not imply every offered product must be free.

The platform accepts no online funds or prepayments. Do not implement Asaas, Pix collection, payment attempts, financial obligations, payout, escrow, refunds, chargebacks, subscription billing or financial reconciliation now. Payment arrangements for priced goods are directly between the participants at pickup; Sua Sobra records the accepted goods price and actual handover, without claiming to verify financial receipt or process returns. Product/attendance complaints remain supported.

Advanced seller features, consented leads, and disclosed advertising are possible future monetization. Pro plans, automatic progressive pricing, recurrence, demand matching, multiple establishments/representatives, opportunity-alert campaigns, advanced impact calculations and mobile are deferred until a new explicit instruction. Leads must not expose private contact data without consent; paid visibility must not bypass truthful stock or safety.

An identified consumer account needs a valid declared contact and accepted launch terms. Automatic email verification/recovery and email campaigns are deferred; do not label declared contact verified. Suppliers are qualified by operations. Standard secure authentication, CSRF, ownership checks, rate limits and confidential history remain required.

## Minimum infrastructure

The backend/database baseline is now Rust/Axum/SQLx and PostgreSQL 18.6 under [ADR-003](0003-rust-postgresql18.md), with the native TypeScript 7.0.2/HTML/CSS web defined by [the initial stack](../stack.md) deferred to its task. Deploy one backend, one database and a reverse proxy serving static web on one small host. No separate permanent worker, broker, Redis, paid observability service or financial-provider dependency. Correct booking/pickup deadlines are checked synchronously by commands; one repeatable backend maintenance command can run from a host timer for due expiry. No dedicated scheduling platform is needed.

Keep transactional stock conservation, accepted-term protection, one final pickup, structured errors, audit, command replay, real-database concurrency tests and security regression checks. Low cost does not justify overselling or unchecked permissions. Minimal reporting is actual offer/reservation/pickup counts and known goods values; no provider-confirmed revenue or invented savings/impact.

## Git and CI

Validate locally and commit each microstep. Push the phase branch to GitHub at its boundary and verify its exact remote hash. Phase branches do not trigger CI and do not wait for CI to advance.

CI runs only for pull requests targeting `main`, when integrating into that branch. Configure the workflow's `pull_request.branches` filter to `main`, a stable required check, and main branch protection/rules requiring that successful check before merge. Do not add an unrestricted push trigger or repeat the same suite on phase pushes or immediately after merging. Direct pushes to main are outside the normal development workflow.

Thus “CI only on main” means the main integration gate, not every feature/phase push. The check runs before merge so it can actually block a bad merge. If branch protection cannot be configured with available permission, record that limitation and do not claim it is enforced. PR creation or merge is performed when instructed; phase push alone does not authorize a merge.

## Authority and retained future rules

This launch decision and revised Part 14 govern present implementation. Other domain parts retain full future behavior, with financial and advanced rules explicitly deferred. Do not implement a future-only rule merely because it appears in the catalog. All stable IDs are preserved; acceptance mapping distinguishes active, mixed and deferred scope. A disabled online feature receives a rejection test, not a fake provider implementation.

The original 101-step full-domain plan is archived privately under `.local/plan/future-full-domain/`. The active private manifest contains only the simple launch steps. The generated foundation/supplier backend has been migrated to Rust; a main-targeting integration CI workflow exists. See ADR-003 and the active private manifest for actual implementation boundaries.
