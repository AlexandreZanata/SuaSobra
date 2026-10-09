-- PostgreSQL owns persistent IDs. Keep legacy Django public tables untouched.
DO $$ BEGIN
  IF current_setting('server_version_num')::int < 180006 THEN
    RAISE EXCEPTION 'PostgreSQL 18.6 or newer is required';
  END IF;
END $$;
CREATE SCHEMA suasobra;
CREATE TABLE suasobra.users (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 email varchar(254) NOT NULL,
 password text NOT NULL,
 first_name varchar(150) NOT NULL DEFAULT '',
 phone_number varchar(32) NOT NULL DEFAULT '',
 is_email_verified boolean NOT NULL DEFAULT false,
 is_phone_verified boolean NOT NULL DEFAULT false,
 declared_adult boolean NOT NULL,
 is_active boolean NOT NULL DEFAULT true,
 standing text NOT NULL DEFAULT 'active' CHECK (standing IN ('active','warned','suspended','blocked','closed')),
 restriction text NOT NULL DEFAULT 'none' CHECK (restriction IN ('none','reservation_restricted','publication_restricted','online_payment_restricted','complete_suspension')),
 roles jsonb NOT NULL DEFAULT '["consumer"]' CHECK (jsonb_typeof(roles)='array'),
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX users_email_ci ON suasobra.users (lower(email));
CREATE TABLE suasobra.sessions (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 token_hash text NOT NULL UNIQUE,
 csrf_hash text NOT NULL,
 user_id uuid REFERENCES suasobra.users(id),
 expires_at timestamptz NOT NULL,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_expiry ON suasobra.sessions(expires_at);
CREATE TABLE suasobra.login_attempts (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 identifier text NOT NULL,
 ip_address inet NOT NULL,
 successful boolean NOT NULL,
 attempt_time timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX login_identifier_time ON suasobra.login_attempts(identifier,attempt_time) WHERE NOT successful;
CREATE INDEX login_ip_time ON suasobra.login_attempts(ip_address,attempt_time) WHERE NOT successful;
CREATE TABLE suasobra.identity_tokens (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 user_id uuid NOT NULL REFERENCES suasobra.users(id),
 token_hash text NOT NULL UNIQUE,
 purpose text NOT NULL CHECK (purpose IN ('email_verification','password_reset')),
 expires_at timestamptz NOT NULL,
 used_at timestamptz,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE suasobra.suppliers (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 owner_id uuid NOT NULL REFERENCES suasobra.users(id),
 trade_name varchar(255) NOT NULL CHECK (btrim(trade_name)<>''),
 legal_name varchar(255) NOT NULL DEFAULT '',
 tax_id varchar(32) NOT NULL CHECK (btrim(tax_id)<>''),
 business_type text NOT NULL CHECK (business_type IN ('bakery','restaurant','grocery','cafe','hotel','other')),
 contact_email varchar(254) NOT NULL,
 contact_phone varchar(32) NOT NULL,
 standing text NOT NULL DEFAULT 'pending_approval' CHECK (standing IN ('pending_approval','active','rejected','warned','suspended','blocked','closed')),
 financial_eligibility text NOT NULL DEFAULT 'disabled',
 rejection_reason text NOT NULL DEFAULT '',
 reapplication_conditions text NOT NULL DEFAULT '',
 warning_reason text NOT NULL DEFAULT '',
 suspension_reason text NOT NULL DEFAULT '',
 suspension_review_date timestamptz,
 closure_reason text NOT NULL DEFAULT '',
 closure_date timestamptz,
 created_at timestamptz NOT NULL DEFAULT now(),
 updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX supplier_owner ON suasobra.suppliers(owner_id,standing);
CREATE TABLE suasobra.supplier_events (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 supplier_id uuid NOT NULL REFERENCES suasobra.suppliers(id),
 actor_id uuid REFERENCES suasobra.users(id),
 event_type text NOT NULL,
 prior_standing text NOT NULL DEFAULT '',
 resulting_standing text NOT NULL,
 reason text NOT NULL,
 evidence text NOT NULL DEFAULT '',
 metadata jsonb NOT NULL DEFAULT '{}',
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX supplier_events_order ON suasobra.supplier_events(supplier_id,created_at,id);
CREATE TABLE suasobra.establishments (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 supplier_id uuid NOT NULL REFERENCES suasobra.suppliers(id),
 name varchar(255) NOT NULL CHECK (btrim(name)<>''),
 description text NOT NULL DEFAULT '',
 is_active boolean NOT NULL DEFAULT true,
 created_at timestamptz NOT NULL DEFAULT now(),
 updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX establishment_supplier ON suasobra.establishments(supplier_id);
CREATE TABLE suasobra.pickup_points (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 establishment_id uuid NOT NULL REFERENCES suasobra.establishments(id),
 -- Strict typed command validation owns this versioned location document.
 data jsonb NOT NULL CHECK (jsonb_typeof(data)='object'),
 version integer NOT NULL DEFAULT 1 CHECK (version>0),
 is_active boolean NOT NULL DEFAULT true,
 created_at timestamptz NOT NULL DEFAULT now(),
 updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX point_establishment ON suasobra.pickup_points(establishment_id);
CREATE TABLE suasobra.pickup_point_history (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 pickup_point_id uuid NOT NULL REFERENCES suasobra.pickup_points(id),
 version integer NOT NULL CHECK (version>0),
 actor_id uuid REFERENCES suasobra.users(id),
 snapshot jsonb NOT NULL,
 change_reason text NOT NULL CHECK (btrim(change_reason)<>''),
 created_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(pickup_point_id,version)
);
CREATE TABLE suasobra.audit_facts (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 entity_type text NOT NULL,
 entity_id text NOT NULL,
 action text NOT NULL,
 actor_id uuid REFERENCES suasobra.users(id),
 actor_scope text NOT NULL DEFAULT '',
 authority_or_policy text NOT NULL CHECK (btrim(authority_or_policy)<>''),
 evidence_kind text NOT NULL,
 reason text NOT NULL DEFAULT '',
 effective_at timestamptz NOT NULL DEFAULT now(),
 recognized_at timestamptz NOT NULL DEFAULT now(),
 prior_state jsonb NOT NULL DEFAULT '{}',
 resulting_state jsonb NOT NULL DEFAULT '{}',
 prior_quantity bigint,
 resulting_quantity bigint,
 quantity_delta bigint,
 money_delta_cents bigint,
 payload jsonb NOT NULL DEFAULT '{}',
 metadata jsonb NOT NULL DEFAULT '{}',
 redacted_fields jsonb NOT NULL DEFAULT '[]',
 correction_of uuid REFERENCES suasobra.audit_facts(id),
 idempotency_key text UNIQUE,
 CHECK ((prior_quantity IS NULL AND resulting_quantity IS NULL AND quantity_delta IS NULL)
  OR (prior_quantity IS NOT NULL AND resulting_quantity IS NOT NULL
   AND prior_quantity>=0 AND resulting_quantity>=0
   AND quantity_delta=resulting_quantity-prior_quantity
   AND btrim(reason)<>'' AND (actor_id IS NOT NULL OR btrim(actor_scope)<>'')))
);
CREATE INDEX audit_entity ON suasobra.audit_facts(entity_type,entity_id,recognized_at);
CREATE TABLE suasobra.outbox (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 topic text NOT NULL CHECK (btrim(topic)<>''),
 event_type text NOT NULL CHECK (btrim(event_type)<>''),
 payload jsonb NOT NULL,
 headers jsonb NOT NULL DEFAULT '{}',
 status text NOT NULL DEFAULT 'pending',
 scheduled_for timestamptz NOT NULL DEFAULT now(),
 processed_at timestamptz,
 attempts integer NOT NULL DEFAULT 0 CHECK (attempts>=0),
 last_error text NOT NULL DEFAULT '',
 idempotency_key text UNIQUE,
 created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE suasobra.command_replays (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 actor_id uuid NOT NULL REFERENCES suasobra.users(id),
 command_name text NOT NULL,
 idempotency_key varchar(128) NOT NULL CHECK (btrim(idempotency_key)<>''),
 input_digest text NOT NULL,
 response_payload jsonb NOT NULL,
 result_code integer NOT NULL CHECK (result_code BETWEEN 200 AND 299),
 created_at timestamptz NOT NULL DEFAULT now(),
 UNIQUE(actor_id,command_name,idempotency_key)
);
-- Migration mappings make legacy integer/UUID references discoverable without changing them.
CREATE TABLE suasobra.legacy_ids (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 entity_type text NOT NULL,
 legacy_id text NOT NULL,
 new_id uuid NOT NULL,
 UNIQUE(entity_type,legacy_id)
);
CREATE TABLE suasobra.legacy_records (
 id uuid PRIMARY KEY DEFAULT uuidv7(),
 source_table text NOT NULL,
 legacy_id text NOT NULL,
 record jsonb NOT NULL,
 UNIQUE(source_table,legacy_id)
);
CREATE FUNCTION suasobra.reject_history_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'History is append-only'; END $$;
CREATE TRIGGER audit_immutable BEFORE UPDATE OR DELETE ON suasobra.audit_facts
 FOR EACH ROW EXECUTE FUNCTION suasobra.reject_history_mutation();
CREATE TRIGGER point_history_immutable BEFORE UPDATE OR DELETE ON suasobra.pickup_point_history
 FOR EACH ROW EXECUTE FUNCTION suasobra.reject_history_mutation();
CREATE TRIGGER supplier_event_immutable BEFORE UPDATE OR DELETE ON suasobra.supplier_events
 FOR EACH ROW EXECUTE FUNCTION suasobra.reject_history_mutation();
