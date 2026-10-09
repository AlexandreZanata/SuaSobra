# Authentication API

Runtime: [ADR-003](../../decisions/0003-rust-postgresql18.md). Prefix `/api/v1/auth/`. Responses are JSON. IDs are database-generated UUIDv7 strings. Session/CSRF secrets are opaque random values, distinct from resource IDs.

## CSRF and session lifecycle

`GET csrf/` returns `{"csrf_token":"<opaque-secret>"}` and sets `sessionid` (HttpOnly) and `csrftoken` cookies. Send cookies and `X-CSRFToken` on every write, including anonymous registration/login. Repeating bootstrap rotates secrets while preserving the current authenticated user. Login/registration rotate them again; refresh the frontend's token from the new cookie/bootstrap before its next write. SameSite=Lax and Path=/ are always set; production adds Secure. No secret is persisted unhashed or returned in a user profile.

`GET session/` returns `{"authenticated":false,"user":null}` or the authenticated user summary. An expired/inactive session is anonymous. `POST logout/` requires valid CSRF, deletes the database session and clears both cookies; it returns `{"status":"ok"}`. Old session cookies cannot authenticate after login rotation/logout.

## Registration and login

`POST register/` accepts only email, password, declared_adult, optional first_name and optional phone_number. Email is normalized to lowercase; password length is 8..128 characters; declared_adult must be boolean true. Profile limits are 150 characters for first_name and 32 for phone_number. Unknown fields, roles, standing and verification overrides are rejected. Success is 201 and an authenticated user summary; case-insensitive duplicate email is 409. Contact is declared, with verification flags false; operations qualify suppliers under ADR-002.

`POST login/` accepts only email/password. Success is 200 with the summary and rotated cookies. Unknown email and invalid password share a generic 401. Five failed attempts in fifteen minutes by identifier or direct peer IP cause 429 before another password attempt. Counters and locks live in PostgreSQL across processes. Forwarded-IP headers do not override the peer. Password CPU work is bounded and runs outside the async executor; saturation also returns 429.

A user summary contains id, username (normalized email), email, first_name, phone_number, is_email_verified, is_phone_verified, declared_adult, standing, restriction and roles. It never contains a password hash or token. Empty legacy roles imply consumer. Role changes and standing are reloaded for commands; a stale session cannot retain removed moderator authority.

## Deferred delivery

`POST verify-contact/request/`, `verify-contact/confirm/`, `password-reset/request/` and `password-reset/confirm/` return structured `501 feature_deferred` after CSRF validation. Automatic delivery/recovery is deferred by ADR-002; these routes do not claim to send mail or create tokens. The underlying Rust hashed/expiring/purpose-bound single-use token services remain tested and imported token records are preserved. Enabling delivery requires a future scoped task, abuse limits and actual notification configuration.

## Errors and compatibility

See [error envelope](errors.md). CSRF/origin failures return 403; unauthenticated resource reads return 401. Login/registration require a bootstrap token before input processing. Django PBKDF2 and Argon2 hashes verify after deliberate data import. Django session cookies cannot authenticate against the Rust backend. UUID identifiers replace numeric legacy IDs; consult [the migration runbook](../rust-migration.md).
