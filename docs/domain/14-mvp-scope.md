## Part 14 — Exact MVP Scope

Revised October 8, 2026 under [ADR-002](../decisions/0002-free-simple-launch.md). This part governs current delivery and supersedes original full-domain launch requirements.

### 14.1 Mandatory MVP

Validate whether local establishments publish genuine surplus and nearby people reserve and collect it. Platform use is free for consumers and establishments.

- Simple identified account, valid declared contact, secure session and supplier approval. Automatic email verification/campaigns are deferred; do not label contact verified without evidence.
- One establishment and public pickup point per supplier, including time zone and service window.
- Genuine surplus classification, simple categories, truthful quantity/condition, and food safety disclosures/hard limits when food is offered.
- Draft/immediate publication, fixed price or free goods, defined sellable unit, booking deadline, pickup interval and quantity cap.
- Nearby discovery, category/free filters, approximate distance and truthful availability.
- Confirmed free or direct-at-pickup reservation, exclusive quantity allocation, immutable relevant terms, consumer caps and cancellation.
- Verification, nominated collector, one final handover and explicit separable-unit partial outcome. Indivisible goods cannot be silently split.
- Booking/pickup expiry, stock release/disposition, supplier shortage, neutral incidents and fair attendance attribution.
- Basic reputation counts, completed-transaction reviews, reports and scoped operations actions.
- Transactional audit, replay safety, ownership checks, real PostgreSQL contention tests and rejection/rollback/boundary regressions.
- Simple actual offer/reservation/pickup metrics and an accessible minimal browser interface using backend contracts.
- One small-host backend/database/reverse-proxy deployment, private database network, backups and isolated restore rehearsal.

No monthly commercial publication allowance, Pro plan, subscription or platform commission at launch. Reasonable technical abuse controls protect the service without becoming hidden paid access.

### 14.2 Money and cancellation boundary

Offers may be free or have an accepted fixed goods price. For priced goods, participants arrange payment directly at pickup. Sua Sobra does not receive, hold, verify, distribute or return money and does not accept prepayment.

No Asaas adapter, Pix collection/instructions, checkout, payment attempt, financial obligation, payout, refund, chargeback or reconciliation module is implemented now. An online mode request is rejected explicitly. No provider credential/funded refund arrangement is a launch dependency.

Cancellation before handover releases/disposes of stock and records responsibility, with no platform financial action. Product mismatch, safety, supplier absence and conduct complaints remain supported. Any payment/return arrangement outside the service is handled directly between participants; do not display fabricated financial settlement.

The initial UI shows “free” or “pay directly at pickup,” accepted price, quantity, place and time. It has no payment-confirmation/refund-status flow.

### 14.3 Deferred evolution

After adoption and a new instruction: optional seller productivity/analytics features, consented leads and disclosed advertising. Keep initial access free for consumers and establishments; do not pre-build billing for unvalidated benefits.

Recurring offers, progressive pricing, follows/alerts, wanted-surplus requests, multiple points/representatives, detailed savings/impact, online financial services and mobile are deferred. The broader domain retains their concepts for later review; their presence is not a current implementation instruction.

### 14.4 Acceptance and integration gates

Require truthful publication, exactly one last-unit winner, protected reservation terms, one final pickup, deterministic stock disposition, fair attendance attribution, proper permission denial and reproducible local/browser/operational checks.

Validate/commit each microstep. Run the phase local gate, push its branch and verify its remote hash. No CI on phase pushes. Require green CI only when integrating into main through a PR, before merge. Provisioning and production deployment still require actual authorization and prerequisites.
