# Reservations API (P03-01, typed command contract)

Reservation resources represent confirmed consumer commitments of whole offer
units with immutable accepted-term snapshots. Persistent IDs are native
PostgreSQL `uuidv7()` values. This task defines the schema and the typed
command; HTTP exposure follows in a later task.

## Immediate confirmation (UC-005, INV-030)

Launch reservations confirm immediately with no online payment state:

- `Command::ReserveNow` accepts `{ offer_id, quantity }` with whole units only.
- The resulting reservation has status `confirmed` for both `free` and
  `at_pickup` offers. No `awaiting_payment` state, payment attempt, Pix
  collection, refund, subscription or commercial allowance exists in launch.
- Free goods carry no payment obligation; priced goods are settled directly
  between participants at pickup.

## Bookability and permission

- Only `published` offers with an open booking interval
  (`booking_starts_at <= now < booking_ends_at`) and a live pickup interval
  accept reservations. Draft, scheduled, paused, closed, withdrawn or expired
  offers, and offers past their booking deadline, are rejected; stale
  displayed availability grants no reservation right.
- The consumer must satisfy `can_reserve`: adult, active, unrestricted
  consumer account. Consumers cannot reserve their own supplier's offers
  (INV-019); restricted or suspended accounts receive `403 Forbidden`.
- Supplier, establishment and pickup point must be eligible and active.
- Consumer caps from the active launch policy apply: per-offer cumulative
  quantity, total simultaneous commitments, and simultaneous free reservations.

## Conservation and contention (INV-004, INV-005, INV-006)

Each accepted unit moves atomically from `available_quantity` to
`committed_quantity` inside one explicit PostgreSQL transaction, preserving
the `conserved_inventory` balance. A commitment never exceeds current
availability; rejected requests allocate no quantity. Locks are taken
consumer-first: the actor row (`FOR UPDATE`) and `Authority::Consumer` are
acquired before supplier, offer and reservation authority, so cap checks for
one consumer serialize and parallel confirms on different offers cannot bypass
account-wide caps. The offer row lock then serializes concurrent attempts so
simultaneous requests for the last unit yield at most one acceptance.

## Immutable accepted snapshot (INV-020, INV-023)

Every reservation copies the accepted terms at acceptance time:

- `accepted_quantity` (always equals `quantity`), `accepted_price_cents`,
  `accepted_currency` (`BRL`), `accepted_condition`,
  `accepted_pickup_point_version`, `accepted_pickup_starts_at`,
  `accepted_pickup_ends_at`, `accepted_policy_version`.
- Database constraints reject non-positive quantities, negative prices,
  invalid states, inverted pickup intervals and dangling references.
- The `reservations_snapshot_immutable` trigger rejects any rewrite of the
  accepted terms, quantities, parties or deletion; only the lifecycle `status`
  may transition in later tasks.
- Later offer or pickup-point edits never touch stored snapshots, so new
  prices or terms never alter old commitments.

## Audit and replay (INV-060, INV-061)

Each confirmation records a `reservation_confirmed` audit fact with the prior
offer state, the resulting reservation, the available-quantity delta and the
known goods value (`money_delta_cents`). Idempotent `reserve_now` replay
returns the original reservation without double-committing stock.
