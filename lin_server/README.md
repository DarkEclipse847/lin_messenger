# lin_server

Lin's pre-alpha WebSocket chat service uses Rust 2024, Actix Web/WS, Tokio,
Diesel/PostgreSQL, and Lapin/RabbitMQ. It manages live rooms in memory, persists
chat creation, and consumes user profiles from the separate authentication service.

## Architecture and authentication

`src/main.rs` starts the HTTP server on `0.0.0.0:8080`, a `ChatServer` task, and
a dedicated RabbitMQ consumer thread. `server.rs` owns sessions and room membership;
`ChatServerHandle` sends commands through MPSC channels and awaits oneshot replies.
`handler.rs` handles each WebSocket session and text commands. Synchronous chat
creation runs through `web::block`.

`GET /chat` upgrades to WebSocket only when `X-User-Id` parses as an `i64`.
The intended gateway flow is:

1. Nginx validates the caller's cookie or bearer token with
   `lin_auth` at `/api/auth/validate`.
2. Auth checks the JWT and the stored Redis access token, returning `X-User-Id`.
3. Nginx overwrites the upstream identity header with that validated value.
4. Rust uses this identity as the administrator when creating a chat.

The Rust service trusts the header and does not verify JWTs. Keep port 8080
private behind the gateway; supplying the header directly bypasses authentication.
Authentication is checked at the handshake, not again for each message.

## WebSocket protocol

Send plain text frames; this is not a JSON message protocol. Leading and trailing
whitespace is trimmed.

| Input | Behavior |
| --- | --- |
| `/list` | Sends each in-memory room ID in a separate text frame |
| `/join <u64>` | Leaves the current room and joins the given ID, creating an in-memory room if absent |
| `/create <title>` | Inserts a PostgreSQL chat with the authenticated user as admin, joins it, and replies `created chat <id> "<title>"` |
| `/name <name>` | Sets a connection-local display prefix; does not change the user profile |
| Other text | Broadcasts to other connections in the current room; the sender receives no echo |

New connections join shared room `0`. `/join`
does not check database existence or membership, and invalid numeric input
is rejected. Empty rooms are removed when their last connection leaves. Rooms and membership disappear on restart;
persisted chats are not automatically loaded into the room list.

The server pings every five seconds and disconnects after more than ten seconds
without a client ping/pong. Frame size is limited to 128 KiB and aggregated
continuations to 2 MiB. Binary messages are logged and ignored.

## Persistence and registration events

The server uses its own PostgreSQL database with `users`, `chats`, and `messages`
tables. It consumes the durable RabbitMQ queue `REGISTRATION_REQ`. JSON with
`interaction_type: "insert"`, `id`, `username`, and `email` creates a local user
with the auth service's ID. A user must exist here before chat creation can
satisfy the administrator foreign key.

The consumer acknowledges successfully persisted deliveries and handles identical
redeliveries idempotently, but does not reconnect after failures. Synchronization is incomplete. Chat messages
are broadcast only: the database message helper is not wired into the WebSocket
flow and uses its supplied `new_id` argument. Message history and attachments
are not implemented.

## Run locally

Use a Rust toolchain compatible with the locked dependencies and edition 2024;
the Docker builder uses Rust 1.94. Native builds need PostgreSQL client development
libraries (`libpq-dev` and `pkg-config` on Debian). PostgreSQL, Redis, and RabbitMQ must
be running, with a dedicated server database created.

**Migration prerequisite:** `src/database/db.rs` uses `embed_migrations!()`.
`migrations/` exists locally but is ignored by Git, so a fresh clone is missing
the SQL needed to build and initialize this service. Obtain the project's
migration files before building; do not infer a complete migration history from
the generated `src/database/schema.rs`. The Dockerfile also requires this directory.

| Variable | Meaning |
| --- | --- |
| `DATABASE_URL` | Required PostgreSQL URL for the server database |
| `RABBITMQ_URL` | Defaults to `amqp://guest:guest@localhost:5672/%2F`; the resolver normalizes trailing-slash URLs for the default vhost |
| `REDIS_URL` | Redis URL including credentials and database; defaults to `redis://localhost:6379/0` when unset or empty |
| `RUST_LOG` | Optional logging filter; defaults to `info` |

With migration files present and variables exported, run from `lin_server/`:

```sh
cargo run --locked
```

Database connection setup also attempts to load `.env` through `dotenvy`, then
runs pending embedded migrations. Keep local secrets out of Git; `.env` is not
currently ignored by the root ignore file. Redis setup lives in `controllers/redis_controller.rs`. Startup connects and checks
`PING` within three seconds, failing startup if Redis is unavailable. A shared async
connection manager is available as `web::Data<redis::aio::ConnectionManager>` to
HTTP handlers, with three-second connection/response timeouts and reconnection
support. Clone the manager for commands; no blocking connection or mutex is needed.
Setup does not read or modify auth's `access_token:<user_id>` strings (15-minute
TTL). Gateway/auth still own token validation.

The root local Compose setup coordinates the gateway, auth, server, two PostgreSQL
databases, Redis, and RabbitMQ using `docker compose up --build`. Compose and
`nginx/` are ignored and must be supplied separately on a fresh clone. The image
includes the server binary but no HTML UI: `/` tries to open `static/index.html`,
which is absent from the tracked repository. Use `/chat` with a WebSocket client.

## Development checks

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
```

Targeted unit regression tests are included. Missing migrations or native
libraries can block compilation. For integration checks, register through auth,
confirm the replicated user exists, connect two clients through the gateway,
create/join the same room, and verify messages reach the other client. Also check
that a gateway handshake without a valid session is rejected. An HTTP listener
alone does not prove the background registration consumer is healthy.

For schema changes, add SQL migrations, regenerate the Diesel schema using
`diesel.toml`, and update models together. See the repository [agent guide](../AGENTS.md)
for service boundaries and contribution checks.

See [the production review](PRODUCTION_REVIEW.md) for security blockers, changes
and the integration validation still required before deployment.
