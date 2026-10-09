# Agent Instructions

These instructions apply to the entire repository. Maintained documents, code identifiers, comments, and commit descriptions use English.

## Start here

1. Read `docs/decisions/0002-free-simple-launch.md`, `docs/decisions/0003-rust-postgresql18.md`, `docs/stack.md`, `docs/README.md`, the workflow, and the assigned task.
2. Read only referenced domain sections; locate UC/INV/EDGE definitions through `docs/domain/catalog.json`.
3. Current launch scope follows ADR-002 and Part 14; future-only rules stay deferred. Business definitions live in `docs/domain/`. Engineering choices live in `docs/engineering/` and `docs/decisions/`.
4. Private tasks and progress live in `.local/plan/` and `.local/progress/`; `.local/` must never be committed.
5. If the private task is unavailable, request its text; do not invent scope or mark it done.

## Work boundaries

- Execute one assigned microstep at a time; respect its prerequisites and allowed paths.
- Do not implement adjacent features, change the stack, rename domain terms, or weaken acceptance criteria.
- Never change business rules to make a test pass. Record conflicts and stop dependent work.
- Follow `docs/stack.md`: Rust 1.99.0 stable with release production builds; PostgreSQL 18.6; frontend TypeScript 7.0.2, native HTML/CSS and zero third-party frontend dependencies. No frontend frameworks, bundlers or CSS libraries.
- Use the pinned Rust/Axum/SQLx backend and PostgreSQL 18.6. Persistent application IDs come from native database `uuidv7()` defaults; never replace them with numeric IDs or application UUID generation.
- Keep backend rules in explicit transactional command services; every entry point must use them.
- Keep launch free for people and establishments; no online payment, Pix collection, refunds, subscription billing, worker/broker or mobile app.
- Preserve unrelated changes. Do not reset, force-push, or stage files indiscriminately.
- Never put credentials, production data, or secrets in fixtures, commits, logs, or prompts.

## Validation and delivery

- Add meaningful success, rejection, permission, and regression checks for the assigned behavior.
- Verify persisted effects and events, not only return values or mocks.
- Use real PostgreSQL for persistence and contention tests; SQLite does not prove these guarantees.
- Run required checks. Missing tools, skipped required tests, and unavailable services are blockers, not passes.
- Record actual commands, exit results, counts, and limitations in private evidence; never fabricate a run.
- Update versioned acceptance traceability and documentation when behavior changes.
- For an authorized implementation task, commit validated changes with its task ID. Exclude `.local/`.
- At a phase boundary, run local gates, push the phase branch, and verify its exact remote commit; no phase CI.
- CI is required only for integration PRs targeting `main`; merge only after its required check passes.
- Deployment, spending, production changes, and live financial actions require task/runbook authorization.

## Maintenance

- Preserve stable UC/INV/EDGE IDs and one canonical definition of every rule.
- Run `make docs` after edits; use `make docs-refresh` when definitions move.
- Correct canonical files and links rather than creating competing business summaries.
- Leave a precise next action and reproducible failure evidence for the next agent.
