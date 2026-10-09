# Fulfillment and Pickup Proofs API (P04-01, typed command contract)

Pickup proofs authenticate the collector at handover. A bare reservation UUID
never authorizes pickup: only a live secret bound to the reservation does.
This task defines authority and the proof primitives; the handover command
that consumes them follows in P04-02. No HTTP exposure yet.

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
