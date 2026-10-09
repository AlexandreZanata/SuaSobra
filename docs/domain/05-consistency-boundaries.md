## Part 5 — Aggregates and Business Consistency Boundaries

### 5.1 Offer and quantity consistency

The offer's eligibility, lifecycle, booking interval, current price, supplier authorization, and quantity position must agree when accepting a commitment. The accepted commitment both allocates quantity and fixes its terms. Either both facts exist or neither exists as a business result. Two simultaneous requests for the last unit cannot both succeed.

The conservation identity is:

**Admitted quantity = available quantity + temporary commitments + confirmed reserved quantity + handed-over quantity + withdrawn quantity + expired quantity.**

All quantities are whole numbers of the offer's sellable unit. Adding genuine stock increases admitted and available quantity by the same amount. Reducing current supply moves only available quantity into withdrawn quantity; admitted quantity remains a cumulative audit total. Correcting a declaration uses an explicit adjustment, never an unexplained disappearing quantity.

The same physical goods cannot be advertised or committed simultaneously through overlapping offers. Repackaging a remainder into a new lot requires ending its old uncommitted availability first, without moving already committed goods. Goods already handed over are outside supplier availability unless a real return and a fresh safety/eligibility assessment establish new supply.

Cancelled reservation quantity is a historical movement, not another current stock bucket. Otherwise the same unit would be counted both as cancelled and available. The history separately reports cancelled commitments and their destinations. An offer with five admitted units and two consumer cancellations still has five admitted units, not seven.

Available quantity may remain physically allocatable while an offer is paused, but **discoverable availability is zero**. At terminal closure, remaining available quantity moves to withdrawn or expired quantity. Temporary or confirmed commitments are disposed of according to their own outcomes.

### 5.2 Reservation consistency

Consumer eligibility, account-wide limits, offer-specific cumulative cap, quantity entitlement, protected terms, and payment deadline must agree at acceptance. A payment-pending reservation counts against consumer limits. Ineligible or rejected requests have no stock allocation.

Temporary commitments become confirmed reserved quantity only with valid financial confirmation. Free and pay-at-establishment reservations become confirmed directly. A cancellation, payment expiry, or terminal pickup outcome releases or disposes of its remaining allocation once. Retrying a failed payment never extends a commitment.

### 5.3 Financial consistency

One obligation belongs to one reservation. At most the agreed total can satisfy it. Excess or duplicate receipts remain separately refundable. A full refund is complete only when all refundable received funds are confirmed returned; approved outstanding money remains a reimbursement obligation.

A provider-confirmed payment received after the commitment has ended never revives it, even if the provider reports an earlier payment time. This conservative rule prevents taking units already reallocated. The late receipt is linked to the ended transaction and refunded. A new reservation requires a fresh acceptance at current terms.

### 5.4 Pickup consistency

Reservation eligibility, payment sufficiency, authorized collector, actual handover, quantity movement, and final pickup outcome must agree. Exactly one final handover consumes the reservation. Separable-unit partial fulfillment records handed-over quantity and explicitly cancels the remainder in that same final outcome. The remainder is released only if resale is allowed; any online overpayment becomes a refund obligation.

### 5.5 Safety and moderation consistency

Safety containment stops new reservations immediately and classifies affected commitments. A dangerous item cannot remain eligible for pickup because it was paid. Removal records quantity withdrawal, reservation cancellation, and refund entitlement; financial execution may complete later. A hidden publication alone is not adequate cancellation of its commitments.

### 5.6 Plan and reputation consistency

Publication checks and allowance consumption form one commercial action. Review eligibility follows the recorded handover. Resolved outcome attribution and reputation consequences agree; reversing attribution reverses the consequence without deleting the original finding.

Cross-boundary actions preserve both their commercial outcome and any unresolved follow-up. For example, cancelling a paid reservation stops pickup entitlement immediately while its refund remains outstanding. An unfinished financial action is visible and assigned, rather than disguised as a completed business operation.

