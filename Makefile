SHELL := /bin/bash
TEST_DATABASE_URL ?= postgres://suasobra:suasobra@127.0.0.1:5448/suasobra_test

.PHONY: docs docs-refresh plan next lint test-task test-unit test-integration test-concurrency test-contract migrations-check verify build run migrate

docs:
	cargo run --quiet --locked --manifest-path tools/repo-check/Cargo.toml -- docs

docs-refresh:
	cargo run --quiet --locked --manifest-path tools/repo-check/Cargo.toml -- docs --refresh

plan:
	cargo run --quiet --locked --manifest-path tools/repo-check/Cargo.toml -- plan

next:
	cargo run --quiet --locked --manifest-path tools/repo-check/Cargo.toml -- next

lint:
	cargo fmt --manifest-path tools/repo-check/Cargo.toml -- --check
	cargo clippy --locked --manifest-path tools/repo-check/Cargo.toml --all-targets -- -D warnings
	cargo fmt --manifest-path backend/Cargo.toml -- --check
	cargo clippy --locked --manifest-path backend/Cargo.toml --all-targets -- -D warnings

test-unit:
	cargo test --locked --manifest-path backend/Cargo.toml --test unit

test-integration:
	TEST_DATABASE_URL='$(TEST_DATABASE_URL)' cargo test --locked --manifest-path backend/Cargo.toml --test integration

test-concurrency:
	TEST_DATABASE_URL='$(TEST_DATABASE_URL)' cargo test --locked --manifest-path backend/Cargo.toml --test concurrency

test-contract:
	TEST_DATABASE_URL='$(TEST_DATABASE_URL)' cargo test --locked --manifest-path backend/Cargo.toml --test contract

test-task:
	@test -n "$(TEST_PATH)" || { echo 'TEST_PATH must name a Rust integration test target'; exit 1; }
	TEST_DATABASE_URL='$(TEST_DATABASE_URL)' cargo test --locked --manifest-path backend/Cargo.toml --test '$(TEST_PATH)' $(TEST_FILTER)

migrations-check:
	TEST_DATABASE_URL='$(TEST_DATABASE_URL)' cargo test --locked --manifest-path backend/Cargo.toml --test integration migration_schema

verify: docs lint
	cargo test --locked --manifest-path tools/repo-check/Cargo.toml
	TEST_DATABASE_URL='$(TEST_DATABASE_URL)' cargo test --locked --manifest-path backend/Cargo.toml --all-targets

build:
	cargo build --release --locked --manifest-path backend/Cargo.toml

run:
	cargo run --locked --manifest-path backend/Cargo.toml -- serve

migrate:
	cargo run --locked --manifest-path backend/Cargo.toml -- migrate
