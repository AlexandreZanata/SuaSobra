-- P02-01: Persist surplus identity and whole-unit buckets with database inventory conservation.
CREATE TABLE suasobra.offers (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    supplier_id uuid NOT NULL REFERENCES suasobra.suppliers(id),
    establishment_id uuid NOT NULL REFERENCES suasobra.establishments(id),
    pickup_point_id uuid NOT NULL REFERENCES suasobra.pickup_points(id),
    title varchar(255) NOT NULL CHECK (btrim(title) <> ''),
    description text NOT NULL DEFAULT '',
    category varchar(64) NOT NULL CHECK (btrim(category) <> ''),
    unit varchar(64) NOT NULL CHECK (btrim(unit) <> ''),
    status text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'paused', 'closed', 'withdrawn', 'expired')),
    admitted_quantity bigint NOT NULL CHECK (admitted_quantity >= 0),
    available_quantity bigint NOT NULL DEFAULT 0 CHECK (available_quantity >= 0),
    committed_quantity bigint NOT NULL DEFAULT 0 CHECK (committed_quantity >= 0),
    fulfilled_quantity bigint NOT NULL DEFAULT 0 CHECK (fulfilled_quantity >= 0),
    disposed_quantity bigint NOT NULL DEFAULT 0 CHECK (disposed_quantity >= 0),
    policy_version text NOT NULL DEFAULT '2026.2-free-launch' CHECK (btrim(policy_version) <> ''),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT conserved_inventory CHECK (admitted_quantity = available_quantity + committed_quantity + fulfilled_quantity + disposed_quantity)
);

CREATE INDEX offers_supplier ON suasobra.offers(supplier_id);
CREATE INDEX offers_establishment ON suasobra.offers(establishment_id);
CREATE INDEX offers_point ON suasobra.offers(pickup_point_id);
CREATE INDEX offers_status ON suasobra.offers(status);
