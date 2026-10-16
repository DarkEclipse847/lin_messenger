# Authentication and cross-service production review

Reviewed 2026-10-15. This pass fixes verified source-level issues; it does not make
Lin production-ready. See also [the server review](../lin_server/PRODUCTION_REVIEW.md).

## Fixed findings

| Severity | Finding | Change |
| --- | --- | --- |
| Critical | An empty JWT secret was accepted, permitting predictable signing keys | Require at least 32 bytes at startup and in token helpers; operators must supply a randomly generated secret |
| High | JWT IDs decoded via float64 could change above 2^53, disagreeing with Go/Rust/PostgreSQL i64 identities | Decode typed int64 claims; reject fractional, overflowing and nonpositive IDs; test the maximum i64 through the gateway response |
| High | Same-user logins within a second generated identical tokens, so session replacement could fail to revoke the earlier token | Add a cryptographically random jti to each token |
| High | JWT parsing accepted other HMAC algorithms, optional expiry, and did not distinguish token purposes | Require HS256, expiry, a positive identity, jti and access purpose; refresh tokens cannot validate as access tokens |
| High | Rejected access tokens were written to logs | Remove token logging and return generic HTTP failures; parameterize ORM SQL logging to avoid recording hashes and tokens as query values |
| High | Registration accepted empty/malformed credentials and hashing errors were swallowed | Validate input before hashing/database work and propagate hashing failures |
| High | Login's OR query could match an unrelated empty field or choose between different supplied identities | Require exactly one login identifier and query only that field |
| High | Durable RabbitMQ queue held transient messages, and successful Publish did not prove broker acceptance | Publish persistent JSON, require publisher confirmation, detect mandatory returns, and bound publication with cancellation that closes the socket |
| Medium | Invalid Redis URLs silently fell back to an unintended local database; every call created a new pool | Fail on invalid configuration, reuse one pool and apply operation timeouts |
| Medium | Refresh cookies had no expiry; POST login/registration used permanent redirects | Match seven-day refresh expiry, explicitly set SameSite=Lax, use 303 and no-store responses, escape legacy usernames in redirect paths |
| Medium | An invalid Authorization header silently fell back to cookies | Explicit malformed credentials fail closed; valid bearer headers retain precedence |
| Medium | Redis outages were reported as expired sessions | Return 503 for store failures, 401 for absent/replaced sessions; neither failure forwards an identity |

## Shared contracts

- Both services preserve positive signed 64-bit user IDs. Connection IDs remain
  separate and must never be used as authenticated user IDs.
- The durable queue is `REGISTRATION_REQ`. A shared fixture at
  `contracts/registration_insert.json` tests Go publication and Rust consumption,
  including an ID above JavaScript's safe integer range. It contains exactly
  `interaction_type`, `id`, `username`, `email`; password serialization is excluded.
- Default broker URLs now explicitly match: the guest account on localhost with
  vhost `/` encoded as `%2F`. Deployment must supply its own broker credentials.
- Auth validates the access token against Redis before emitting `X-User-Id`.
  Rust trusts the gateway's overwritten header; positive-ID parsing is not authentication.
- Confirmed publication and consumer acknowledgement improve delivery handling,
  but do not make registration an atomic cross-service operation.

## Deployment compatibility

1. Configure a randomly generated JWT_SECRET of at least 32 bytes before starting
   auth. Never commit it. Existing JWTs lack purpose/jti and are deliberately rejected;
   users must sign in again. This does not terminate already-upgraded WebSockets.
2. Registration usernames must be 3–32 ASCII letters, digits or underscores;
   email must be a bare valid address up to 254 bytes; passwords must be 8–1024
   bytes. Passwords are not trimmed or normalized. Email case is preserved to avoid
   silently changing existing account lookup semantics.
3. Login accepts exactly one of email or username, with a nonempty password of at
   most 1024 bytes. Existing identifiers exceeding the new size limits require
   migration or an explicit compatibility policy before deployment.
4. Clients must accept 303 instead of 301. The browser flow still uses Secure,
   HttpOnly cookies and requires HTTPS. SameSite=Lax is not a complete CSRF policy.

## Remaining production blockers

- **Registration consistency:** User insertion, event publication, refresh storage
  and Redis session storage span independent systems. A broker outage can leave an
  auth-only account; a session write failure can leave an account created despite
  an unsuccessful HTTP response. Add an auth-owned transactional outbox, monitored
  retries and reconciliation. Keep direct cross-service database reads prohibited.
  Publishing confirms are not an outbox and do not prove Rust has created the profile.
- **Session consistency:** Concurrent logins can interleave PostgreSQL and Redis
  writes, producing mismatched access/refresh sessions. Define a session ID/version
  and serialize replacement or use a coherent session store before implementing
  refresh rotation. Refresh/logout endpoints, reuse detection and per-device sessions
  remain missing. Existing WebSockets outlive JWT expiry/revocation.
- **Abuse protection:** Argon2 costs 64 MiB per verification. Input/body limits do
  not bound concurrent work. Add per-account and gateway-aware rate limits, bounded
  hash concurrency, connection limits and abuse monitoring. User-not-found timing
  and registration conflict responses can disclose account existence.
- **Browser and account security:** Add a deliberate Origin/CSRF policy, email
  ownership verification, password recovery, and signing-key rotation. Restrict
  Rust to the trusted gateway and test header spoofing through the deployed proxy.
- **Operations:** Configure database connection/statement timeouts and pools,
  readiness checks for dependencies and the consumer, graceful shutdown, secret-free
  deployment templates, reviewed migrations, backups and CI. PostgreSQL operations
  still need request-scoped deadlines. The Rust consumer still stops on failure
  without reconnecting and requires operator recovery.
- **Private messaging:** Membership authorization, durable messages, history and
  offline delivery are still absent from lin_server. Auth fixes do not protect
  arbitrary `/join` access or remove the shared lobby.

## Validation scope

Regression tests cover token identity precision, purpose/algorithm/expiry checks,
unique credentials, password hashing, request rejection, cookie flags/expiry,
bearer precedence, Redis configuration failure, gateway identity forwarding and
session rejection, and the shared registration payload. The gateway handler tests
use an injected token lookup, not a live Redis server.

Run `go test ./...`, `go vet ./...`, `go build ./...` in lin_auth and
`cargo fmt --check`, `cargo check --locked`, `cargo test --locked` in lin_server.
All three Go checks passed with 11 top-level regression tests (plus table-driven
cases). All three Rust checks passed with seven tests; existing unused-code
warnings remain. No dependencies were added.

No live PostgreSQL, Redis, RabbitMQ, Nginx or two-client WebSocket integration flow
was exercised. Broker acknowledgements, reconnect/restart behavior, database races,
GORM migrations and end-to-end registration require disposable integration tests.
