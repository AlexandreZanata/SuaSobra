# Fulfillment and Pickup Proofs API (P04-01, typed command contract)

Pickup proofs authenticate the collector at handover. A bare reservation UUID
never authorizes pickup: only a live secret bound to the reservation does.
P04-02 adds the handover command that consumes them, exposed at
`POST /v1/reservations/:id/pickup` (body `{ "secret": "<proof>" }`).

## Collector and owner scope

- The reservation holder owns the entitlement and is the only party that may
  issue a proof (`Command::IssuePickupProof`). Outsiders and the supplier
  owner are refused with `403`.
- The collector is the holder in person (`holder` scope) or exactly one
  nominated third party (`nominated` scope): an existing active adult account
  in good standing. The supplier can never be nominated to collect its own
  offer, and there is no advanced representative management in launch.
- Proofs require a `confirmed` reservation with a live pickup window; past
  the accepted pickup end no new proof is issued.

## Protected proof (INV-057, INV-007)

- At issuance a random 32-byte secret is shown exactly once; only its SHA-256
  hash is persisted, bound to the reservation with the accepted pickup window
  as its expiry. Identical retries return the sealed original response rather
  than minting a second proof.
- Verification (`verify_pickup_proof`) is read-only and undifferentiated:
  wrong, expired, already-used, or cross-reservation secrets all fail alike,
  so proofs are not an oracle.
- Redemption (`consume_pickup_proof`) is one atomic conditional update for
  the handover transaction: concurrent redemptions of one secret yield a
  single success.
- Proof rows are append-only evidence: no deletion, no field rewrites, and
  the single-use mark flips from NULL exactly once.
- A valid proof reveals only the required handover data — proof and
  reservation IDs, collector ID and first name, scope, quantity and
  condition. Contact details and addresses are never included.

## Final handover (P04-02, INV-007/INV-061)

- `Command::CompletePickup` runs one transaction: lock the reservation, then
  the offer, then consume the proof. It requires a `confirmed` (or
  `partially_committed`) reservation inside its pickup window, the caller to
  be the supplier owner (actor scope `owner`), and a live single-use secret
  bound to that reservation. Wrong, expired, used, or cross-reservation
  secrets fail alike with `403`, so unknown clients cannot forge completion.
- The handover moves the reservation to `fulfilled`, consumes the committed
  units once, writes one `reservation_events` row (`handed_over`) and one
  `reservation_commands` row (`CompletePickup`) under the same idempotency
  key, and releases no units back to the offer. The direct goods price stays
  a goods value, not provider-confirmed receipt.
- Concurrent duplicate handovers yield one final pickup: the losers meet the
  already-`fulfilled` guard or the already-consumed proof and fail without a
  second unit effect or a second event.

## Partial handover (P04-03, INV-001/INV-010/INV-011)

- `Command::CompletePartialPickup` hands over a positive whole-unit count
  strictly below the accepted quantity, so a single-unit reservation stays
  indivisible and must use the full handover. The caller declares the
  terminal remainder outcome: `released` (genuine surplus back to the offer
  for re-offer) or `disposed` (perished or unsafe units to the disposed
  bucket). Proof, buckets, `picked_up` status, audit (`partial_pickup_completed`)
  and replay move in one transaction; conservation holds across all four
  buckets. Exposed at `POST /v1/reservations/:id/pickup/partial` with
  `{ "secret", "handover_quantity", "remainder" }`.
- The accepted unit price stays fixed; only handed-over units count as goods
  value and no refund is processed. After a partial handover the reservation
  is terminal: duplicates, replays with fresh keys, and late full handovers
  all fail without reviving handed-over inventory.

## Failed attendance (P04-04, INV-060/INV-061/INV-062)

- `Command::ReportNoShow` records supplier evidence that an `expired`
  reservation saw no handover: owner (or moderator) only, with a factual
  readiness statement. The declaration opens an exact 24-hour challenge
  window (`challenge_until`), applies no penalty, moves no quantity, and
  changes no status. Live, picked-up, handed-over, and already-declared
  reservations are refused. Exposed at
  `POST /v1/reservations/:id/attendance/report` with `{ "statement" }`.
- `Command::ChallengeNoShow` lets the holder contest within the window, in
  any account standing — restricted or closed participants retain case
  access. Outsiders, late challenges, and second challenges are refused.
  Exposed at `POST /v1/reservations/:id/attendance/challenge`.
- `Command::ResolveAttendance` is a moderator determination
  (`no_show`/`supplier_failure`/`neutral` with rationale). A `no_show`
  outcome requires the window to have elapsed; exoneration may resolve
  earlier. Appeals supersede only as linked `correction_of` resolutions with
  full history and no repeated quantity, money, or penalty effect. Exposed at
  `POST /v1/reservations/:id/attendance/resolve`.
