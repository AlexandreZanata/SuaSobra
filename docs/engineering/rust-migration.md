# Django to Rust and PostgreSQL 18.6 Migration

Read [ADR-003](../decisions/0003-rust-postgresql18.md). This is a source/runtime migration with a separately invoked data adoption command. Existing database data is never automatically rewritten when the Rust server starts.

## Preserved source and scope

The owner-authorized history cleanup moves the original implementation history into ignored `.local/backups/pre-clean-001.bundle`. The complete former backend, uncommitted work and private plan/progress are recoverable from `.local/backups/pre-clean-001-private.tar.gz`. These backups must not be committed. The active GitHub history starts from the reviewed Rust/native-web baseline. Rust ports scoped replay and transaction-local lock ordering instead of retaining Python thread-local behavior in an async runtime.

The migration ports the implemented foundation and supplier domain. Marketplace offers, reservations, handover, trust/reporting and minimal web are later tasks. Automatic email delivery is deferred according to ADR-002; the old token HTTP routes explicitly reject with `501 feature_deferred`, while hashed one-use token primitives remain tested. Current policy has no online-payment clock or monthly commercial quota.

## Rehearsal procedure

1. Record the original revision, database server version, schema and per-table counts privately. Stop application writes for a final cutover. Production writes/deployment require their task/runbook authorization; this repository conversion authorizes only local development work.
2. Keep the PostgreSQL 17 volume and make a private logical backup with `pg_dump` from that server. Do not print its contents or commit it. Back up globals/role configuration separately when applicable.
3. Provision an isolated PostgreSQL 18.6 database and restore the legacy dump into its `public` schema using a compatible `pg_restore`. Do not restore over a nonempty live target. See [PostgreSQL upgrading guidance](https://www.postgresql.org/docs/18/upgrading.html).
4. Set private `DATABASE_URL` to that isolated target. Run the new executable's migration and import:

   ```bash
   cargo run --locked --manifest-path backend/Cargo.toml -- migrate
   cargo run --locked --manifest-path backend/Cargo.toml -- import-django
   ```

5. Compare legacy table counts with `suasobra.legacy_records`, entity maps in `suasobra.legacy_ids` and active target table counts. Reconcile every account, supplier, establishment, point, event, history version, token, login attempt and outbox row. Check source rows have not changed. Test representative legacy passwords and participant/operations access using private test accounts.
6. Verify the API now returns UUID strings. Update local clients to mapped identifiers and discard old sessions/CSRF tokens. Request a new `/api/v1/auth/csrf/` token and log in. Run all gates on an isolated test database, rehearse restore/rollback, then obtain the required authorization for an actual production switch.

The importer requires the legacy `public.identity_user` table and an empty Rust target. Optional tables are adopted when present, supporting different previously completed phases. It acquires a transaction advisory lock, maps IDs using native `uuidv7()`, inserts all target rows and commits once. Any duplicate case-insensitive email, missing mapped foreign key, invalid constraint or unexpected row causes full rollback. It refuses to overwrite or repeat an import.

## Data treatment and compatibility

New `suasobra` tables use UUID primary/foreign keys. `legacy_ids` maps each source entity/type and original identifier to its new UUID. Original records are retained exactly in `legacy_records`; only restricted database operators may inspect these records because they include credentials/contact data. No HTTP route exposes the archive or ID map. Imported passwords stay hashed and verify with Django PBKDF2 or Argon2 compatibility; new registration uses Argon2id.

Supplier lifecycle, establishment and point configuration/history, supplier events, tokens, login-attempt counters and pending outbox rows are copied. Legacy audit facts and command replay records are retained exactly in the archive, including old policy/event/replay identities. They are intentionally not active replay records for the new UUID/API protocol. Imported history snapshots and outbox payloads preserve original references; resolve them through the map rather than rewriting historical evidence. No outbox dispatcher runs in this launch. Django framework metadata and session tables remain in `public`; sessions are not reused. Legacy login attempts retain their timestamps and identifier/IP failure counters, so a cutover does not reset an active lockout.

The importer handles the generated Django models, not arbitrary third-party schemas or a universal live upgrade. Its command reports only a total count, with no raw data or secrets. It does not execute `pg_dump`, `pg_restore`, production writes or a volume upgrade on the user's behalf.

## Rollback and verification

Before any newly accepted Rust writes, rollback can point the old executable at the unchanged PostgreSQL 17 source and discard only the isolated rehearsal target after review. After Rust accepts new writes, do not simply switch back: reconcile those new writes and obtain an explicit recovery decision to avoid data loss.

Rust migration checks prove PostgreSQL >=18.6, `DEFAULT uuidv7()` on every application ID, UUID version 7 on inserted resources and repeatable SQLx migration application. Tests exercise source preservation, legacy-to-new mapping, password compatibility, import refusal/rollback, permission rejection, point history, transactional audit and real concurrent replay. They do not prove a production restore rehearsal or a measured throughput figure.
