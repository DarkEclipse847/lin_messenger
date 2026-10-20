# lin_auth

Authentication service for Lin, built with Go and Fiber v3. It owns credentials
in PostgreSQL, stores active access tokens in Redis, and publishes new user
identities to `lin_server` through RabbitMQ. This is pre-alpha software.

## Request flow

Registration hashes the password with Argon2id, creates the user through GORM,
with a random UUIDv4 identity, and publishes their profile to `REGISTRATION_REQ`. Registration and login then
issue an HS256 JWT pair: a 15-minute access token and a seven-day refresh token.
The access token is stored at Redis key `access_token:<user_id>`; a SHA-256 hash
of the refresh token is upserted in PostgreSQL. A later login replaces the stored
session for that user.

Both endpoints set `access_token` and `refresh_token` cookies with `HttpOnly`,
`Secure`, and path `/`, then return a **303 redirect** to `/<username>`. They do
not return a JSON token response. The access cookie expires after 15 minutes;
the refresh cookie expires after seven days. Both explicitly use SameSite=Lax.
Responses use Cache-Control: no-store.

| Method | Path | Input / result |
| --- | --- | --- |
| POST | `/api/register` | Body fields `username`, `email`, `password`; creates a user and session |
| POST | `/api/login` | `password` plus exactly one of `email` or `username`; creates a replacement session |
| Any | `/api/auth/validate` | Bearer token or `access_token` cookie; returns 200 with `X-User-Id`, 401 for an invalid session, or 503 for a session-store outage |

Validation prefers `Authorization: Bearer <token>` over the cookie; malformed
explicit authorization headers are rejected. It verifies the JWT and requires an exact match with Redis. Nginx uses
this endpoint for WebSocket handshake authorization and forwards `X-User-Id` to
the chat service. PostgreSQL is not queried during validation.

## Run locally

Use Go 1.25.0 or newer compatible with `go.mod`, plus running PostgreSQL, Redis,
and RabbitMQ instances. Create a dedicated auth database and supply your own
development credentials through the environment:

| Variable | Meaning |
| --- | --- |
| `DATABASE_URL` | Required PostgreSQL connection string accepted by GORM's PostgreSQL driver |
| `JWT_SECRET` | Required randomly generated secret of at least 32 bytes; shorter values prevent startup |
| `REDIS_URL` | Redis URL; defaults to `redis://localhost:6379/0` |
| `RABBITMQ_URL` | AMQP URL; defaults to `amqp://guest:guest@localhost:5672/%2F` |

From `lin_auth/`, with these variables exported:

```sh
go mod download
go run .
```

The service binds to `:8000`; `APPLICATION_PORT` does not change it. Startup
connects to PostgreSQL and converts legacy user IDs to UUIDs, then runs GORM `AutoMigrate` for `User` and `AuthCreds`.
See the [UUID migration guide](../USER_UUID_MIGRATION.md) before upgrading an existing deployment.
Environment variables must be exported explicitly; this service does not load
a `.env` file. Use HTTPS for the browser cookie flow because cookies are Secure.

The repository's local root Compose file can coordinate the services via
`docker compose up --build`, but it and the Nginx configuration are ignored by
Git and are not included in a fresh clone. See the [server setup notes](../lin_server/README.md)
for the additional migration prerequisite. Do not commit deployment credentials.

## Registration event

The Go publisher declares a durable queue named `REGISTRATION_REQ` and publishes
persistent JSON through RabbitMQ's default exchange using that queue name as the
routing key. It waits for publisher confirmation and checks for unroutable returns:

```json
{"interaction_type":"insert","id":"550e8400-e29b-41d4-a716-446655440000","username":"alice","email":"alice@example.test"}
```

The Rust consumer preserves this user ID in its own profile database. Passwords
are excluded from serialization. Keep event field names and types compatible
with `lin_server/src/controllers/models.rs`.

Publishing failures are logged but do not abort registration; the database write,
event publication, and session storage are not one transaction. A successful
registration therefore does not guarantee that the server profile already exists.

## Development

`routes/` wires endpoints, `controllers/` handles auth and RabbitMQ, `database/`
owns GORM operations, `models/` defines records, `redis/` stores access tokens,
and `utils/` contains password and JWT helpers.

```sh
go test ./...
go vet ./...
go build ./...
```

Targeted unit and HTTP handler tests are included. Integration checks require the
three backing services; verify registration, login, cookie/header validation,
and rejection of missing, expired, or replaced tokens. Refresh-token issuance
exists, but refresh and logout endpoints are not implemented.

## Hardening changes and compatibility

Tokens now require HS256, expiry, a non-nil UUID identity, a unique `jti`, and
an access-token purpose for validation. Existing tokens must be replaced by logging
in again. Registration validates usernames (3–32 ASCII letters/digits/underscores),
bare email addresses (up to 254 bytes), and passwords (8–1024 bytes). Login requires
exactly one identifier. The HTTP request-body limit is 16 KiB.

See [the production review](PRODUCTION_REVIEW.md) for verified bugs, cross-service
contracts, deployment compatibility, remaining blockers, and validation limits.
