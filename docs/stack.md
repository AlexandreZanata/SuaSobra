# Initial Project Stack

This is the canonical stack specification. The accepted launch scope remains [ADR-002](decisions/0002-free-simple-launch.md). Stack changes require an explicit owner instruction and an updated decision record.

## Backend

Use **Rust 1.99.0 stable**, pinned in root `rust-toolchain.toml`. Production builds use Cargo's **release** profile via `make build`; no nightly, beta or release-candidate toolchain. The existing modular monolith uses pinned Axum, Tokio, SQLx, Serde and Argon2 dependencies recorded in `backend/Cargo.toml` and `Cargo.lock`. The frontend dependency restriction does not remove these backend libraries.

Use **PostgreSQL 18.6**. Every persistent application primary key defaults to the database's native `uuidv7()`; foreign keys and HTTP resource IDs use UUIDs. No UUID extension or application-side persistent UUID generation.

Repository validation tools are Rust as well, in `tools/repo-check/`. No Python runtime, environment, package lock or script is required to build, validate or run this project.

## Frontend

Use **TypeScript 7.0.2**, plain **HTML** and native **CSS**. The owner's “7.02” notation is recorded as semantic version 7.0.2, a stable release. [Official TypeScript release](https://github.com/microsoft/typescript-go/releases/tag/typescript%2Fv7.0.2).

Frontend application code has **zero third-party dependencies**. Use native DOM, Fetch, URL, History and browser APIs. No UI framework, component library, CSS framework, preprocessor, bundler, client SDK or CDN package. The TypeScript compiler is a build-time tool; it is not an application dependency or a browser runtime. Verify and pin that exact compiler when the assigned web bootstrap task creates the frontend.

Compile strict TypeScript into JavaScript ES modules for the browser. Author semantic accessible HTML and ordinary `.css` files. Serve the generated JavaScript, HTML and CSS as static files through the same origin as the backend. No permanent frontend server or JavaScript runtime is needed in production.

Use typed API contracts with UUID strings, native credentialed Fetch, the backend's session/CSRF lifecycle and structured error responses. Render untrusted text safely through DOM text APIs. The browser displays authoritative backend terms, permissions and quantities; it never owns business decisions. Native browser test pages/harnesses may exercise real-backend journeys without adding frontend libraries.

## Infrastructure and current boundaries

Keep one backend executable, one PostgreSQL database and a static-file/reverse-proxy process on a small host. Docker/Compose and a future Caddy configuration are infrastructure tools, not frontend dependencies. No Redis, broker, permanent worker, financial provider or mobile app belongs in this initial stack.

The existing backend foundation/supplier features are implemented. The minimal native web is planned and has not been created by this stack declaration. See [architecture](engineering/architecture.md), [local development](engineering/local-development.md) and [test strategy](quality/test-strategy.md).
