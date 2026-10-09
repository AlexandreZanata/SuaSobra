# Business Domain Index

Version 1.0, originally specified on October 8, 2026. These 17 canonical files were split without altering or omitting the original business text. Engineering is documented separately because the subsequent request explicitly includes backend, infrastructure, and minimal web planning.

Current implementation follows [ADR-002](../decisions/0002-free-simple-launch.md) and revised Part 14. Financial/advanced concepts in other parts are future-only. Stable IDs are retained, with launch scope classified in the acceptance catalog.

1. [Executive summary](01-executive-summary.md) — purpose, defaults, and ambiguities.
2. [Glossary](02-glossary.md) — stable terms.
3. [Actors and responsibilities](03-actors-and-responsibilities.md) — authority and permissions.
4. [Business entities](04-business-entities.md) — information and relationships.
5. [Consistency boundaries](05-consistency-boundaries.md) — stock and commercial consistency.
6. [State machines](06-state-machines.md) — allowed transitions and independent financial positions.
7. [Use cases](07-use-cases.md) — UC-001 through UC-021.
8. [Invariants](08-invariants.md) — INV-001 through INV-066.
9. [Domain events](09-domain-events.md) — triggers, meanings, and effects.
10. [Automatic policies](10-automatic-policies.md) — eligibility, food, pricing, timing, alterations, alerts.
11. [Payments and cancellations](11-payments-and-cancellations.md) — Asaas and cancellation matrix.
12. [Trust, moderation, audit](12-trust-moderation-and-audit.md) — reputation, no-show, cases, history.
13. [Monetization](13-monetization.md) — allowances and plans.
14. [MVP scope](14-mvp-scope.md) — launch and deferred capabilities.
15. [Roadmap and metrics](15-roadmap-and-metrics.md) — phases and evidence definitions.
16. [Edge cases](16-edge-cases.md) — EDGE-001 through EDGE-060.
17. [Readiness checklist](17-readiness-checklist.md) — review and launch configuration.

[catalog.json](catalog.json) contains provenance, the original metadata preamble, and definition locations. This index routes to rules; it does not supersede them.
