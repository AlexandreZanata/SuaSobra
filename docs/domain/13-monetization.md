## Part 13 — Monetization

Launch scope update (October 8, 2026): follow [ADR-002](../decisions/0002-free-simple-launch.md) and Part 14. Online money processing, returns, subscriptions and advanced capabilities below are future-only; launch uses free offers or a fixed goods price settled directly at pickup. Current service access is free for consumers and establishments.

### 13.1 Consumer and free supplier plan

Consumer discovery, reservation, and account use are free in the baseline. Consumers pay only an accepted nonzero goods price. Free goods remain fully free.

The free supplier plan includes one establishment/pickup point, basic offer publication, fixed prices, eligible formats, free offers, chosen permitted payment mode, cancellation/expiry, pickup confirmation, basic history, and basic results. The initial proposed allowance is 30 accepted publications/schedules per monthly period; operations may change future versions after growth evidence. No final plan price is prescribed.

A plan period starts at enrollment and ends on the same local calendar day/time next month, using the last day if that day does not exist. Periods are start-inclusive/end-exclusive, in the supplier's establishment time zone. Allowance resets at the next period start; unused capacity does not carry over. An enrollment-time policy fixes these boundaries.

Drafts consume no allowance. Accepting a schedule or publishing a draft consumes one; opening that schedule consumes no additional allowance. Editing, pausing/resuming, cancellation, expiry, or moderation removal does not restore allowance. Creating a new publication of genuine remaining surplus consumes another. A verified platform error may receive an explicit allowance credit with reason. No silent reset or refund occurs.

### 13.2 Future Pro plan

Potential benefits: higher/unlimited declared publication allowance, recurrence templates, progressive pricing, advanced statistics, followers and interest alerts, longer accessible business history subject to privacy policy, multiple establishments, and multiple authorized representatives. Every benefit has a named entitlement and effective interval.

Follows, interests, and basic accessibility do not need to become compulsory paid features. Actual Pro benefit selection is configurable. A paid plan must provide concrete disclosed benefits; having future domain concepts alone does not justify charging for absent functions.

### 13.3 Upgrade, downgrade, and cancellation

- **Upgrade:** supplier accepts disclosed price and benefits; activation occurs only after confirmed subscription payment. Launch rule for a later Pro release: charge the disclosed proportional price difference for the remaining current period, keep its original end, and apply the higher full-period allowance minus already consumed publications. No mid-period reset.
- **Downgrade:** schedule for next period. Current benefits remain until the current period ends. At the next period, publish only within the new allowance; existing publications and pickups remain valid.
- **Non-renewal:** retains benefits until paid period end, then becomes Free. No automatic renewal charge after effective cancellation.
- **Failed renewal:** once the new period starts without confirmed renewal, Free applies. Do not grant an unpaid new Pro period by assuming payment will arrive. Existing pickups and records remain accessible.
- **Over-limit after downgrade:** future publication is blocked until allowance resets or an upgrade activates; existing accepted publications are not cancelled. Remaining capacity is never reported below zero.
- **Recurring templates after benefit loss:** pause future occurrence creation; independently published occurrences continue.
- **Active progressive schedules after benefit loss:** already accepted schedules finish for those offers; suppliers cannot add new paid-feature schedules without entitlement.
- **Multiple locations after benefit loss:** preserve existing reservation points and history; owner selects the free-plan point for future publication. No relocation of accepted reservations.

Future subscription payment obligations are distinct from goods-payment obligations. They have their own beneficiary, period, billing terms, and cancellation policy; one subscription receipt cannot settle a consumer goods reservation.

### 13.4 Future commissions and other revenue

Possible later monetization includes a supplier commission on fulfilled paid units, Pro subscription, disclosed local promotion, or organization participation. Commission applies only to received and fulfilled goods value under the accepted fee version; a refund reverses its corresponding commission. Free goods have no transaction commission in the baseline.

Amounts, percentages, minimums, caps, billing periods, applicable payment modes, and tax-inclusive disclosure are business parameters. Use monetary amounts to the currency's smallest unit; round a fee once per transaction under a published rounding rule, with any final partial-refund rounding remainder returned on the final refund. Never invent extra quantity to compensate for a fee.

A later consumer fee requires an explicit new policy and acceptance before commitment; it cannot appear retrospectively. Sponsored placement must be disclosed and still meet eligibility, proximity, safety, and truthful-availability rules. Promotion does not buy exemption from moderation or stock fairness.

