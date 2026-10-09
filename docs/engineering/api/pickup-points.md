# Pickup Point API

All routes require a session. Writes require CSRF, owner/moderator authority and the shared transactional command service. Resource IDs are UUID strings. Optional `Idempotency-Key` follows [supplier replay semantics](suppliers.md).

## Creation and queries

`GET /api/v1/establishments/{establishment_id}/pickup-points/` lists active points. `POST` creates one with 201. Required writable fields: name, address_street, address_number, address_neighborhood, address_city, address_state (two letters), address_postal_code, latitude and longitude. Optional: address_complement, time_zone (default America/Sao_Paulo), access_instructions, service_intervals (default []), is_private_address (default false).

Coordinates accept JSON decimal numbers or decimal strings with at most six fractional digits; latitude is -90..90 and longitude -180..180. NaN, booleans, null and out-of-range values are rejected. Responses normalize exact coordinates to six-decimal strings and public approximations to two-decimal strings, rounding half-even. Time zones must be IANA identifiers. Service intervals contain only day_of_week (integer 0=Sunday..6=Saturday), start_time/end_time (HH:MM), with start strictly before end; overnight service needs separate intervals.

`GET /api/v1/pickup-points/{point_id}/` returns full detail to owner/moderator. Other authenticated actors receive only public fields: id, supplier_id, establishment_name, name, street/number subject to privacy, neighborhood/city/state/postal code, time zone, approximate coordinates, service intervals, privacy flag, version and active flag. Exact latitude/longitude, complement, access instructions and private timestamps are excluded. Private addresses replace street with a disclosure message and number with an empty string.

Full detail includes all location fields plus establishment_id, supplier_id, establishment_name, version, is_active, created_at and updated_at. Creating a point commits version 1 and its history snapshot atomically. Closed suppliers cannot create/update points. Legacy private-address behavior remains preserved even though advanced residential/multi-site UI is deferred.

## Versioned edits and history

`PATCH /api/v1/pickup-points/{point_id}/` accepts a subset of creation fields, optional boolean is_active, and required nonblank change_reason. Both coordinates must be supplied together. Empty required address/name values, unknown fields, direct version/state IDs and invalid typed values are rejected. Success 200 increments version exactly once and commits full snapshot, actor/reason and audit in the same transaction. Replaying an identical keyed edit does not add another version; conflicting key reuse is 409.

`GET /api/v1/pickup-points/{point_id}/history/` requires owner/moderator and returns up to 100 snapshots in descending version order. Database constraints protect unique (point,version), and triggers reject history updates/deletes. Public point queries never expose this private history. A failed history write rolls back the point, audit and replay result together. The future reservation service must keep its own accepted snapshot; a later point edit cannot rewrite accepted terms.
