# Surplus and Offers API

Offer resources represent genuine surplus items with conserved whole-unit inventory buckets. Persistent IDs are native PostgreSQL `uuidv7()` values.

## Inventory conservation

Every offer maintains five whole-unit quantity buckets governed by the database check constraint `conserved_inventory`:

$$\text{admitted\_quantity} = \text{available\_quantity} + \text{committed\_quantity} + \text{fulfilled\_quantity} + \text{disposed\_quantity}$$

- `admitted_quantity` (bigint $\ge 0$): Total admitted sellable whole units.
- `available_quantity` (bigint $\ge 0$): Currently available units for reservation.
- `committed_quantity` (bigint $\ge 0$): Currently committed units held by active reservations.
- `fulfilled_quantity` (bigint $\ge 0$): Successfully collected/handed-over units.
- `disposed_quantity` (bigint $\ge 0$): Withdrawn, expired, or cancelled units.

Database check constraints reject negative quantities on each individual bucket, fractional quantities via integer column types, and unbalanced bucket sums. Foreign keys strictly link each offer to its accountable `supplier_id`, `establishment_id`, and `pickup_point_id`.

## Typed draft creation (P02-02)

Drafts represent validated surplus declarations before publication. The typed command `Command::CreateDraft` enforces:

- **Canonical category:** Must be one of `food`, `fruit_vegetables`, `construction`, `wood_carpentry`, `paper_printing`, `packaging`, `agriculture`, `workshop_parts`, `textiles_offcuts`, `other_reusable`.
- **Sellable unit:** Discrete non-empty unit definition (e.g., `portion`, `kg`, `loaf`, `pack`, `unit`).
- **Exact whole quantity:** Discrete positive integer; fractional or negative quantities are rejected.
- **Substantiated classification (INV-016):** Genuine non-empty surplus reason (minimum 5 characters) explaining why goods are surplus.
- **Truthful condition:**
  - For `food`: `prepared_today`, `prepared_previously`, `near_expiry`, `shelf_stable`, `surprise_assortment`. Unsafe, spoiled or compromised goods are rejected.
  - For non-food: `fresh`, `surplus_production`, `cosmetic_defect`, `imperfect_packaging`, `day_end_clearance`, `clean_offcuts`, `usable_surplus`, `intact`.
- **Material disclosures (INV-063, INV-017):**
  - For `food`: Mandatory `storage_instructions`, `age_or_preparation`, future `expiry_or_best_before` timestamp, and `allergens_or_dietary` information. Free goods (`price_cents = 0`) must satisfy all safety and condition checks without exemption.
  - For non-food: Relevant `safe_handling`, `condition_notes`, or `packaging_integrity`.
- **Unknown fields & invented prices:** Strict serialization rejection (`deny_unknown_fields`) prevents unverified original prices or unsupported parameters.

## Offer publication (P02-03)

Offers transition from `draft` to `published` (or `scheduled`) via `Command::PublishOffer` or `POST /api/v1/offers/{id}/publish/`. Publication enforces:

- **Supplier standing (INV-018):** Supplier must be in `active` or `warned` standing. Pending, rejected, suspended, or closed suppliers are rejected.
- **Publication authority:** Outsiders without owner or moderator authority receive `403 Forbidden`.
- **Site eligibility:** Both the establishment and pickup point must be active.
- **Available whole-unit stock:** Admitted and available quantity must be positive whole units.
- **Interval eligibility:**
  - `booking_starts_at`: Defaults to UTC `now` if omitted; if in the future, the offer transitions to status `scheduled`.
  - `booking_ends_at`: Must be in the future and strictly after `booking_starts_at`.
  - `pickup_starts_at`: Must be strictly before `pickup_ends_at` and cannot precede `booking_starts_at`.
  - `pickup_ends_at`: Must be in the future and at or after `booking_ends_at`.
- **Hard food safety limit (INV-063, INV-017, INV-028):**
  - Unsafe or expired food is strictly rejected.
  - Neither booking deadline nor pickup end may exceed the hard safety limit (`expiry_or_best_before`).
- **Pricing and direct payment:**
  - `price_cents`: Zero (`payment_mode: "free"`) or fixed BRL cents (`payment_mode: "at_pickup"`).
  - Settled directly between participants at pickup without online payment obligations, subscriptions, or commercial quotas.
- **No price lock on display (INV-021):**
  - Viewing a published price or offer confers no stock priority or price lock; allocations require explicit accepted reservation commitments.

## Discovery and location privacy (P02-04)

Consumers and anonymous visitors discover nearby available surplus via `GET /api/v1/offers/discover/` (also `POST /api/v1/offers/discover/` and `/api/v1/discovery/`). Discovery enforces read-only spherical distance calculations with strict eligibility and privacy filters:

- **Query parameters:**
  - `latitude` (required, float $\in [-90.0, 90.0]$): User location latitude.
  - `longitude` (required, float $\in [-180.0, 180.0]$): User location longitude.
  - `radius_km` (optional, default `10.0`, range $[0.1, 50.0]$): Search radius in kilometers.
  - `category` (optional, string): Canonical surplus category filter (`food`, `wood_carpentry`, etc.). Empty string rejected with `400 Bad Request`.
  - `free_only` (optional, boolean): If `true`, restricts matches to zero-priced items (`price_cents = 0`).
  - `max_price_cents` (optional, unsigned integer): Maximum price filter in BRL cents.
  - `limit` (optional, default `20`, range $[1, 100]$): Bounded page size.
  - `offset` (optional, default `0`, unsigned integer): Bounded pagination offset.
- **Active and safety filters (UC-004):**
  - Offer must be in `published` status (drafts, scheduled, paused, closed, or withdrawn offers are excluded).
  - Positive available quantity (`available_quantity > 0`).
  - Currently bookable: `booking_starts_at <= now AND booking_ends_at > now`.
  - Live pickup window: `pickup_ends_at > now`.
  - Accountable supplier in `active` or `warned` standing.
  - Active establishment and pickup point (`is_active = true`).
  - Food safety filter: food offers must have future `expiry_or_best_before > now` and truthful condition.
- **Location privacy (INV-057):**
  - Pickup point documents in discovery and public detail views are sanitized via `public_point()`:
    - Exact GPS coordinates (`latitude`, `longitude`) are stripped.
    - Sensitive operational fields (`access_instructions`, `address_complement`, `establishment_id`) are stripped.
    - Private addresses (`is_private_address: true`) mask `address_street` as `"[Private location revealed after reservation]"` and blank `address_number`.
    - Distance is provided as an approximate estimate rounded to 0.1 km (`approximate_distance_km`), never exact coordinates or routing.
- **No price lock or priority (INV-021):**
  - Discovery and detail queries are purely read-only; no database inventory buckets are mutated, and viewing grants no reservation hold, allocation lock, or price freeze.
- **Deterministic ordering:**
  - Matches are stably ordered by `distance_km ASC, booking_ends_at ASC, id ASC`.
- **Public offer detail:**
  - `GET /api/v1/offers/{id}/`:
    - Public visitors and non-owner consumers receive active published offers with masked pickup points and establishment name.
    - Unpublished drafts, paused/closed offers, or offers on inactive sites return `404 Not Found` to outsiders.
    - Supplier owners and authorized moderators receive full unmasked point documents and draft access.

## Status and exposure

- Draft creation: `POST /api/v1/offers/` (P02-02).
- Publication: `POST /api/v1/offers/{id}/publish/` (P02-03).
- Individual offer retrieval: `GET /api/v1/offers/{id}/` (P02-03, updated P02-04).
- Supplier offers list: `GET /api/v1/suppliers/{id}/offers/` (P02-03).
- Nearby offer discovery: `GET /api/v1/offers/discover/` with bounded pagination and location privacy (P02-04).
- Future owner-authorized quantity corrections and withdrawals (P02-05).


