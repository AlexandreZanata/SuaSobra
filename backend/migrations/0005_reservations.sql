-- P03-01: Persist confirmed reservations with immutable accepted-term snapshots.
-- Launch confirms free/at-pickup reservations immediately; no online payment state.
CREATE TABLE suasobra.reservations (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    offer_id uuid NOT NULL REFERENCES suasobra.offers(id),
    supplier_id uuid NOT NULL REFERENCES suasobra.suppliers(id),
    establishment_id uuid NOT NULL REFERENCES suasobra.establishments(id),
    pickup_point_id uuid NOT NULL REFERENCES suasobra.pickup_points(id),
    consumer_id uuid NOT NULL REFERENCES suasobra.users(id),
    quantity bigint NOT NULL CHECK (quantity > 0),
    status text NOT NULL DEFAULT 'confirmed'
        CHECK (status IN ('confirmed', 'cancelled', 'picked_up', 'expired')),
    -- Immutable accepted snapshot (INV-020, INV-023): copied at acceptance time.
    -- Later offer or pickup-point edits never rewrite these columns.
    accepted_quantity bigint NOT NULL CHECK (accepted_quantity > 0),
    accepted_price_cents bigint NOT NULL CHECK (accepted_price_cents >= 0),
    accepted_currency varchar(3) NOT NULL DEFAULT 'BRL' CHECK (accepted_currency = 'BRL'),
    accepted_condition text NOT NULL CHECK (btrim(accepted_condition) <> ''),
    accepted_pickup_point_version integer NOT NULL CHECK (accepted_pickup_point_version > 0),
    accepted_pickup_starts_at timestamptz NOT NULL,
    accepted_pickup_ends_at timestamptz NOT NULL,
    accepted_policy_version text NOT NULL CHECK (btrim(accepted_policy_version) <> ''),
    CONSTRAINT reservations_accepted_quantity_matches CHECK (quantity = accepted_quantity),
    CONSTRAINT reservations_accepted_pickup_order CHECK (accepted_pickup_starts_at < accepted_pickup_ends_at),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX reservations_offer ON suasobra.reservations(offer_id);
CREATE INDEX reservations_consumer ON suasobra.reservations(consumer_id);
CREATE INDEX reservations_supplier ON suasobra.reservations(supplier_id);
CREATE INDEX reservations_status ON suasobra.reservations(status);

-- Accepted snapshots and identity linkage are append-only (INV-020, INV-023):
-- status lifecycle transitions stay possible for later tasks, but the accepted
-- terms, quantities and parties can never be rewritten, and rows are never deleted.
CREATE FUNCTION suasobra.reject_reservation_snapshot_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'DELETE' THEN RAISE EXCEPTION 'Reservations are append-only'; END IF;
  IF OLD.offer_id IS DISTINCT FROM NEW.offer_id
     OR OLD.supplier_id IS DISTINCT FROM NEW.supplier_id
     OR OLD.establishment_id IS DISTINCT FROM NEW.establishment_id
     OR OLD.pickup_point_id IS DISTINCT FROM NEW.pickup_point_id
     OR OLD.consumer_id IS DISTINCT FROM NEW.consumer_id
     OR OLD.quantity IS DISTINCT FROM NEW.quantity
     OR OLD.accepted_quantity IS DISTINCT FROM NEW.accepted_quantity
     OR OLD.accepted_price_cents IS DISTINCT FROM NEW.accepted_price_cents
     OR OLD.accepted_currency IS DISTINCT FROM NEW.accepted_currency
     OR OLD.accepted_condition IS DISTINCT FROM NEW.accepted_condition
     OR OLD.accepted_pickup_point_version IS DISTINCT FROM NEW.accepted_pickup_point_version
     OR OLD.accepted_pickup_starts_at IS DISTINCT FROM NEW.accepted_pickup_starts_at
     OR OLD.accepted_pickup_ends_at IS DISTINCT FROM NEW.accepted_pickup_ends_at
     OR OLD.accepted_policy_version IS DISTINCT FROM NEW.accepted_policy_version
  THEN RAISE EXCEPTION 'Accepted reservation terms are immutable'; END IF;
  NEW.updated_at = now();
  RETURN NEW;
END $$;
CREATE TRIGGER reservations_snapshot_immutable BEFORE UPDATE OR DELETE ON suasobra.reservations
 FOR EACH ROW EXECUTE FUNCTION suasobra.reject_reservation_snapshot_mutation();
