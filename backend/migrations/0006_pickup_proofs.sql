-- P04-01: Hashed expiring single-use pickup proofs scoped to reservations.
-- The secret is shown once at issuance; only its SHA-256 hash rests here.
-- A reservation UUID alone never authorizes pickup (INV-057).
CREATE TABLE suasobra.pickup_proofs (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    reservation_id uuid NOT NULL REFERENCES suasobra.reservations(id),
    collector_id uuid NOT NULL REFERENCES suasobra.users(id),
    scope text NOT NULL CHECK (scope IN ('holder', 'nominated')),
    proof_hash text NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    created_by uuid NOT NULL REFERENCES suasobra.users(id),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX pickup_proofs_reservation ON suasobra.pickup_proofs(reservation_id);
CREATE INDEX pickup_proofs_collector ON suasobra.pickup_proofs(collector_id);

-- Proofs are append-only evidence: no deletion, no field rewrites, and the
-- single-use mark can only go from NULL to set, exactly once (INV-007).
CREATE FUNCTION suasobra.reject_pickup_proof_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' THEN RAISE EXCEPTION 'Pickup proofs are append-only'; END IF;
  IF OLD.reservation_id IS DISTINCT FROM NEW.reservation_id
     OR OLD.collector_id IS DISTINCT FROM NEW.collector_id
     OR OLD.scope IS DISTINCT FROM NEW.scope
     OR OLD.proof_hash IS DISTINCT FROM NEW.proof_hash
     OR OLD.expires_at IS DISTINCT FROM NEW.expires_at
     OR OLD.created_by IS DISTINCT FROM NEW.created_by
     OR (OLD.used_at IS NOT NULL AND OLD.used_at IS DISTINCT FROM NEW.used_at)
  THEN RAISE EXCEPTION 'Pickup proofs are immutable'; END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER pickup_proof_immutable BEFORE UPDATE OR DELETE ON suasobra.pickup_proofs
 FOR EACH ROW EXECUTE FUNCTION suasobra.reject_pickup_proof_mutation();
