# Decision Log

[ADR-001](0001-minimal-architecture.md) defines the minimal initial architecture, consolidated around [the canonical stack](../stack.md).

[ADR-002](0002-free-simple-launch.md) defines the free launch, no platform payments/returns, minimum infrastructure and CI only for integration PRs targeting main.

[ADR-003](0003-rust-postgresql18.md) defines Rust/PostgreSQL 18.6, native UUIDv7 and deliberate legacy data adoption boundaries.

[ADR-004](0004-native-web-clean-baseline.md) specifies native TypeScript 7.0.2/HTML/CSS, zero frontend dependencies, native repository tools and owner-authorized clean history.

Business rules remain canonical in docs/domain/. Decisions must explain context, affected guarantees and verification rather than creating competing business definitions.
