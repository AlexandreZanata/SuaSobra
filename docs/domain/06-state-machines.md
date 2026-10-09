## Part 6 — State Machines

Launch scope update (October 8, 2026): follow [ADR-002](../decisions/0002-free-simple-launch.md) and Part 14. Online money processing, returns, subscriptions and advanced capabilities below are future-only; launch uses free offers or a fixed goods price settled directly at pickup. Current service access is free for consumers and establishments.

The notation is **current state → action → condition → new state**, followed by the business effect. Listed transitions are exhaustive for the baseline; any unlisted transition is prohibited. Repeated recognition of the same action has no additional stock, payment, sanction, or metric effect.

### 6.1 Offer lifecycle and availability

Offer lifecycle states are **Draft, Scheduled, Published, Paused, Closed, Expired, Cancelled, Removed**. Published means eligible for booking during its interval. Its availability label is **Active** when all unfulfilled units are available, **Partially Reserved** when some quantity is committed or already fulfilled and some remains available, and **Sold Out** when no quantity is available. These labels are derived conditions, not competing terminal lifecycle states.

- Draft → save changes → supplier is authorized → Draft. No public commitment exists.
- Draft → schedule → eligible complete offer; booking starts later; publication allowance available → Scheduled. One allowance is consumed for this committed publication, including if later cancelled.
- Draft → publish → eligible complete offer; booking interval is open; allowance available → Published. Consume one allowance and expose current available quantity.
- Scheduled → open booking → booking start reached; supplier and offer still eligible → Published. No second allowance consumption.
- Scheduled → pause → supplier requests or operations contains a non-terminal issue → Paused. No public quantity commitment.
- Scheduled or Published → cancel → authorized supplier states reason → Cancelled. End new booking, withdraw uncommitted stock, and cancel affected unfulfilled commitments with refunds where owed.
- Published → pause → authorized supplier or operations; booking not ended → Paused. Stop new reservations; existing confirmed terms continue unless separately cancelled.
- Paused → resume → eligibility restored and booking interval open → Published; or Scheduled if booking has not opened. No entitlement extension.
- Published or Paused → close booking voluntarily → supplier will fulfill existing commitments → Closed. Withdraw only uncommitted stock; confirmed entitlements and already accepted temporary payment commitments retain their original deadlines. Timely payment can confirm such a surviving commitment, but no new commitment is accepted.
- Scheduled, Published, or Paused → booking deadline reached → deadline has arrived → Expired. Expire uncommitted stock and unresolved payment commitments. Confirmed reservations remain collectible until their own deadlines.
- Draft, Scheduled, Published, or Paused → remove → operations finds prohibited or unsafe publication → Removed. No further booking; all affected unfulfilled commitments are cancelled and financially remedied.
- Closed, Expired, or Cancelled → record removal → a later safety or prohibited-activity finding affects historical publication → Removed. Preserve prior closure history and remedy any still-live commitments.
- Draft → discard → supplier abandons unpublished draft → Cancelled. No published activity or stock impact.
- Published/Active → accept quantity → some availability remains → Published/Partially Reserved.
- Published/Active or Published/Partially Reserved → accept final available units → no availability remains → Published/Sold Out.
- Published/Sold Out → release or add eligible quantity → booking still open and available quantity becomes positive → Published/Active or Published/Partially Reserved according to quantity history.

Closed, Expired, Cancelled, and Removed do not reopen. A new genuine surplus publication receives its own identity, quantity declaration, deadline, and allowance consumption. Selling out does not cancel a price schedule or existing reservations. A post-closure safety case can cancel remaining entitlements without pretending that closure occurred twice.

### 6.2 Reservation fulfillment

Fulfillment states are **Initiated, Awaiting Payment, Confirmed, Ready for Pickup, Picked Up, Cancelled by Consumer, Cancelled by Supplier, Cancelled by Platform, Expired, Outcome Under Review, No-Show, Failed Pickup**. Disputed and Refunded are separate financial/case statuses; they never replace physical fulfillment history.

- No reservation → request → identified consumer submits quantity → Initiated. No allocation until acceptance.
- Initiated → accept online terms → all eligibility, time, price, caps, and stock checks pass → Awaiting Payment. Create temporary commitment and one payment obligation.
- Initiated → accept free or establishment terms → all checks pass → Confirmed. Allocate confirmed quantity; no online commitment interval.
- Initiated → reject → any acceptance check fails → Expired with reason Not Accepted. No stock, no no-show, no payment obligation.
- Awaiting Payment → recognize payment → exact obligation satisfied before deadline, commitment still live → Confirmed. Move quantity from temporary to confirmed reserved.
- Awaiting Payment → payment definitively fails or deadline arrives → original obligation unpaid → Expired. Dispose of allocation under release rules; never record no-show.
- Awaiting Payment → consumer cancels → still unfulfilled → Cancelled by Consumer. End entitlement, invalidate pending obligation, release quantity where permitted.
- Confirmed → pickup opens → supplier has not declared a problem; accepted interval open → Ready for Pickup. Readiness describes entitlement, not independent proof of product condition.
- Confirmed or Ready for Pickup → consumer cancels → handover has not occurred → Cancelled by Consumer. Apply stock disposition and refund entitlement.
- Awaiting Payment, Confirmed, or Ready for Pickup → supplier cannot fulfill → supplier states reason → Cancelled by Supplier. Preserve accountability and apply refund entitlement.
- Awaiting Payment, Confirmed, or Ready for Pickup → platform cancels → justified safety, fraud, account, or moderation decision → Cancelled by Platform. Attribute fault separately.
- Ready for Pickup → final handover → verified actual receipt and required payment satisfied → Picked Up. Record full or agreed separable-unit partial fulfillment; cancel/dispose of any remainder.
- Confirmed or Ready for Pickup → effective pickup deadline reached → no verified final handover → Outcome Under Review. End entitlement, release/dispose once, and establish any refund obligation for uncollected quantity.
- Ready for Pickup → report failed collection → consumer attended but promised fulfillment failed → Outcome Under Review. Stop handover if unsafe or unavailable and resolve responsibility.
- Outcome Under Review → resolve absent consumer → supplier was ready and no substantiated exception; challenge opportunity provided → No-Show. Apply proportionate consequence once.
- Outcome Under Review → resolve supplier failure or neutral incident → substantiated supplier failure or force majeure → Failed Pickup. Record attribution; no consumer no-show penalty.
- Outcome Under Review → recognize prior completed handover → evidence establishes one handover occurred before the deadline → Picked Up. Only undisposed quantities with consistent history may be corrected; if quantity was reallocated or refunded, operations records a reconciliation case and financial adjustment instead of automatic rewriting.

Terminal fulfillment states cannot become a fresh active reservation. A later receipt triggers a refund; a later complaint opens a case. An attribution appeal may change No-Show to Failed Pickup or vice versa through an authorized correction, never without evidence and preserved history.

### 6.3 Payment obligation and attempt

Online obligation positions are **Created, Pending, Confirmed, Failed, Expired, Cancelled, Partially Refunded, Refunded**. A **Disputed** case is an additional flag with the prior financial position retained. Payment attempts independently retain Pending, Failed, Expired, Cancelled, or Confirmed receipt outcomes.

- Created → initiate permitted attempt → live commitment and valid amount/method → Pending.
- Pending → attempt fails → retry time remains → Pending obligation with Failed attempt. A retry uses the same remaining deadline and quantity commitment.
- Pending → recognize exact receipt → before deadline and live reservation → Confirmed.
- Pending → definitive failure → no permitted retry remains → Failed. Reservation expires and quantity is disposed of once.
- Created or Pending → deadline reached → unsatisfied obligation → Expired. End payment entitlement; late funds remain returnable.
- Created or Pending → cancel → reservation ended or consumer abandons payment → Cancelled. A later receipt is exceptional received money with a refund obligation, not renewed collection eligibility.
- Confirmed → confirm partial return → positive returned amount below received amount → Partially Refunded.
- Confirmed or Partially Refunded → confirm remaining full return → cumulative returned amount equals received amount → Refunded.
- Expired, Cancelled, or Failed → recognize late receipt → provider substantiates money received → retain ended obligation position and record an exceptional receipt awaiting return; once returned, record Refunded disposition. Never confirm the reservation.
- Any received-money position → open financial dispute → external contestation or justified complaint → same financial position with Disputed case. Collection eligibility is assessed separately.
- Disputed case → resolve → substantiated decision or financial-provider outcome → Resolved case. Apply any confirmed reversal/return and reconcile outstanding refund obligations.

Refund execution states are **Requested → Approved or Rejected; Approved → In Progress; In Progress → Completed or Failed; Failed → In Progress** after its obstacle is addressed. A financial dispute pauses overlapping refund execution when necessary to prevent double recovery, but does not erase entitlement.

At-establishment obligations use **Due → Acknowledged Paid → Partially Refunded or Refunded** with corresponding evidence. Free reservations have **Not Applicable** for payment. A consumer screenshot or card authorization alone is not confirmation of online receipt.

### 6.4 Pickup

- Planned → pickup opens → confirmed reservation and permitted interval → Ready.
- Ready → verify entitlement → valid unused confirmation reference and authorized collector → Verified. Quantity remains reserved.
- Verified → acknowledge final handover → both parties confirm actual quantity and payment condition satisfied → Completed. Fulfillment is Full or Partial; exactly one final handover.
- Verified → verification abandoned → no handover and interval remains open → Ready. No stock movement.
- Planned, Ready, or Verified → reservation cancelled → no actual handover → Cancelled.
- Planned, Ready, or Verified → deadline or failed attempt → no valid final handover → Failed. Reservation attribution enters review; stock and money follow their own rules.
- Failed → exceptional reconciliation → operations verifies earlier completed handover with consistent quantity and financial evidence → Completed with correction record. A second pickup remains forbidden.

Completed cannot return to Ready. A claim that a completed handover was fabricated opens a case; reversing erroneous evidence requires dual review and reconciled quantity history, rather than a new collection permission.

### 6.5 Report and appeal

- No report → submit → identified reporter, valid target/reason → Submitted.
- Submitted → triage → operations assigns severity and scope → Under Review. Immediate safety containment may already apply.
- Under Review → request evidence → evidence materially incomplete → Awaiting Evidence.
- Awaiting Evidence → evidence arrives → sufficient to proceed → Under Review.
- Awaiting Evidence → response deadline reached → decide on available evidence → Under Review. Silence does not prove guilt.
- Under Review → decide substantiated → supported finding → Upheld. Apply remedy and proportionate sanction.
- Under Review → decide unsupported → allegation not established → Dismissed. Remove unjustified provisional restrictions.
- Submitted or Under Review → merge duplicate allegation → same underlying case → Closed as Duplicate with reference. No doubled sanction.
- Upheld or Dismissed → appeal → affected party submits new evidence or identifies decision error within 7 days → Appealed.
- Appealed → independent review → authorized reviewer issues reasoned decision → Upheld or Dismissed with appeal decision recorded.
- Upheld or Dismissed → close → remedies assigned and appeal resolved or interval ended → Closed.

Later material safety evidence can open a linked new case. A report cannot be deleted to remove a sanction history or refund obligation.

### 6.6 Supplier standing

- Pending Approval → approve → identity, authority, establishment, and policy checks pass → Active.
- Pending Approval → decline → essential eligibility not met → Rejected with reason and reapplication conditions.
- Rejected → resubmit → relevant defect addressed → Pending Approval.
- Active → warn → resolved minor violation → Warned. May publish subject to stated correction.
- Warned → clear warning → correction verified and observation period satisfied → Active.
- Active or Warned → suspend → serious incident, urgent containment, or recurring resolved failures → Suspended with scope and review date.
- Suspended → restore → corrective requirements met and risk acceptable → Active or Warned.
- Any non-Closed standing → block → substantiated fraud or prohibited activity, or urgent high-risk provisional containment explicitly labeled → Blocked.
- Blocked → successful appeal/reconsideration → authorized evidence-based decision → Suspended or Active. No silent reinstatement.
- Active, Warned, Suspended, or Blocked → request business closure → new publication disabled, outstanding obligations remain assigned → Closed.

Suspension or closure never makes paid obligations disappear. Live pickups continue only when safe fulfillment is explicitly allowed; otherwise they are cancelled and refunded. Financial-provider eligibility has a separate Enabled/Restricted/Disabled standing and cannot be inferred from supplier approval.

