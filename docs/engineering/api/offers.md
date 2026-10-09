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

## Status and exposure

Public routes are not exposed in task P02-01. Future microsteps will introduce:
- Typed draft creation with category disclosures and condition (P02-02).
- Publication with eligible pricing and pickup intervals (P02-03).
- Discovery queries with bounded radius and location privacy (P02-04).
- Owner-authorized quantity corrections and withdrawals (P02-05).
