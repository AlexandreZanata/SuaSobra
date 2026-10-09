## Part 7 — Use Cases

Launch scope update (October 8, 2026): follow [ADR-002](../decisions/0002-free-simple-launch.md) and Part 14. Online money processing, returns, subscriptions and advanced capabilities below are future-only; launch uses free offers or a fixed goods price settled directly at pickup. Current service access is free for consumers and establishments.

Each use case lists its actor, preconditions, command, validations, result, domain events, and failure behavior. Events are defined in Part 9. A failed action produces a reason and preserves any existing entitlement; it does not create fictional success.

### UC-001 — Register and qualify a supplier

1. **Actor:** supplier owner; operations approves.
2. **Preconditions:** identified owner authorized to represent an eligible supplier.
3. **Command:** declare supplier and establishment, request approval.
4. **Validations:** identity, authority, genuine business activity, contact, pickup location, time zone, and policy acceptance.
5. **Result:** Pending Approval, then Active or Rejected; online financial eligibility remains separate.
6. **Events:** SupplierRegistered, SupplierApproved or SupplierRejected.
7. **Failures:** explain missing information or ineligible activity; no publishing rights until approved.

### UC-002 — Create an offer

1. **Actor:** authorized supplier.
2. **Preconditions:** establishment exists; supplier has permission to prepare offers.
3. **Command:** declare surplus and save a draft.
4. **Validations:** draft values must not assert impossible negative price/quantity; missing publication information is identified.
5. **Result:** Draft with no consumer entitlement and no allowance consumption.
6. **Events:** OfferCreated, OfferChanged when a saved declaration changes.
7. **Failures:** reject invalid values; incomplete drafts may remain unpublished.

### UC-003 — Publish or schedule an offer

1. **Actor:** supplier; platform opens an accepted schedule.
2. **Preconditions:** complete eligible draft, Active or appropriately Warned supplier, publication allowance available.
3. **Command:** publish now or accept a future booking interval.
4. **Validations:** genuine surplus reason, sellable unit, positive admitted quantity, condition, category disclosures, valid price/mode, pickup point, booking before pickup end, safety limit, and usable intervals.
5. **Result:** Published or Scheduled and one allowance consumed. Scheduled offers cannot be reserved before opening.
6. **Events:** OfferScheduled or OfferPublished, PublicationAllowanceConsumed.
7. **Failures:** no partial publication or allowance consumption if acceptance fails; a later safety or supplier restriction prevents scheduled opening.

### UC-004 — Discover relevant offers

1. **Actor:** consumer or visitor.
2. **Preconditions:** discovery area or approximate location supplied.
3. **Command:** find surplus within category, radius, price, and free filters.
4. **Validations:** booking currently open, eligible supplier and offer, positive availability, permitted public-location detail.
5. **Result:** relevant opportunities with current terms, approximate distance, and urgency. Distance is an estimate, not a route or arrival guarantee.
6. **Events:** none affecting stock; a discovery observation may support conversion measurement.
7. **Failures:** no location means request an area; stale displayed availability creates no reservation right.

### UC-005 — Reserve quantity

1. **Actor:** consumer.
2. **Preconditions:** eligible account and currently bookable offer.
3. **Command:** accept current terms for a requested number of sellable units.
4. **Validations:** quantity, offer and consumer caps, safety, supplier standing, time, actual availability, current price, and adequate payment or free-pickup interval.
5. **Result:** Awaiting Payment with temporary allocation, or Confirmed for free/establishment mode. Protected terms and one transaction are established.
6. **Events:** ReservationCreated, QuantityCommitted, ReservationConfirmed when immediately valid, OfferSoldOut if last units allocated.
7. **Failures:** reject unavailable units or changed terms; present the new price for acceptance rather than silently charging it. No provisional entitlement survives a rejected request.

### UC-006 — Initiate or retry online payment

1. **Actor:** consumer.
2. **Preconditions:** Awaiting Payment, live original deadline, Asaas financial eligibility enabled.
3. **Command:** attempt to satisfy the accepted payment obligation.
4. **Validations:** exact amount/currency, permitted method, one live attempt, financial account readiness, and no terminal cancellation.
5. **Result:** Pending attempt linked to the same obligation; retry does not change terms or deadline.
6. **Events:** PaymentCreated on first obligation recognition, PaymentPending, PaymentAttemptFailed if rejected.
7. **Failures:** preserve a live commitment only until its original deadline; definitive failure ends it. Never instruct the consumer to pay the supplier privately for an online obligation.

### UC-007 — Recognize online payment

1. **Actor:** platform based on substantiated Asaas financial outcome.
2. **Preconditions:** receipt belongs to an identified obligation.
3. **Command:** recognize received funds and assess fulfillment eligibility.
4. **Validations:** amount/currency, source obligation, live commitment, confirmation accepted before deadline, duplicates, and prior return/dispute activity.
5. **Result:** exact timely receipt confirms reservation; insufficient receipt does not confirm; excess or late receipt creates a refund obligation.
6. **Events:** PaymentConfirmed and ReservationConfirmed for valid receipt; LatePaymentRecognized or ExcessPaymentRecognized and RefundRequested otherwise.
7. **Failures:** unmatched receipts enter a financial case; screenshots do not grant pickup; never take quantity from another accepted reservation.

### UC-008 — Cancel a consumer reservation

1. **Actor:** reservation owner.
2. **Preconditions:** no completed handover; live commitment or entitlement.
3. **Command:** cancel with a reason.
4. **Validations:** ownership, current state, any concurrent completed handover, previously returned money.
5. **Result:** Cancelled by Consumer; remaining allocation moves once to available, expired, or withdrawn; paid uncollected quantity is fully refundable.
6. **Events:** ReservationCancelled, QuantityReleased or QuantityExpired or QuantityWithdrawn, RefundRequested where required.
7. **Failures:** completed pickup cannot be cancelled; open a complaint instead. An already cancelled reservation does not release twice. Repeated late cancellations can trigger abuse review, not automatic no-show attribution.

### UC-009 — Cancel an offer or fail supplier fulfillment

1. **Actor:** supplier; operations for protective intervention.
2. **Preconditions:** supplier controls the offer or operations has case authority.
3. **Command:** cancel offer, or declare inability to fulfill named reservations.
4. **Validations:** reason, affected quantity/commitments, safety, payment/refund positions, whether any handover already occurred.
5. **Result:** new reservations stop; unfulfilled affected reservations cancel with supplier or platform attribution; unsafe units withdraw; full refunds for uncollected paid units.
6. **Events:** OfferCancelled or OfferRemoved, ReservationCancelled, SupplierFailureRecorded when resolved, RefundRequested.
7. **Failures:** completed handovers remain completed; a later safety incident receives separate remedies. Cancelling an offer cannot simply hide outstanding commitments.

### UC-010 — Verify and complete pickup

1. **Actor:** authorized supplier representative and consumer or nominated collector.
2. **Preconditions:** Ready for Pickup, unused valid entitlement, accepted point and interval.
3. **Command:** verify reservation, acknowledge actual handover once.
4. **Validations:** collector authority, confirmation reference, actual condition/contents, quantity no greater than reserved, hard safety limit, required online or establishment payment satisfaction.
5. **Result:** Pickup Completed and reservation Picked Up; actual quantity moves to handed-over. Separable-unit partial outcome cancels remainder and establishes refund for uncollected paid units.
6. **Events:** PickupVerified, EstablishmentPaymentAcknowledged where applicable, PickupConfirmed, PartialFulfillmentRecorded when applicable, ReviewEligibilityGranted.
7. **Failures:** refuse invalid reference, unsafe goods, unmatched collector, or unpaid online entitlement. Verification without actual handover consumes no quantity. No second handover under the same reservation.

### UC-011 — End a missed pickup and attribute responsibility

1. **Actor:** platform identifies missed deadline; parties provide evidence; operations resolves contested attribution.
2. **Preconditions:** confirmed entitlement, effective pickup deadline ended, no recorded handover.
3. **Command:** end entitlement and evaluate non-collection.
4. **Validations:** supplier readiness, closure, consumer attendance evidence, cancellation notices, force majeure, grace/safety cutoff, and pending handover discrepancy.
5. **Result:** Outcome Under Review, stock disposition, and refund entitlement immediately; then No-Show or Failed Pickup. Default no-show is proposed only if supplier readiness is declared, no competing incident exists, and the consumer has had the 24-hour challenge opportunity. Otherwise neutral or contested cases require review.
6. **Events:** PickupMissed, ReservationEnded, QuantityReleased/Expired/Withdrawn, RefundRequested, OutcomeAttributed, NoShowRecorded or SupplierFailureRecorded.
7. **Failures:** payment-pending expiry is never no-show; missing proof of supplier readiness cannot justify automatic consumer punishment.

### UC-012 — Expire an offer or payment commitment

1. **Actor:** platform.
2. **Preconditions:** relevant deadline reached.
3. **Command:** close the named interval.
4. **Validations:** current state, accepted timestamps, confirmed entitlements, prior dispositions.
5. **Result:** offer Expired and uncommitted stock expired; payment commitments expire and dispose of quantity once; later confirmed pickups remain protected.
6. **Events:** OfferExpired, ReservationExpired, PaymentExpired, QuantityExpired or QuantityReleased.
7. **Failures:** expiry must not erase pickup or financial evidence; a paid confirmed reservation needs its own pickup or cancellation outcome.

### UC-013 — Alter future offer terms or quantity

1. **Actor:** authorized supplier.
2. **Preconditions:** non-terminal offer; declared change is allowed under Part 10.
3. **Command:** revise uncommitted price/description or add/remove genuine available units.
4. **Validations:** protected reservations, truthful surplus, admitted quantity identity, category safety, future schedules, usable deadlines, and no changed unit definition after commitment.
5. **Result:** future booking terms change; existing accepted terms remain fixed; quantity adjustment is explicit.
6. **Events:** OfferChanged, OfferPriceChanged, QuantityAdded or QuantityWithdrawn, PriceScheduleChanged when applicable.
7. **Failures:** reject a reduction below allocated quantities. If physical stock has been lost, declare shortage and cancel affected obligations instead of fabricating stock.

### UC-014 — Apply progressive pricing or become free

1. **Actor:** platform under supplier-approved schedule; supplier may request a permitted manual future change.
2. **Preconditions:** bookable non-terminal offer, effective stage reached, schedule valid.
3. **Command:** apply the effective price to new commitments.
4. **Validations:** chronological stage, non-increasing schedule price, applicable policy, zero-price transition rules.
5. **Result:** new consumers receive the current price; zero-priced new commitments use Free mode and its cap/deadline. Paid and payment-pending commitments retain their original amount and mode.
6. **Events:** PriceStageActivated, OfferPriceChanged, OfferBecameFree when zero reached.
7. **Failures:** do not apply a stage after booking closes; do not retroactively refund or charge a price difference solely because a later stage exists.

### UC-015 — Report and moderate

1. **Actor:** identified reporter; operations investigates.
2. **Preconditions:** identifiable target and permitted report reason.
3. **Command:** submit allegation, assess risk, issue reasoned decision.
4. **Validations:** evidence relevance, duplicate cases, severity, conflicts of interest, affected commitments and financial remedies.
5. **Result:** urgent containment if warranted; Upheld or Dismissed finding; scoped sanction, restitution, and appeal opportunity.
6. **Events:** ReportSubmitted, ModerationContainmentApplied, ModerationDecisionIssued, SupplierStandingChanged or ConsumerRestrictionChanged as applicable.
7. **Failures:** lack of corroboration does not automatically punish the target; retaliatory or knowingly false reports become separate cases.

### UC-016 — Approve and execute a refund

1. **Actor:** platform under policy; financial operations for exceptions; Asaas substantiates online execution.
2. **Preconditions:** received payment and an entitlement or justified decision.
3. **Command:** approve a specified return against that source payment.
4. **Validations:** approved reason, amount, original payer, previous returns, pending dispute recovery, responsible funder, method eligibility.
5. **Result:** tracked reimbursement obligation, then confirmed partial or full return; blocked execution remains outstanding with an owner.
6. **Events:** RefundRequested, RefundApproved or RefundRejected, RefundStarted, RefundFailed or RefundCompleted.
7. **Failures:** insufficient funds or provider restriction does not mark Refunded; escalate execution while preserving entitlement. Never pay an unrelated recipient suggested by an unverified message.

### UC-017 — Review a completed transaction

1. **Actor:** consumer and supplier as separate participants.
2. **Preconditions:** recorded final handover, genuine participant, no previous review by that author for that transaction.
3. **Command:** submit feedback within 14 days after pickup.
4. **Validations:** completed transaction, authorship, prohibited content, self-dealing, manipulation indicators.
5. **Result:** one eligible review; publish when both submit or the interval ends. Objective reliability remains separate.
6. **Events:** ReviewSubmitted, ReviewPublished, ReputationUpdated.
7. **Failures:** cancelled/no-show transactions cannot receive transaction star reviews; incident reports remain available. A partial final handover is eligible and its fulfillment scope is disclosed.

### UC-018 — Change plan benefits

1. **Actor:** supplier owner.
2. **Preconditions:** existing enrollment and published commercial terms.
3. **Command:** request upgrade, downgrade, or non-renewal.
4. **Validations:** effective date, accepted price/period, payment confirmation for paid upgrade, allowance already consumed, live reservations.
5. **Result:** upgrade takes effect after confirmed payment; downgrade/non-renewal at period end; existing commitments and history are preserved.
6. **Events:** PlanChangeRequested, PlanActivated, PlanChangeScheduled, PlanPeriodEnded.
7. **Failures:** unpaid upgrade creates no benefit; downgrade cannot retract completed publications or create negative allowance.

### UC-019 — Configure interests and issue a relevant alert

1. **Actor:** consumer chooses preferences; platform evaluates relevance.
2. **Preconditions:** opted-in preferences and a discoverable offer.
3. **Command:** declare interests, then identify a new matching opportunity.
4. **Validations:** category, radius, price/free preference, current stock, usable pickup time, permission, repetition cap.
5. **Result:** eligible alert referring to current offer terms, with no stock priority.
6. **Events:** InterestsChanged, RelevantOpportunityIdentified.
7. **Failures:** removed, expired, sold-out, out-of-radius, or opted-out offers create no alert; following an establishment does not bypass safety or price filters.

### UC-020 — Publish a future recurring occurrence

1. **Actor:** supplier confirms a template occurrence.
2. **Preconditions:** recurrence entitlement, active template, expected business day.
3. **Command:** confirm today's actual surplus quantity and condition.
4. **Validations:** publication rules, current supplier readiness, genuine quantity, period allowance, intervals, safety.
5. **Result:** an independent eligible offer; missed confirmation means no offer.
6. **Events:** RecurrenceOccurrenceConfirmed, OfferPublished, PublicationAllowanceConsumed.
7. **Failures:** template activation alone never generates consumer stock; no confirmation after the booking deadline.

### UC-021 — Declare and fulfill future demand

1. **Actor:** consumer; suppliers may view aggregated local interest.
2. **Preconditions:** eligible request category and declared approximate area.
3. **Command:** publish a wanted-surplus request with quantity and expiry.
4. **Validations:** truthful non-prohibited request, unit basis, category, radius, validity, and privacy.
5. **Result:** Active demand; matching offers create suggestions, accepted pickups reduce remaining demand with consumer acknowledgment; zero remaining demand closes it as Fulfilled.
6. **Events:** DemandPublished, DemandMatched, DemandFulfilled, DemandExpired or DemandCancelled.
7. **Failures:** a match does not reserve quantity, promise future stock, or expose the consumer's exact location.

