-- P02-02: Add surplus classification, condition, and safety disclosures to offers
ALTER TABLE suasobra.offers
    ADD COLUMN surplus_reason text NOT NULL DEFAULT '',
    ADD COLUMN condition text NOT NULL DEFAULT 'fresh',
    ADD COLUMN safety_disclosures jsonb NOT NULL DEFAULT '{}' CHECK (jsonb_typeof(safety_disclosures) = 'object'),
    ADD COLUMN price_cents bigint NOT NULL DEFAULT 0 CHECK (price_cents >= 0),
    ADD COLUMN currency varchar(3) NOT NULL DEFAULT 'BRL' CHECK (currency = 'BRL');
