# ADR-004 — Native Web and Clean Repository Baseline

Status: Accepted owner instruction. Date: October 9, 2026. Supersedes earlier frontend framework/tooling proposals and obsolete repository history. Product scope remains ADR-002; backend/database remain ADR-003.

## Decision

The canonical initial stack is [Rust 1.99.0 stable, PostgreSQL 18.6, TypeScript 7.0.2, native HTML and native CSS](../stack.md). Production Rust builds use `--release`. The frontend has zero third-party application dependencies and compiles to browser JavaScript ES modules. Its only required language build tool is the pinned TypeScript compiler. No frontend framework, bundler, CSS library or client SDK is authorized.

Replace the repository's documentation and private-plan check scripts with a small Rust tool, retaining stable-definition, acceptance mapping, links, dependency ordering and private-file checks. Remove obsolete interpreter/test/lint caches and recoverable build output. Preserve functional Rust code and existing database data; housekeeping is not authorization to discard business behavior or erase database volumes.

## History cleanup

The owner explicitly requested removal of the old implementation commits from GitHub. Preserve a verified Git bundle and private legacy materials under ignored `.local/backups/`, then publish the reviewed current tree as a new root commit. Make `main` the GitHub default and remove the obsolete phase branches so their old ancestors are absent from the active advertised history. Check exact remote references before replacement and use leases when removing the old branches. Never use a mirror push or rewrite unrelated refs.

This is a one-time initial-history cleanup authorized by the owner, not permission for future agents to rewrite published work or bypass normal integration. Subsequent microsteps use scoped commits, local phase gates/pushes and main-targeting PR CI. Provider-retained unreachable objects, caches and other clones are outside active Git branch history; do not claim a provider-wide erasure.

## Verification

Run documentation validation, Rust formatting/Clippy, all real PostgreSQL tests and the release build. Exercise the replacement checker against deliberately invalid documentation/catalog/test mappings and private plan state. Verify no maintained `.py` files or obsolete framework setup remains. Keep `.local/` untracked. Compare the pushed clean root with the exact local revision and verify the GitHub default/branch inventory after delivery.
