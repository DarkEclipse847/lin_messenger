# Authentication readiness

This document describes current safeguards and remaining gaps. It is not a
production certification or a record of a completed integration test run.
See the [auth README](README.md) for endpoints, configuration, and local checks.

## Current safeguards and contracts

- Passwords use Argon2id. Registration validates usernames, email addresses, and
  password length; login accepts exactly one of email or username.
- JWT validation requires HS256, expiry, a non-nil UUID user identity, a unique
  token identifier, and access-token purpose. The signing secret must be at least
  32 bytes. Access tokens must match Redis; refresh tokens are stored as hashes.
- Cookies are Secure, HttpOnly, and SameSite=Lax. Login and registration return
  303 redirects with Cache-Control: no-store.
- Explicit malformed authorization headers are rejected. Invalid sessions return
  401; session-store failures return 503. Neither failure forwards an identity.
- Auth and the server preserve the same UUID identity. Connection IDs are separate.
  The [shared registration fixture](../contracts/registration_insert.json) covers
  `interaction_type`, `id`, `username`, and `email`; passwords are excluded.
- Registration events use the durable `REGISTRATION_REQ` queue, persistent
  messages, publisher confirms, and mandatory-return checks. These do not make
  registration atomic or guarantee that the server has created the profile.

## Remaining gaps

- User creation, publication, refresh storage, and Redis session storage span
  independent systems. Publication is best effort; failures can leave auth-only
  accounts. There is no transactional outbox or automated reconciliation.
- Concurrent logins can interleave PostgreSQL and Redis session writes. There is
  one stored session per user, with no refresh/logout endpoints, refresh rotation,
  reuse detection, or per-device session management. Upgraded WebSockets are not
  terminated when tokens expire or are replaced.
- Input limits do not bound concurrent password hashing. Rate limits, bounded
  hashing concurrency, email verification, password recovery, signing-key rotation,
  and a deliberate browser Origin/CSRF policy remain missing.
- Deployment needs dependency readiness checks, database deadlines, recovery
  procedures, backups, and CI. Local Compose and gateway configuration are untracked.
- The server enforces chat membership and persists messages, but history, offline
  delivery, and end-to-end encryption are not implemented. See the
  [server readiness notes](../lin_server/PRODUCTION_REVIEW.md).

## Validation scope

Regression tests cover UUID claims, token purpose/algorithm/expiry, unique tokens,
password hashing, request validation, cookie settings, bearer precedence, Redis
configuration, gateway identity responses, and registration serialization.
Handler tests use an injected token lookup rather than a live Redis server.

Run `go test ./...`, `go vet ./...`, and `go build ./...` from `lin_auth/`.
These checks do not establish end-to-end behavior. With disposable services,
exercise registration/login, Redis session replacement, gateway authorization,
RabbitMQ profile creation, and failure/restart recovery. Report results from the
actual run rather than relying on historical test counts.
