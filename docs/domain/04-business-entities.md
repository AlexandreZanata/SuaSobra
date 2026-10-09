## Part 4 — Business Entities and Concepts

### 4.1 Consumer account

**Purpose:** identify responsibility for reservations, collection, and feedback. **Information:** identity, age eligibility declaration, verified contact, account status, accepted terms, interests, restriction history, and transaction history. **Rules:** ownership is required for account actions; no duplicate benefit claims through multiple accounts. **Invariants:** one actual pickup is counted once; sanctions reference resolved evidence. **Relationships:** reservations, collector authorizations, reviews, reports, and optional future demand requests.

### 4.2 Supplier and establishment

**Purpose:** identify who offers surplus and who must fulfill it. **Information:** business identity, authorized owner, supplier type, contact, establishment name, opening hours, pickup points, category capability, declarations, approval status, and financial eligibility where needed. **Rules:** truthful identity and operational pickup capacity are required; online eligibility is separate from publication eligibility. **Invariants:** every offer has one accountable supplier and establishment. **Relationships:** offers, pickup points, plan enrollment, representatives, incidents, and reputation.

### 4.3 Pickup point

**Purpose:** fix the place of handover. **Information:** address, local time zone, approximate discovery location, access instructions, service intervals, and public-address policy. **Rules:** one offer uses one point; a reservation fixes that point. **Invariants:** exact address is accessible to the valid reservation holder; moving a point cannot silently move an accepted reservation. **Relationships:** establishment, offers, reservation terms. Future residential suppliers reveal only an approximate area before confirmation; a temporary payment commitment alone does not reveal a private address.

### 4.4 Offer

**Purpose:** represent one genuine surplus opportunity. **Information:** title, honest description, category, optional subcategory, surplus reason, condition, format, sellable-unit definition, quantity, price, currency, payment mode, booking and pickup intervals, safety limit where applicable, point, consumer cap, status, and policy version. Photos are useful but optional unless needed to substantiate disclosed damage. **Rules:** the declared quantity must exist; category and safety requirements must be met; only future uncommitted terms may change under Part 10. **Invariants:** no negative quantity, undefined unit, undisclosed compulsory charge, or retrospective change. **Relationships:** supplier, quantity position, reservations, price schedule, publication entitlement, and reports.

### 4.5 Quantity position

**Purpose:** explain the disposition of every admitted sellable unit. **Information:** cumulative admitted quantity; available, temporarily committed, confirmed reserved, handed-over, withdrawn, and expired quantities; adjustment reasons. **Rules:** each unit occupies exactly one current disposition. **Invariants:** the conservation identity in Part 5 always holds; release occurs once. **Relationships:** offer and quantity movements caused by reservations, pickup, cancellation, expiry, or safety withdrawal.

### 4.6 Reservation and protected terms

**Purpose:** define a consumer's exclusive claim and collection obligation. **Information:** consumer, offer, quantity, accepted price, total, payment mode, pickup point and interval, effective pickup deadline, condition and contents promise, cancellation/refund rules, policy version, timestamps, and outcome. **Rules:** terms are fixed at accepted commitment; confirmation grants collection entitlement; no automatic extension or restart. **Invariants:** one quantity claim and one transaction per reservation; neither another consumer nor a price change takes its allocation. **Relationships:** payment obligation, transaction, pickup, nominated collector, and outcome attribution.

### 4.7 Payment obligation and payment attempt

**Purpose:** distinguish what is owed from efforts to pay it. **Information:** reservation, expected amount and currency, permitted method, payer, named payee, financial cost allocation, deadline, attempt history, received amount, returned amount, and disputed amount. **Rules:** only substantiated receipt satisfies an online obligation; one live attempt at a time; retries stay within the original deadline. **Invariants:** receipts cannot settle multiple obligations; duplicate or surplus receipts are refundable excess. **Relationships:** reservation, refunds, financial case, and Asaas results for online payments.

### 4.8 Transaction

**Purpose:** join commercial outcomes without conflating pickup and money. **Information:** reservation terms, quantity outcomes, paid/refunded totals, final handover, cancellation responsibility, dispute status, fees, and closure condition. **Rules:** a pickup can be completed while a later financial dispute remains open. **Invariants:** financial correction never creates a second handover or erases the first. **Relationships:** reservation, pickup, payment obligation, refunds, and reviews.

### 4.9 Pickup and collector authorization

**Purpose:** verify one actual handover. **Information:** reservation entitlement, pickup confirmation reference, authorized collector, attempted time, actual quantity, payment acknowledgment where needed, both-party confirmation, and any exception evidence. **Rules:** verification alone is not handover; consumer acknowledges actual receipt; the supplier acknowledges release. **Invariants:** no pickup before confirmation, outside safety limits, or for more than reserved quantity. **Relationships:** reservation, transaction, and incident case.

### 4.10 Refund and financial case

**Purpose:** preserve a return obligation until the money is confirmed returned. **Information:** source payment, entitlement reason, approved amount, already returned amount, pending amount, responsible funder, execution status, evidence, and decisions. **Rules:** approval, submission, failure, and completion are separate facts; partial returns have explicit amounts. **Invariants:** cumulative returns and confirmed dispute recoveries never exceed original receipts for the same amount. **Relationships:** payment obligation, cancellation, partial pickup, and moderation findings.

### 4.11 Price schedule

**Purpose:** lower prices predictably as surplus urgency increases. **Information:** effective timestamps, ordered prices, accepted version, zero-price payment-mode rule, and activation or cancellation reason. **Rules:** stages are strictly increasing in time and non-increasing in price; no stage outside the booking interval. **Invariants:** past stages and accepted reservations remain historical facts. **Relationships:** offer and future commitment terms.

### 4.12 Category and eligibility declaration

**Purpose:** preserve product identity and category-specific disclosures. **Information:** primary category, optional subcategory, surplus reason, condition, provenance where relevant, safety declarations, and restricted attributes. **Rules:** the supplier selects one primary category; moderation corrects misclassification without altering accepted safety information. **Invariants:** free status does not override eligibility or safety. **Relationships:** offer, discovery, moderation, and demand matching.

### 4.13 Review and reputation summary

**Purpose:** report experience and observable reliability. **Information:** eligible transaction, author, recipient, rating, comments, submission time, publication status, completed pickups, resolved failures, and applicable observation window. **Rules:** one review per author per completed transaction; repeated dealings with one counterparty do not dominate reliability. **Invariants:** an allegation is not a confirmed failure; corrected attribution corrects indicators. **Relationships:** transaction, report, and moderation case.

### 4.14 Report and moderation case

**Purpose:** investigate suspected unsafe, misleading, abusive, or prohibited behavior. **Information:** reporter, target, reason, severity, evidence, provisional containment, assigned decision-maker, response, outcome, sanction, appeal, and rationale. **Rules:** protect reporter identity; consider relevant counterevidence; urgent safety containment can precede a final finding. **Invariants:** reports are allegations until resolved; sanctions have scope and review conditions. **Relationships:** offer, account, supplier, transaction, and financial remedy.

### 4.15 Plan enrollment and publication allowance

**Purpose:** control commercial entitlements without affecting accepted pickups. **Information:** supplier, plan, benefit version, period start/end, allowance, consumed count, pending plan change, billing obligation, and status. **Rules:** first publication consumes one allowance; editing, cancelling, or removal does not restore it; a genuinely new publication consumes another. **Invariants:** one published offer consumes allowance once; downgrade never cancels accepted commitments. **Relationships:** supplier, offers, recurrence entitlement, and future subscription payments.

### 4.16 Interests and alert eligibility

**Purpose:** identify useful nearby opportunities without excessive interruption. **Information:** categories, radius, location preference, favored establishments, free preference, price range, urgency settings, and opt-out. **Rules:** match active availability and preferences; do not notify the same person repeatedly about an unchanged opportunity. **Invariants:** private addresses are not disclosed through alerts. **Relationships:** consumer, offer, and future price-stage alerts.

### 4.17 Future demand request

**Purpose:** let a consumer declare wanted surplus. **Information:** category, description, desired unit/quantity, discovery area, radius, acceptable condition/price, validity, remaining demand, and status. **Rules:** match eligible offers; matching itself makes no reservation; quantities with different unit bases require clarification. **Invariants:** fulfilled demand cannot become negative or imply stock ownership. **Relationships:** consumer, matching offers, explicit accepted pickups.

### 4.18 Future recurrence template

**Purpose:** reduce repeated offer preparation. **Information:** supplier, category, description, weekdays, local time zone, expected intervals, default price, pickup point, and daily confirmation requirement. **Rules:** each occurrence needs supplier confirmation of actual quantity and current condition; a template is not available stock. **Invariants:** no phantom publication on a missed confirmation or closed day. **Relationships:** supplier, plan entitlement, and independently accountable occurrence offers.

### 4.19 Business history and impact statement

**Purpose:** explain decisions and measure substantiated outcomes. **Information:** actor, action, affected objects, prior/resulting states, quantity and money changes, times, reasons, evidence class, and policy version; impact unit and measurement basis. **Rules:** corrections append explanatory facts; estimates are labeled. **Invariants:** history cannot be rewritten to fabricate availability, savings, or pickup. **Relationships:** every consequential domain action and its metrics.

