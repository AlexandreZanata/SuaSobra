-- P02-03: Add booking and pickup intervals, payment mode, and scheduled status to offers
ALTER TABLE suasobra.offers
    ADD COLUMN booking_starts_at timestamptz,
    ADD COLUMN booking_ends_at timestamptz,
    ADD COLUMN pickup_starts_at timestamptz,
    ADD COLUMN pickup_ends_at timestamptz,
    ADD COLUMN payment_mode text NOT NULL DEFAULT 'at_pickup' CHECK (payment_mode IN ('free', 'at_pickup'));

ALTER TABLE suasobra.offers
    DROP CONSTRAINT IF EXISTS offers_status_check;

ALTER TABLE suasobra.offers
    ADD CONSTRAINT offers_status_check CHECK (status IN ('draft', 'scheduled', 'published', 'paused', 'closed', 'withdrawn', 'expired'));

CREATE INDEX offers_booking_window ON suasobra.offers(booking_starts_at, booking_ends_at);
CREATE INDEX offers_pickup_window ON suasobra.offers(pickup_starts_at, pickup_ends_at);
