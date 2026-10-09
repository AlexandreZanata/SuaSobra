# ADR-001 — Minimal Initial Architecture

Status: Accepted baseline, consolidated after the owner-authorized repository cleanup on October 9, 2026. [The canonical stack](../stack.md) supersedes obsolete stack drafts. ADR-002 defines current launch scope; ADR-003 defines database/transaction boundaries; ADR-004 records the native frontend and clean history.

Use a Rust 1.99.0 stable modular monolith with explicit PostgreSQL command transactions. Build production executables with Cargo's release profile. PostgreSQL 18.6 owns persistent UUIDv7 generation. Pinned backend dependencies and Cargo locks provide reproducible builds.

The minimal frontend uses TypeScript 7.0.2, native HTML and native CSS with zero third-party application dependencies. Compile to browser ES modules; use native DOM/Fetch/browser APIs and same-origin session/CSRF contracts. No frontend framework or bundler is part of the baseline.

Keep one backend, one private database and one static web/reverse-proxy process on a small host. No permanent worker, broker, cache service, payment provider or mobile application. Local and main integration checks run the actual committed Rust suites against PostgreSQL. The private execution plan is not a build dependency.

Preserve canonical business definitions, immutable accepted terms, conservation, exclusive deadlines, ownership, append-only evidence and scoped command replay. Minimal architecture does not weaken business guarantees. Current implemented foundation/supplier behavior and remaining tasks are described in the engineering documents and private plan.
