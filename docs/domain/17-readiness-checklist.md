## Part 17 — Final Readiness Checklist

Launch scope update (October 8, 2026): follow [ADR-002](../decisions/0002-free-simple-launch.md) and Part 14. Online money processing, returns, subscriptions and advanced capabilities below are future-only; launch uses free offers or a fixed goods price settled directly at pickup. Current service access is free for consumers and establishments.

Use this checklist to review the domain before implementation and again before launch. Checking an item means its business rule is agreed and its acceptance behavior is verifiable; it does not imply that software already exists.

### Product identity and actors

- [ ] Only genuine surplus is eligible, and ordinary secondhand/retail activity has a clear refusal policy.
- [ ] Consumer, supplier, establishment, collector, operations, platform, and financial-provider responsibilities are explicit.
- [ ] Supplier publication eligibility and online financial eligibility are distinct.
- [ ] Minimum publication information is sufficient without compulsory negotiation or unnecessary onboarding.
- [ ] Primary categories, sellable-unit definitions, formats, and condition declarations are coherent.
- [ ] Food, surprise bags, damaged packaging, and hard safety limits have truthful disclosure and refusal rules.

### Entitlements, quantity, and timing

- [ ] Admitted quantity always equals all six current dispositions; cancelled quantity is not double-counted.
- [ ] Last-unit contention has one accepted winner and no unallocated payment entitlement.
- [ ] Account caps include temporary commitments, and offer caps include previous completed units.
- [ ] Confirmation, booking deadline, payment deadline, pickup interval, grace, and safety limit are distinct.
- [ ] Exact boundary behavior and pickup-point time zones are defined.
- [ ] Protected reservation terms cannot be worsened retroactively.
- [ ] Price changes before acceptance and during a live payment commitment have determinate results.
- [ ] Progressive schedules, zero stages, schedule cancellation, and future alterations preserve prior commitments.
- [ ] Every cancellation/expiry has an explicit stock destination and at-most-once effect.
- [ ] Booking expiry preserves later confirmed pickup entitlements.
- [ ] Terminal offers do not reopen, and genuine republication cannot falsify physical condition/age.

### Pickup and money

- [ ] Entitlement verification is distinct from actual handover.
- [ ] Authorized third-party collection and one final handover are enforceable business rules.
- [ ] Indivisible units cannot be partially collected; separable-unit shortfall has explicit quantity and refund outcomes.
- [ ] Free, local-payment, and online modes each have an unambiguous obligation model.
- [ ] Online confirmation, supplier funds availability, fulfillment, and refunds are distinct facts.
- [ ] Late, duplicate, short, excessive, wrong-currency, and unmatched receipts have defined remedies.
- [ ] Cancellation before/after payment and supplier failure preserve full uncollected-value return under the MVP policy.
- [ ] Refund approval, execution failure, actual completion, and outstanding funding responsibility are distinguishable.
- [ ] Provider costs have a declared payer and cannot silently reduce a promised consumer return.
- [ ] Financial-provider method eligibility is verified before activation; custody/conditional-release capabilities are not assumed.
- [ ] Refund and external-dispute overlap cannot return the same amount twice.
- [ ] Completed pickup remains visible during later financial complaints and corrections.

### Trust, moderation, and commercial policy

- [ ] Unpaid expiry and supplier failure cannot be mislabeled consumer no-show.
- [ ] No-show attribution includes readiness checks, challenge time, neutral causes, and appeal corrections.
- [ ] Proportional restrictions have sample conditions, scopes, end dates, and first-use fairness.
- [ ] Reviews require completed participation and prohibit duplicates, self-dealing, coercion, and fabricated pickup.
- [ ] Safety containment, findings, sanctions, and appeals are distinct and reasoned.
- [ ] Antifraud responses rely on corroborated conduct rather than a household/location assumption.
- [ ] Consumer support and financial remedies survive restrictions or account closure.
- [ ] Publication allowance, period, consumption, credits, and resets are defined.
- [ ] Upgrade/downgrade/non-renewal preserve existing commitments and do not reset consumed allowance silently.
- [ ] Future subscriptions/fees have distinct obligations and disclosed prospective terms.

### Evidence, measurement, and growth

- [ ] History records action, actor, effective/recognized time, before/after position, reason, evidence, and policy version.
- [ ] Corrections reconcile quantity/money/reputation while retaining prior facts.
- [ ] Location privacy, evidence access, and retention exceptions have stated business rules.
- [ ] Metrics have defined cohorts, denominators, pending outcomes, and evidence labels.
- [ ] Actual reuse, declared measurements, estimated savings, and avoided-disposal inference are not conflated.
- [ ] Mandatory MVP functions are separated from immediate follow-up and future evolution.
- [ ] Recurrence requires actual occurrence confirmation; demand matching creates no reservation priority.
- [ ] All 60 edge cases can be answered using the stated policies without inventing a fundamental rule.
- [ ] Changes to policy defaults apply prospectively, preserving accepted commercial terms and auditable history.

### Decisions required for launch configuration

The business rules above are complete proposed defaults. Before launch, the business owner approves the operating area's named time zone and service calendar, supplier qualification criteria, actual free-plan allowance, enabled payment modes, current Asaas method conditions and named financial beneficiaries/funders, provider-cost agreement, and the operations contacts responsible for cases. These are bounded operational/commercial choices; they do not replace the domain's stock, fulfillment, or refund invariants.

For service targets expressed in business days, use the published operations calendar: Monday through Friday excluding its declared closure dates, with the day's service end stated in that calendar. Reservation, expiry, grace, appeal, and no-show intervals expressed in minutes/hours/days are elapsed-time intervals unless a specific calendar-period rule is stated. The platform must not silently reinterpret an elapsed deadline as a business-day deadline.

All baseline monetary prices and amounts are nonnegative values expressed to the currency's smallest unit; for BRL, this is one centavo. The accepted per-unit amount multiplied by whole sellable units determines the goods total. Estimated ordinary values are separately labeled and cannot replace the actual obligation. Any future fee rounding rule follows Part 13 and cannot alter that original goods price.

An accepted commitment keeps its policy version. Introducing a new category, payment method, delivery model, consumer fee, residence-based supplier, or institution-specific donation program requires an explicit domain review of eligibility, timing, quantity, responsibility, safety, privacy, and financial remedies before activation.
