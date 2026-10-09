# Verification Strategy Against Common AI Errors

The goal is demonstrated business correctness, not a claim of zero defects or a high coverage percentage alone. [Acceptance catalog](acceptance-catalog.json) initially marks all 147 domain IDs as planned. The Rust foundation/supplier tests now run; unimplemented business flows remain planned. Migration-specific tests and commands are described in the Rust migration runbook.

Current launch scope is ADR-002. Test only active behavior and the active portion of mixed definitions; deferred financial/advanced IDs remain explicitly deferred rather than requiring fake implementations. Keep the original full-domain strategy below as future guidance.

## Required test layers

- **Unit:** money, timestamp boundaries, enums, validation, deterministic price/quantity calculations, policy decisions. Inject a fixed clock; do not sleep to simulate expiry.
- **Service integration:** real PostgreSQL transactions, constraints, commands, audit/outbox records, rollback, permissions, replay and protected snapshots.
- **Concurrency:** independent database connections with synchronization barriers, real commit boundaries, simultaneous last-unit/cap/payment/pickup/cancellation transitions. Assert exactly one commercial winner and conserved persisted quantities. Ordinary test transaction wrappers must not hide commits.
- **HTTP contracts/security:** exact writable/public fields, status/error codes, actor scopes, CSRF, session expiry, mass assignment, cross-owner lookups, pagination and private evidence redaction.
- **Provider boundary:** sanitized authoritative schemas, timeout classification, authenticated event recognition, duplicate/reordered/late financial facts, refund amount limits, and reconciliation. Fake adapter behavior is independently checked against current official contracts.
- **Property/model checks:** generate valid and invalid command sequences against an independent small reference model; after each accepted/rejected step assert quantity conservation, protected terms, receipt uniqueness, and one final handover. Do not build the oracle by calling the implementation being tested.
- **End-to-end:** minimal browser with a real backend/database and test provider; roles, stale offer, reservation, payment, cancellation, pickup, and supplier failure. No browser-only fake success path.
- **Operational:** clean install, migration from empty and previous revision, restart durability, worker retry/crash recovery, backup restore to an isolated database, TLS/routes, sanitized logs, and rollback rehearsal.

## Error classes that must be caught

1. Hallucinated framework/provider methods: dependency locks, official-contract fixtures, import checks, and adapter contract assertions.
2. Wrong lifecycle or conflated refund/pickup state: all allowed transitions plus representative forbidden transitions and orthogonal-position assertions.
3. Missing ownership and object-level authorization: outsider/consumer/staff/supplier cases against every protected command and HTTP write.
4. Overselling and cap races: real PostgreSQL contention, independent consumers/offers, and persisted bucket assertions.
5. Float/rounding errors: integer cents, exact accepted totals, partial/full return reconciliation and boundary amounts.
6. Naive time or inclusive-end mistakes: timezone-aware clock, exact opening/deadline/stage tests, calendar-month edges and safety-over-grace cases.
7. Duplicate financial/event effects: command replay, receipt/event uniqueness, cancelled-then-late receipt, and outbox crash/retry tests.
8. Retrospective mutations: immutable snapshot assertions after price, location, quantity, plan, and schedule changes.
9. Lost audit or side effects before commit: intentional service rollback and worker retry; no provider action from uncommitted success.
10. A test passing without testing the rule: persisted result assertions, negative cases, minimum case manifests, skipped-test review, and scoped mutation evidence.
11. Mocks that hide broken integration: separate real-database, schema/contract, and sandbox-readiness evidence.
12. Secret leaks or permissive production configuration: environment separation, log/output redaction, deployment settings, and staged-file checks.
13. Fake coverage/phase completion: executable domain mapping, test node existence, real exit results, remote-tip/CI confirmation, and explicit blocked status.
14. Accidental scope growth: allowed-path diff check, task exclusions, decision log, and one-task commits.

## Reservation contention and rollback matrix (P03-06)

`backend/tests/p03_06.rs` exercises confirm/cancel/expiry races on real PostgreSQL 18.6 through independent connection pools to the same database: last-unit races across connections, reserve-vs-cancel and cancel-vs-expiry agreement, three-way cap contention, identical-key races, and a mixed-lifecycle reconciliation asserting that persisted units, reservation states, immutable snapshots and audit events agree. Batch-level rollback after bucket mutation is covered by the corrupted-row case in `p03_05.rs`; per-command rejections across `p03_01`–`p03_05` assert zero side effects (no rows, no audit, conserved buckets), which is the rollback leg for commands whose fallible checks all precede the first write.

## Release requirements

All active EDGE scenarios and active portions of mixed cases require substantive assertions. Active INV and UC items require mapped tests and explicit operational checks. The full 147-ID catalog remains available, but deferred items do not block launch and cannot be falsely marked verified. The catalog distinguishes implemented from verified and reports pending operational readiness rather than declaring universal coverage.

For critical quantity, reservation, pickup, financial, and permission modules, require at least 90% branch coverage after the relevant suite exists; other backend application modules target 80% branch coverage. Coverage is a secondary gap detector. Collect on application code only, excluding generated migrations/fixtures and third-party code; any exception needs a reasoned record. Do not raise a percentage by excluding a failing module.

Run scoped mutation checks on conservation, accepted price, deadline precedence, one-pickup enforcement, late-receipt non-revival, and refund bounds. Selected mutations must be killed by substantive tests. Surviving mutations are reviewed and fixed or explicitly justified; no numeric mutation score substitutes for these critical checks. P11 verifies a compatible mutation tool before adding it, rather than inventing a command.

Required tests may not be deleted, skipped, xfailed, or weakened to make CI green without an approved rule/test correction. An environment-caused skip is blocked evidence. Retries may diagnose a flaky test, but its root cause must be resolved before the phase gate.

For transaction-sensitive PostgreSQL tests, use independent SQLx connections and real commits in isolated PostgreSQL 18.6 databases. The Rust test harness requires an explicit `_test` administration database, creates uniquely named test databases and removes only those it owns. Missing PostgreSQL or suites fails the gate.

The active private launch phases establish local PostgreSQL verification, backend acceptance, browser checks and an operational rehearsal. CI runs only for PRs targeting main; phase branch pushes do not trigger it. The old full-domain phase numbers are archived. A private plan is not required in CI: committed tests, fixtures, mappings, and scripts contain all reproducible inputs.
