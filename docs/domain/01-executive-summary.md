## Part 1 — Executive Summary of the Domain

Launch scope update (October 8, 2026): follow [ADR-002](../decisions/0002-free-simple-launch.md) and Part 14. Online money processing, returns, subscriptions and advanced capabilities below are future-only; launch uses free offers or a fixed goods price settled directly at pickup. Current service access is free for consumers and establishments.

### 1.1 Purpose and identity

Sua Sobra connects local suppliers with consumers who can collect useful surplus. Its central proposition is: **what is left over for one person can have value for another**. The initial focus is local businesses and small towns. Successful outcomes include recovered supplier revenue, affordable consumer purchases, free pickups, and evidenced reuse.

An eligible offer represents a real surplus circumstance: daily leftovers, excess production, approaching expiry, minor damage, damaged packaging, a remaining batch, construction leftovers, offcuts, discontinued stock, items intended for disposal, free surplus, or reusable materials. A low price or a zero price alone does not establish eligibility. Ordinary retail inventory and generic secondhand resale are outside the product's identity.

### 1.2 Core business journey

1. An authorized supplier registers an establishment and a pickup point.
2. The supplier publishes an eligible offer with a clear sellable unit, quantity, price or free status, payment mode, booking interval, pickup interval, and relevant condition information.
3. A consumer discovers nearby offers and requests a specific quantity.
4. An accepted request temporarily commits quantity for online payment, or immediately confirms a free or pay-at-establishment reservation.
5. The reservation fixes the relevant commercial terms. The supplier cannot retroactively increase its price, reduce its promised quantity, or move its pickup point.
6. The consumer collects within the agreed interval. Both parties verify the handover, and any applicable payment must be satisfied.
7. The business records the actual quantity handed over, the outcome, the financial position, and review eligibility.
8. Cancellations, payment failures, expiry, safety incidents, and disputes follow explicit recovery rules.

Chat and negotiation are never prerequisites. An offer provides enough information to decide and collect. Publishing a small surplus should require only information necessary for eligibility, safety, truthful availability, and fulfillment.

### 1.3 Launch decisions and resolved ambiguities

- **Supplier eligibility:** the MVP accepts local organizations, businesses, microenterprises, producers, and professionals acting as identifiable suppliers at a declared establishment. Casual household sellers are a future expansion.
- **Consumer eligibility:** browsing requires no commercial commitment. Reservations require an identified adult account, verified contact, acceptance of the terms, and no applicable restriction. The proposed launch policy is age 18 or older.
- **Fulfillment:** pickup only. Delivery, shipping, bargaining, auctions, and multi-supplier baskets are outside the baseline.
- **Offer unit:** one offer has one fixed sellable unit. A unit can be a single item, a described pack, a lot, a surprise bag, a box, or a basket. A heterogeneous lot is indivisible.
- **Payment modes:** online through Asaas, at the establishment, or free. A published offer has one mode. Consumers do not switch a live reservation between modes.
- **Reservation priority:** the first accepted commitment obtains the quantity. Viewing, expressing interest, or starting an unaccepted request creates no entitlement.
- **Price lock:** the accepted quantity commitment locks the current price, including during its payment deadline. Looking at a price does not lock it.
- **Expiry:** offer booking expiry stops new commitments. It does not cancel confirmed pickups whose protected deadlines are later.
- **Consumer cancellations and no-shows:** the MVP refunds online payments for uncollected quantities in full, including a no-show. Repeated abuse is controlled through proportionate reservation restrictions rather than forfeiture.
- **Partial pickup:** one final handover is allowed. Separable units can have a smaller agreed handover with an explicit outcome for the remainder. An indivisible unit cannot be split.
- **Commercial thresholds:** the defaults below make behavior determinate. Operations may change future policy versions; existing commitments retain their accepted version.

These choices favor a simple first experience while protecting quantity ownership, truthful expectations, and financial traceability.

### 1.4 Proposed launch policy defaults

- Online payment commitment: at most 10 minutes, bounded by the offer's booking close time.
- Total simultaneous consumer commitments: 3, counting temporary payment commitments and confirmed reservations.
- Simultaneous free reservations: 2 within that total. New consumers are eligible without previous ratings.
- Maximum quantity per consumer per offer: 3 sellable units, including completed pickups and live commitments; the supplier may lower this before publication. A lot is one unit. Cancelled quantities stop consuming this cap, but repeated cancellations are assessed for abuse.
- Standard pickup grace: up to 10 minutes after the accepted pickup end, never beyond a declared hard safety limit or the establishment's confirmed service end.
- Free pickup deadline: the earlier of the offer's pickup end and 60 minutes after the later of reservation confirmation or pickup opening. The exact resulting interval is shown before acceptance.
- Free minimum usable pickup interval: 15 minutes. Do not accept a free reservation with less remaining time.
- Online minimum payment interval: 2 minutes. Do not accept an online commitment if less time remains before booking closes.
- No-show attribution challenge interval: 24 hours after the provisional outcome is communicated. No punishment applies before attribution is resolved.
- Initial marketplace radius: 10 km; consumers can select a smaller or larger radius within supported local coverage.
- Normal complaint response target: acknowledge within 1 business day and decide within 3 business days after sufficient evidence. Urgent safety reports take immediate priority.
- Free-plan publication allowance: an initial configurable launch allowance of 30 offers per monthly plan period. This is a proposal, not a fixed commercial commitment.
- No consumer subscription or reservation fee in the MVP. No marketplace commission in the MVP. Actual financial-provider costs must have an identified payer under Part 11.

Every deadline has a named time zone, exact timestamp, and purpose. Offer and pickup times use the pickup point's time zone. Start boundaries are inclusive; end boundaries are exclusive. Payment confirmation must be accepted before the payment deadline. Pickup must occur before the effective pickup deadline. At an exact boundary, expiry takes precedence over a new acceptance. Operations cannot backdate an action to obtain priority.

