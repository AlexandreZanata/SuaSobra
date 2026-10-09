# Documentation Entry Point

This repository is designed for AI creation and maintenance. Read a small explicit context packet for each assigned task instead of loading every document.

Read [the current launch decision](decisions/0002-free-simple-launch.md) and [the Rust/PostgreSQL decision](decisions/0003-rust-postgresql18.md) first. The initial release is free, uses direct local pickup, and has no platform payment/refund processing. Advanced rules are retained for future use.

## Authority and navigation

1. [Initial stack](stack.md) specifies pinned Rust/PostgreSQL/TypeScript and dependency-free native HTML/CSS.
2. [Root instructions](../AGENTS.md) define the repository-wide working contract.
3. [Domain index](domain/README.md) routes to 17 canonical business parts retaining the complete original text.
4. [Domain catalog](domain/catalog.json) maps 21 use cases, 66 invariants, and 60 edge cases to their canonical definition files and lines.
5. [Architecture](engineering/architecture.md) fixes the proposed minimal baseline.
6. [Contracts](engineering/contracts.md) define behavior shared by all entry points.
7. [Agent workflow](engineering/agent-workflow.md) defines task execution, evidence, commits, phase pushes, and handoffs.
8. [Test strategy](quality/test-strategy.md) protects business rules against common AI errors.
9. [Acceptance catalog](quality/acceptance-catalog.json) records coverage status for every domain ID. Unimplemented items remain pending.
10. [Decision log](decisions/README.md) records justified changes separately from business rules.

The implemented backend is Rust. Use [local development](engineering/local-development.md) to run it and [the migration runbook](engineering/rust-migration.md) to adopt existing Django data.

## Finding and editing a rule

Search stable identifiers or business terms with `rg`. Use the catalog to find definitions rather than every reference. For quantity, read Parts 5, 6, and 8. For payment/cancellation, read Part 11 and relevant use cases. For trust, read Part 12. For scope, read Parts 14–15. For acceptance scenarios, read Part 16.

Edit the canonical defining file; preserve IDs and vocabulary. A policy change needs rationale, affected IDs, prospective effects, and updated scenarios. Engineering documents reference rules rather than redefining them. Architecture cannot override accepted price, quantity, refund, pickup, or audit invariants.

Refresh moved definition lines with `make docs-refresh`, then run the check without that option. The original-source digest is provenance, not a prohibition against reviewed future edits. Original content was verified as losslessly reconstructed during reorganization.

## Private execution materials

`.local/plan/README.md` contains task routing; `.local/progress/` contains status; `.local/evidence/` contains actual local evidence. These paths intentionally do not appear on GitHub. Versioned docs retain architecture, behavior, testing contracts, and acceptance mapping for maintenance.
