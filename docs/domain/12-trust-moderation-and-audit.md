## Part 12 — Reputation, Moderation, Antifraud, and Audit

### 12.1 Observable consumer reliability

Display completed pickups, free completed pickups, resolved eligible collection outcomes, resolved no-shows, consumer cancellations, and substantiated abuse restrictions. Keep lifetime counts and a clearly labeled recent 60-day reliability window.

Consumer attendance rate equals **completed pickup obligations divided by completed pickup obligations plus resolved consumer-attributed no-shows**, for the same window. A final partial handover counts as one attended pickup, with partial fulfillment separately visible. Exclude supplier failures, force majeure, pending attribution, and payment-pending expiry. A user with no eligible outcomes is shown as New, not as 0% reliable. Avoid public percentages until at least five eligible outcomes; show counts instead.

Consumer cancellation counts distinguish timely cancellation from late cancellation, proposed as less than 30 minutes before accepted pickup end. This is an operational behavior indicator, not proof of fraud. No-show and cancellation counts must not be combined into one unexplained punishment score.

### 12.2 Observable supplier reliability

Display actual fulfilled reservations, declared/verified revenue basis, reserved quantity successfully handed over, supplier-attributed failed/cancelled reservations, substantiated missing products, serious mismatches, safety findings, and eligible reviews. Mere report count is not a reliability penalty.

Supplier fulfillment reliability equals **completed final handovers divided by completed final handovers plus resolved supplier-attributed failed/cancelled reservations** in the same observation window. Show quantity fulfillment separately so partial handovers do not disguise chronic shortages. Exclude unpaid commitment expiries, consumer no-shows/cancellations, force majeure, and unresolved cases. Suppress percentage rankings below five eligible outcomes and show the sample count.

An offer is commercially fulfilled when at least one unit is picked up, with full/partial offer fulfillment reported separately. “Sold Out” during booking is allocation, not fulfillment. End-of-offer actual pickup counts determine the fulfillment label.

### 12.3 Review eligibility and retaliation prevention

Only consumer and accountable supplier from a completed pickup may review that transaction, each once, within 14 days. Cancellation, payment without pickup, and no-show permit incident reporting but not transaction star reviews. A legitimate partial final handover is eligible with its partial scope disclosed.

Publish both sides when both submit or the 14-day interval ends. Before publication neither sees the other's review. Allow correction of factual mistakes before publication, with history, but no repeated ratings. Removing abusive text does not create a second review right. Post-publication factual corrections and removals require moderation with reasons.

Self-dealing, fake pickups, linked-account ratings, coerced positive feedback, and trade of free stock for reviews are prohibited. A supplier cannot refuse a promised pickup or refund because of a negative review. Repeated transactions with the same counterparty remain real history but cannot dominate public comparison: for a displayed aggregate star rating, count at most one review per reviewer-recipient pair per 30-day interval, using the most recent eligible review in that interval. Keep excluded reviews available as individual legitimate experiences.

### 12.4 Proportional no-show response

Use resolved incidents within a rolling 60-day window. Communicate reason, counted incidents, restriction scope, end time, and appeal route. Proposed progression:

1. **First resolved no-show:** record and explain collection responsibility; no reservation restriction.
2. **Second:** warning and clear reminder of the accepted deadline; no automatic block.
3. **At least three, and no-show rate at least 40% across at least five eligible outcomes:** for 7 days reduce total simultaneous commitments to 1 and free simultaneous reservations to 1.
4. **At least five within the window, with the same rate condition, or a new resolved no-show during an active restriction:** operations reviews for a 7-day block on new reservations. The evidence and proportionality decision are explicit.
5. **Substantiated deliberate hoarding or evasion:** stronger scoped suspension or account block, with appeal.

When incident count increases but a restriction's sample/rate condition is not met, continue recorded guidance/warnings without an automatic restriction. A suspension is a new reasoned decision, not a punishment inferred solely from a small sample.

The platform never automatically blocks an occasional user on one incident or an unresolved accusation. Restrictions expire at the stated time unless a new reasoned decision applies. A successful appeal removes the incident from active thresholds. Old incidents leave the 60-day window; historical records remain with limited access. Safe collection of existing reservations remains possible unless the underlying case requires explicit cancellation.

Supplier failures receive equivalent seriousness: first isolated failure prompts correction; repeat failures prompt warning; three attributable failures across at least five outcomes in 60 days and failure rate at least 40% trigger publication restriction and operations review. Serious safety or fraud can trigger immediate containment regardless of sample size. Restore publishing only after an adequate explanation and corrective readiness; do not penalize consumers for the supplier restriction.

### 12.5 Moderation severity and remedies

- **Level 1 — Honest error:** confusing category, correctable non-material description. Request correction; remove exposure if eligibility is unclear; record guidance rather than fraud.
- **Level 2 — Negligence:** repeated inaccurate availability, missed service, undisclosed relevant condition. Warn, require correction, remedy transactions, and temporarily restrict publishing if needed.
- **Level 3 — Recurring abuse:** hoarding, repeated false declarations, retaliatory conduct, evasion. Restrict the relevant capability, investigate linked activity, and impose time-bounded suspension where justified.
- **Level 4 — Fraud or immediate serious risk:** fabricated goods/payments/pickups, stolen or counterfeit items, materially unsafe goods. Contain immediately, cancel affected commitments, preserve evidence, arrange returns, and determine blocking/appeal.
- **Level 5 — Prohibited activity with substantial danger:** serious prohibited trade or organized harmful evasion. Block relevant business activity, protect affected consumers, escalate to authorized operations leadership, and handle any required external action through a separately authorized process.

Severity depends on harm, intent, repetition, corroboration, and corrective behavior. An error does not become fraud because the consumer disliked it. Temporary containment is labeled provisional; final sanctions identify the findings. The same underlying incident should not receive multiple escalating penalties simply because it generated several reports.

### 12.6 Report rules and evidence

Permit offer, supplier, consumer, and transaction reports for nonexistent stock, unsafe condition, misleading price/value, prohibited goods, identity/payment fraud, missed fulfillment, harassment, discriminatory conduct, retaliation, or abusive reservations. Collect a clear explanation and proportionate evidence: accepted terms, attendance statements, condition photos, payment substantiation, or relevant correspondence. Do not require chat to establish an otherwise evident missed promise.

Publicly undisclosed private information and payment details stay restricted to relevant authorized reviewers. The target can respond to the substance without receiving the reporter's private identity. Reports about the same underlying conduct may be grouped, preserving each reporter's account. A knowingly fabricated report is its own abuse case; a sincere report that cannot be proven is not punishable by default.

### 12.7 Antifraud risk register

Each entry provides **signal; response; impact; escalation**.

- **Nonexistent offer:** repeated missing-goods evidence or implausible stock; pause affected booking, check supply declaration; cancel affected commitments/refund and attribute substantiated failures; repeat or deliberate fabrication leads to suspension/block.
- **Misleading price or ordinary value:** improbable discounts, ordinary price changed just before listing; require a credible basis and remove unsupported savings claims; accepted actual price remains protected, consumer can reject material deception; deliberate repetition leads to publication restriction.
- **Reservation hoarding:** broad rapid commitments, repeated non-collection; apply published caps and resolve outcomes; restrict new commitments proportionately; substantiated deliberate scarcity manipulation leads to operations review.
- **Multiple accounts:** corroborated identity/control links and coordinated evasion; investigate before combining limits; stop duplicate benefits and preserve independent legitimate users; deliberate evasion can extend existing sanctions.
- **Fake pickups/reviews:** reciprocal unusual transactions, no credible handover, concentrated feedback; exclude suspect rating influence pending review and request evidence; reverse proven fraudulent impact/reputation while preserving audit; block responsible parties when substantiated.
- **Private-payment diversion:** supplier asks an online consumer for outside payment; stop misleading instructions and investigate receipts; preserve refund entitlement and do not confirm the online obligation privately; recurring coercion leads to suspension.
- **Free-stock abuse:** accounts repeatedly claim caps or demand resale-scale quantities; apply quantity and simultaneous caps, examine corroborated behavior; keep first-time legitimate access; organized hoarding receives scoped restrictions.
- **Ordinary goods disguised as surplus:** continuous normal catalog without a credible reason; require surplus explanation and remove ineligible offers; preserve remedies for accepted commitments; deliberate repeated misclassification leads to publication suspension.
- **Repeated phantom republication:** same allegedly scarce stock repeatedly missing or expiring with inconsistent declarations; require current evidence and prevent evasion of alert/allowance policies; do not revive old reservations; fraud findings can block supplier activity.
- **Stolen pickup reference:** collector cannot substantiate nomination; refuse handover and let owner replace authorization; no quantity loss; repeated attempts lead to identity/abuse review.
- **Counterfeit payment proof:** screenshot not supported by provider receipt; preserve pending/expired status and investigate; no pickup entitlement, no unsupported accusation; substantiated falsification leads to suspension.
- **Duplicate refund claims:** overlapping cancellation, complaint, and external reversal; reconcile same source amounts and prior returns; consumer receives entitlement once; deliberate double-recovery attempts become abuse cases.
- **Unsafe or stolen material:** disputed provenance, hidden contamination, recalled goods; immediate containment and condition/provenance review; no unsafe pickup, explicit refunds; substantiated prohibited activity leads to blocking.
- **Promised assortment manipulation:** bag repeatedly falls below its quantity/value promise; require truthful basis and substantiate contents; remedies for affected consumers and removal of claims; repeat abuse restricts surprise offers/publication.
- **Retaliatory reports or coercion:** report timing plus explicit threats or conditional refund demands; protect transaction rights and investigate conduct; suppress neither legitimate criticism nor support access; confirmed retaliation leads to warning/restriction/suspension according to harm.

No single approximate-location match, household, device assumption, or unfamiliar behavior is conclusive proof of fraud. Business responses must identify corroborated conduct and provide an appeal path.

### 12.8 History, audit, and corrections

Preserve consequential history: account/supplier qualification; establishment and pickup-point changes; offer draft/publication/schedule; eligibility declarations; price and schedule changes; quantity additions/withdrawals; each accepted commitment; protected terms; payment attempts and receipts; cancellations/expiry; nominated collector changes; verified actual handover; missed pickup and attribution; refund requests/approval/failure/completion; disputes; reviews; reports; sanctions/appeals; plan changes/allowance usage; recurrence confirmation; demand outcomes; and impact corrections.

Each record answers **what happened, when it took effect, when it was recognized, who acted, under which authority/policy, what the prior and resulting state were, what quantity/money changed, and why**. Automated actions name their policy. Monetary records identify their source obligation and liable party. Business outcomes distinguish supplier declarations, participant acknowledgments, provider-confirmed facts, and estimates.

Corrections append a reasoned decision with affected records and reconciled consequences. They do not erase an inconvenient cancellation or retrofit a better price. Sensitive evidence is available only to necessary participants and authorized operations. Consumers see their own commercial history; suppliers see their own fulfillment and financial responsibility. Public reputation uses aggregates and eligible reviews, not private evidence.

For launch, retain routine identifiable history for 24 months after final financial/case closure, then remove or anonymize personal details while preserving non-identifying aggregates. Active financial obligations, safety cases, or justified preservation requirements suspend that routine removal only for necessary records. This is a proposed business retention policy, not a legal retention assertion. Closure/deletion requests cannot erase an active reimbursement duty; operations records the reason and scope of retained information.

