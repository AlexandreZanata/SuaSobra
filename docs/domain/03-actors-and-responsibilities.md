## Part 3 — Actors and Responsibilities

### 3.1 Responsibility matrix

Each entry explicitly assigns **authority**, **execution**, and **participation**. These assignments constitute the responsibility matrix; an unassigned action is not implicitly permitted.

- **Consumer account and interests:** authority and execution — consumer; participation — platform applies eligibility and privacy rules; operations resolves account disputes.
- **Supplier identity and establishment:** authority — supplier owner for declarations, operations for approval or restrictions; execution — authorized supplier representative; participation — platform validates completeness.
- **Offer creation and truthful condition:** authority and execution — supplier; participation — platform validates rules, operations moderates eligibility.
- **New reservation acceptance and quantity allocation:** authority and execution — platform under published rules; participation — consumer requests, supplier previously commits the offered stock.
- **Reservation cancellation:** authority and execution — consumer for their own unfulfilled reservation, supplier for their fulfillment failure, operations for justified intervention; participation — platform applies consequences.
- **Price and future quantity changes:** authority and execution — supplier within the alteration policy; participation — platform preserves existing terms and executes an accepted price schedule.
- **Financial confirmation:** authority — Asaas for externally received online funds; execution — platform recognizes substantiated results; participation — consumer pays, operations resolves discrepancies. A supplier declaration cannot confirm an online payment.
- **At-establishment payment:** authority and execution — supplier acknowledges receipt and consumer acknowledges payment; participation — operations resolves contested evidence. Asaas is not involved in this payment mode.
- **Pickup:** authority — consumer and supplier jointly acknowledge handover; execution — supplier hands over and platform recognizes one final outcome; participation — operations resolves exceptional verification.
- **Automatic expiry and release:** authority and execution — platform under the accepted policy; participation — operations handles exceptions without inventing retrospective entitlements.
- **Refund entitlement:** authority — the accepted refund policy, with operations deciding contested cases; execution — platform initiates and tracks, the responsible financial party funds, Asaas processes eligible online returns.
- **Reviews:** authority and execution — eligible participants; participation — platform enforces eligibility, operations moderates abuse.
- **Reports and sanctions:** authority — operations; execution — operations investigates and decides, platform applies restrictions; participation — any identified reporter and affected parties provide evidence.
- **Plans and commercial rules:** authority — operations; execution — platform applies the published period and benefits; participation — supplier chooses a plan.
- **Metrics and impact claims:** authority and execution — platform applies documented definitions; participation — supplier supplies declared measurements, operations audits misleading claims.

### 3.2 Permission boundaries

A supplier owner controls establishment permissions. A pickup representative may verify a reservation and acknowledge payment at the establishment but cannot change plans, refund online payments, or rewrite accepted terms without the relevant authority. Multiple representatives and multiple establishments are future capabilities; the owner performs these roles in the MVP.

A consumer can view and cancel only their own commitments and nominate a collector. A nominated collector can perform that pickup only; nomination confers no access to payment history or cancellation authority. A supplier cannot reserve their own offer or review their own transaction. Acting as a consumer at an unrelated supplier is allowed.

Operations separates ordinary moderation authority from financial-decision authority. Exceptional financial adjustments and reversal of completed pickup attribution require a second authorized reviewer. No actor can erase history, invent provider confirmation, or bypass a hard safety limit.

Account restrictions must be scoped: reservation restriction, publication restriction, online-payment restriction, or complete business suspension. Restrictions preserve access to refunds, support, evidence, and safe resolution of existing commitments.

