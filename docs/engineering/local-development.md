# Rust Local Development

The backend is Rust with PostgreSQL 18.6 and native UUIDv7. Repository/documentation/private-plan validation also uses Rust; no interpreter runtime is required. See [the initial stack](../stack.md) for the future dependency-free native frontend.

## First run

Install the toolchain named by root `rust-toolchain.toml`, Docker and Compose. Then, from repository root:

```bash
cp infra/.env.example .env
docker compose -f infra/compose.local.yml up -d
make migrate
make run
```

The executable loads root `.env` when launched from root. `make migrate` embeds and applies committed SQL migrations; serving does not run migrations implicitly. Health: `http://127.0.0.1:8000/api/v1/health/`. Schema: `http://127.0.0.1:8000/api/v1/schema/`.

The local service uses `postgres:18.6-alpine`, loopback port `5448`, container `suasobra-postgres18` and named volume `suasobra_postgres18_data`. PostgreSQL 18's volume is mounted at `/var/lib/postgresql`. The previous `suasobra_postgres_data` volume is preserved; follow [the migration runbook](rust-migration.md) to adopt its data. Never reuse its raw 17 data directory as an 18 directory.

## Settings

`DATABASE_URL` is required. `TEST_DATABASE_URL` must target a database name ending in `_test`. The committed example uses local dummy credentials; replace them in private configuration outside development. `ENVIRONMENT` is development/test/production. `ALLOWED_HOSTS` lists explicit host[:port] values without wildcards. `LISTEN_ADDR` defaults to `127.0.0.1:8000`. `DB_MAX_CONNECTIONS` defaults to ten and must be 1..100. Use HTTPS at the reverse proxy in production; production cookies are always Secure.

Compose initializes `suasobra` and `suasobra_test` only when its new volume is first created. Tests require a local test role with CREATE DATABASE privilege; the isolated Compose development role supplies it. Runtime production roles should not have that privilege. Do not expose the database port publicly.

## Verification

`make docs` checks canonical IDs, catalog locations, acceptance mappings and links with the Rust tool in `tools/repo-check/`. Use `make docs-refresh` only when canonical definition lines move. `make plan` validates the ignored private execution plan; `make next` selects its next task. These private commands require the separately supplied `.local/` materials and are not CI prerequisites. Git is required for plan privacy checks.

```bash
make test-unit
make test-integration
make test-concurrency
make test-contract
make migrations-check
make verify
make build
```

`make test-task TEST_PATH=integration TEST_FILTER=pickup_privacy` runs a named Rust test target/filter. Required target files exist; missing suites or PostgreSQL fail rather than passing as skips. `make verify` checks docs, formatting and Clippy for both Rust packages, runs the repository tool's rejection/regression tests, and executes each backend suite once. Checker fixtures cover moved/missing/duplicate definitions, test mappings and evidence, broken links, invalid acceptance state, dependency order, incomplete delivery and private Git tracking. `make migrations-check` creates a clean database, applies/reapplies the locked migration and checks native UUID defaults and actual generated IDs.

Tests create randomly named `suasobra_rust_test_*` databases from the `_test` administration URL, use independent committed transactions and remove their own database after success. They never drop development databases/volumes. A panic may leave an isolated test database for diagnosis; inspect its name before manual cleanup. Do not point the test role at production.

## Runtime and maintenance

`make build` creates `backend/target/release/suasobra-backend`. The executable supports `serve`, `migrate`, `import-django` and `prune-auth`. Run the latter periodically from a host timer to delete expired sessions, obsolete login attempts and long-expired tokens. It does not change business state.

The backend image is built with `docker build -t suasobra-backend:local backend`. The runtime runs as UID/GID 65532. Database, reverse-proxy, backups and production deployment still need their explicitly assigned infrastructure tasks; no deployment is implied by an image build.

Stop/start the local service with Compose `stop`/`start`. Do not remove its named volume as part of routine development or automated tests.
