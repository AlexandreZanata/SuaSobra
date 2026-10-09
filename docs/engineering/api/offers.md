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

## Status and exposure

Public routes remain unexposed until subsequent publication and discovery microsteps:
- Publication with eligible pricing and pickup intervals (P02-03).
- Discovery queries with bounded radius and location privacy (P02-04).
- Owner-authorized quantity corrections and withdrawals (P02-05).
