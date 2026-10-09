## Part 11 — Payments and Financial Policies

Launch scope update (October 8, 2026): follow [ADR-002](../decisions/0002-free-simple-launch.md) and Part 14. Online money processing, returns, subscriptions and advanced capabilities below are future-only; launch uses free offers or a fixed goods price settled directly at pickup. Current service access is free for consumers and establishments.

### 11.1 Roles and permitted modes

Asaas is the financial provider for online payments. It determines substantiated financial receipt and return outcomes for the enabled method. Sua Sobra defines reservation entitlement, commercial refund policy, responsibility, and customer communication. Supplier approval does not imply financial-provider approval.

- **Online:** an approved supplier may publish this mode only if its payment method, named recipient, financial costs, refund funding, and operational ability to execute returns are established. Launch can enable Pix; other methods are later additions under the same domain rules. Do not offer a slow-confirmation method for an interval it cannot reasonably satisfy.
- **At the establishment:** the accepted total is due there before the final handover. The supplier declares the locally accepted means before reservation; consumers are not required to pay online. Both parties acknowledge payment. Sua Sobra records the declaration but does not pretend Asaas verified it.
- **Free:** total is BRL 0.00 and no payment obligation exists. A compulsory fee is incompatible with Free mode.

The supplier is the commercial seller and designated beneficiary for the goods. Sua Sobra is accountable for coordinating every refund promised by this specification. For an online supplier-attributed failure, the supplier bears the refunded sale and agreed costs; the platform ensures the consumer remedy is pursued even if supplier funding is delayed. For platform-attributed error, the platform bears its attributable loss. Consumer cancellation/no-show involves no consumer penalty in the MVP; ordinary provider costs follow the predeclared supplier/platform cost agreement.

Do not enable online publication without an assigned, financially viable refund responsibility. This is a business readiness condition, not a claim that a particular Asaas account arrangement automatically provides these protections.

### 11.2 Online transaction sequence

1. Accept a quantity commitment under the current offer terms.
2. Define one obligation equal to accepted unit price multiplied by quantity; no hidden charges or consumer fees in the MVP.
3. Define the deadline as the earlier of acceptance plus 10 minutes and booking close. Refuse commitments with less than 2 minutes available.
4. Initiate a permitted attempt. Only one attempt may remain live; a definitively failed attempt can be retried within the unchanged deadline.
5. Recognize a substantiated exact receipt before the deadline while the commitment remains live.
6. Confirm the reservation and transfer temporary quantity to confirmed reserved quantity.
7. Complete pickup once when entitled, or apply cancellation/expiry and refund rules.
8. Close financial obligations only when all receipts, fees, returns, and disputes are reconciled.

Confirmation is distinct from funds becoming available to the supplier, and both are distinct from the supplier earning value through fulfillment. A provider-confirmed receipt may be sufficient for reservation confirmation without implying immediate supplier access to money. Pickup does not cause a second consumer charge.

### 11.3 Exceptional receipts and precedence

- **Late receipt:** retain terminal reservation status and create full refund entitlement. Never reclaim quantity from a later consumer. This includes evidence recognized after expiry even if the payer initiated payment earlier.
- **Short receipt:** the launch policy does not accept partial settlement or top-ups. End the unsatisfied commitment and return the received amount. The consumer can create a new reservation if current stock permits.
- **Overpayment:** if exact obligation satisfaction is substantiated and timely, confirm only the original obligation and refund the excess. Never increase reserved quantity.
- **Duplicate receipt:** recognize each financial receipt once, settle the obligation once, and return duplicate money. Repeated recognition of the same receipt is not a duplicate financial payment.
- **Unmatched or wrong-currency receipt:** no pickup entitlement; open a reconciliation case and return funds to the substantiated original payer when appropriate.
- **Cancellation and receipt arriving together:** only a still-live accepted obligation can confirm. Once cancellation has ended it, later recognition creates a refund, not revival.
- **External payment screenshot:** evidence for investigation only. Online eligibility requires substantiated receipt through the declared provider.
- **Money paid privately for an online reservation:** does not satisfy its obligation. Resolve the private transfer as a separate incident; do not encourage a second payment until its disposition is explained. A supplier that demanded it is subject to moderation.

### 11.4 Refund entitlement and funding

For all uncollected paid quantity, the baseline consumer entitlement is its original accepted amount in full. Supplier cancellation, missing product, unsafe item, grave mismatch, platform removal, consumer cancellation, and no-show all produce this entitlement. No provider processing cost is deducted from that consumer return.

For an agreed separable-unit partial handover, refund the original unit price times uncollected units. An indivisible unit is either accepted as the promised unit or rejected; it cannot become an involuntary discounted partial unit. If a material defect is established after pickup, operations may approve full or partial restitution against substantiated loss; this does not automatically undo the physical quantity outcome.

Refunds return against the original source payment to its original payer. Record amount, reason, entitlement decision, liable party, requested date, execution progress, confirmed return, and outstanding remainder. An approved refund blocked by available balance, method constraints, or financial restrictions stays outstanding. Operations assigns corrective funding and informs the consumer of progress. The platform does not substitute store credit without the consumer's explicit choice and an applicable later policy.

The proposed service target is to approve clear automatic cases immediately and initiate eligible approved returns within 1 business day. Actual completion depends on the financial method and provider result; never label an initiated request as money received by the consumer. A missed service target escalates internally and does not reduce entitlement.

If a supplier has already received sale proceeds, its reimbursement responsibility survives cancellation and account closure. The platform maintains responsibility for arranging the promised consumer return and separately pursues supplier recovery. A supplier funding problem cannot be transformed into a consumer no-show fee.

### 11.5 Financial-provider facts and limits

Asaas documents full and partial returns for paid Pix and credit-card charges, and limits cumulative Pix returns to the original paid amount. Its reference also notes that original processing charges may remain and insufficient available funds can prevent a return. These facts support separating refund entitlement from execution and naming a responsible funder. [Source: Asaas refund reference](https://docs.asaas.com/reference/estornar-cobranca).

Asaas's help center lists account eligibility, a maximum receipt age, and sufficient available funds among Pix-return conditions. Sua Sobra must check the applicable current conditions before enabling a method; this document does not turn a provider's operational return window into the expiry of a consumer's reimbursement entitlement. [Source: Asaas Pix-return conditions](https://central.ajuda.asaas.com/hc/pt-br/articles/53177331159835-Quais-as-regras-e-custos-para-estornar-um-Pix).

These provider facts were checked on October 8, 2026. No automatic custody of money, conditional release service, guaranteed refund time, fee amount, or guaranteed dispute recovery is assumed. Any such later commercial arrangement must be verified and disclosed before use. Method-specific parameters belong to a dated business policy and can change prospectively.

### 11.6 Supplier proceeds, fees, and reconciliation

Show separately: gross amount received, amount associated with fulfilled units, refundable unfulfilled amount, actual provider costs, platform commission if later enabled, confirmed returns, outstanding returns, disputed amount, and supplier net proceeds.

At-establishment revenue is **declared received**, with both-party acknowledgment; online revenue is **provider-confirmed**. Neither alone proves a pickup. Recovered supplier revenue metrics count fulfilled accepted value and distinguish declared local receipts from verified online receipts.

The MVP platform commission is zero. Before enabling online payments, disclose who absorbs provider charges and whether its cost makes a small offer commercially viable. A supplier may use establishment or free mode for a future publication if online costs are unsuitable; live online commitments cannot be switched silently. A returned sale reverses any later platform commission attributable to the refunded portion under the accepted fee policy. Non-returned provider charges remain a supplier/platform cost.

Outstanding consumer returns and substantiated financial risk affect future online eligibility. Do not promise that supplier funds can be delayed or clawed back automatically. Any proceeds eligibility policy requires a verified financial arrangement and a disclosed contract. Financial exposure must remain assigned even when sale proceeds have already been distributed.

### 11.7 Disputes and contested payments

A consumer commercial complaint and an external financial contestation are different cases, although they may concern the same transaction. An external dispute may reverse funds without changing recorded pickup. Record claimed amount, evidence, prior financial position, decision, returned/reversed funds, and remaining exposure.

When funds are disputed before pickup, pause fulfillment eligibility while confirming whether the payment still satisfies the obligation. If it cannot be confirmed before the safe pickup limit, cancel the unfulfilled reservation without a consumer penalty solely for contesting. After pickup, preserve the physical outcome and resolve the financial case.

Prevent overlapping refund and dispute recovery of the same amount. If a return is already in progress, operations reconciles it with any external reversal; the consumer is not made to lose a valid remedy because of internal uncertainty. A rejected dispute does not automatically prove a dishonest consumer; sanctions require separate substantiation.

### 11.8 Cancellation matrix

Each entry specifies **final fulfillment state; quantity disposition; financial effect/refund; reputation; republication**. Availability always follows the safety/closure precedence in Part 10.

1. **Consumer cancels before payment:** Cancelled by Consumer; release if eligible, otherwise expire/withdraw; end unpaid obligation and return any exceptional late receipt; record cancellation without no-show; eligible remaining surplus may stay published or be genuinely republished after terminal closure.
2. **Consumer cancels after online payment, before handover:** Cancelled by Consumer; release/expire/withdraw; full return for uncollected units; late-cancellation behavior may be reviewed but no automatic star review or no-show; genuine safe remainder may be offered again.
3. **Consumer cancels free or establishment reservation before handover:** Cancelled by Consumer; release/expire/withdraw; no online refund, and return any substantiated local prepayment; record cancellation; genuine remainder may be offered again.
4. **Supplier cancels before any commitment:** offer Cancelled; uncommitted units withdrawn; no consumer payment obligation; ordinary cancellation count, no invented consumer incident; new publication only with real eligible stock.
5. **Supplier cancels with unpaid online commitments:** Cancelled by Supplier for affected commitments; units withdrawn if offer cancelled or unavailable; stop unpaid obligations and return later receipts; supplier failure assessed unless a substantiated neutral cause; safe genuinely restored stock requires an explicit new declaration.
6. **Supplier cancels confirmed free/local commitments:** Cancelled by Supplier; units withdrawn if unavailable, otherwise disposition reflects the reason; return any acknowledged local prepayment; resolved supplier failure affects reliability; no automatic restoration of cancelled reservations.
7. **Supplier cancels a paid reservation:** Cancelled by Supplier; unavailable/unsafe units withdrawn, genuinely available units released only on a still-open eligible offer; full refund for uncollected paid quantity, supplier cost responsibility; supplier failure unless neutral cause established; new offer must not conceal the failure.
8. **Payment fails or expires:** Expired reservation; release if booking remains open, otherwise expire/withdraw; unpaid obligation ends, exceptional receipts refunded; no no-show; remaining eligible stock may be reserved normally.
9. **Offer booking expires without a commitment:** offer Expired; available units move to expired; no refund needed; expiry is not misconduct; new publication requires unchanged truthful physical age/condition and a genuinely valid new commercial window.
10. **Offer booking expires with confirmed later pickups:** offer Expired, reservations remain Confirmed/Ready; reserved units retained; payment remains tied to their accepted obligation; no failure solely for booking closure; no republication of reserved units.
11. **Online payment pending when booking expires:** reservation Expired; temporary units expire unless an earlier withdrawal reason applies; late receipt fully refunded; no no-show; never create a second claim to expired units.
12. **Platform removes a publication with live commitments:** Cancelled by Platform; affected units withdrawn; all uncollected online payments refundable, local prepayments remedied through the supplier; responsibility depends on substantiated reason, no consumer penalty; removed goods cannot reappear through simple republication.
13. **Item becomes unsafe or recalled:** affected unfulfilled reservations cancelled by supplier/platform; units withdrawn regardless of price or commitment; full refund for uncollected paid goods, post-pickup incident reviewed separately; supplier negligence only when established; no republication until eligibility can genuinely be restored, and unsafe food remains excluded.
14. **Establishment closes unexpectedly:** Cancelled by Supplier or Failed Pickup after attendance case; no stock released into an unavailable point; full refund for uncollected paid units; supplier fault or neutral cause resolved; republish only after service availability is real.
15. **Force majeure prevents pickup:** Cancelled by Supplier/Platform or Failed Pickup with Neutral attribution; withdraw or expire according to actual condition; full return for uncollected paid units; no consumer no-show or automatic supplier penalty; eligible future surplus may be published after recovery.
16. **Consumer does not collect:** No-Show only after attribution; remaining units release only if still bookable and safe, otherwise expire/withdraw; full return for uncollected online value; proportional resolved no-show consequence; same physical stock may be offered only under truthful current conditions.
17. **Consumer arrives but product is missing/materially different:** Failed Pickup with Supplier attribution when substantiated; missing/unsafe units withdrawn; full return for rejected uncollected quantity; supplier reliability affected, consumer protected; corrected genuine stock requires truthful new/current declaration.
18. **Agreed partial pickup of separable units:** Picked Up with Partial fulfillment and remainder Cancelled; handed-over quantity fixed, remainder release/expire/withdraw; return original value of uncollected units; shortage responsibility assessed; only remaining eligible units may be offered.
19. **Cancellation requested after completed handover:** retain Picked Up; no automatic stock reversal; open a complaint and approve restitution only where justified; no no-show; returned physical goods require independent safety/eligibility assessment before any new offer.
20. **Account suspended or closed with live commitments:** existing fulfillment assessed for safety and authority; cancelled commitments explicitly dispose quantity; online returns and local remedy duties survive closure; responsibility depends on the underlying case; no bypass through a new account.

