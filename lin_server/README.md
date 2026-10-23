# lin_server

Lin's pre-alpha WebSocket chat service uses Rust 2024, Actix Web/WS, Tokio,
Diesel/PostgreSQL, and Lapin/RabbitMQ. It manages live rooms in memory, persists
chat creation, and consumes user profiles from the separate authentication service.

## Architecture and authentication

`src/main.rs` starts the HTTP server on `0.0.0.0:8080`, a `ChatServer` task, and
a dedicated RabbitMQ consumer thread. `server.rs` owns sessions and room membership;
`ChatServerHandle` sends commands through MPSC channels and awaits oneshot replies.
`handler.rs` handles each WebSocket session and text commands. Synchronous chat creation and permission queries run through `web::block`.

`GET /chat` upgrades to WebSocket only when `X-User-Id` parses as a non-nil UUID.
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
| `/list` | Sends each persisted chat ID the authenticated user belongs to, including offline chats, in a separate text frame |
| `/join <positive i64>` | Joins an existing chat only if the authenticated user is a member; a denied join preserves the current room |
| `/create <title>` | Inserts a PostgreSQL chat with the authenticated user as admin, joins it, and replies `created chat <id> "<title>"` |
| `/add <chat ID> <user UUID>` | Admin only: adds a replicated user to the chat (idempotent) |
| `/remove <chat ID> <user UUID>` | Admin only: removes a member and evicts all their active connections from that chat (idempotent); the admin cannot remove themself |
| `/name <name>` | Sets a connection-local display prefix; does not change the user profile |
| Other text | Broadcasts to other connections in the current room; the sender receives no echo |

New connections start without a room. Create a chat or join one you belong to
before sending messages. Chat creation inserts both the chat and its creator's
membership in one transaction. Members may join and send/receive messages; only
`chats.admin` may manage membership. There is no role delegation, admin transfer,
or invitation acceptance workflow yet. `/add` grants access immediately.

The command processor binds each connection to the gateway-authenticated UUID.
It checks persisted permissions for joins, lists, and every broadcast (both sender
and recipients), and fails closed when the database is unavailable. Removal and
broadcasts are serialized by this processor. Frames already queued before removal
may still arrive. Live rooms disappear when empty or on restart; persisted
membership survives. Chat messages themselves are still not stored.

The membership migration backfills existing administrators only. Legacy chats
without an administrator remain inaccessible until explicitly repaired by an
operator; historical participation is not treated as authorization. Room `0` is
no longer a shared lobby. Errors use `!!! chat access denied`,
`!!! chat permissions unavailable`, or `!!! join a chat first`.

For example, Alice sends `/create project`, then `/add <chat ID> <Bob UUID>`.
Bob can now discover the chat with `/list`, join it, and exchange messages with
Alice. After Alice sends `/remove <chat ID> <Bob UUID>`, Bob cannot rejoin or
send/receive further broadcasts in that chat.

The server pings every five seconds and disconnects after more than ten seconds
without a client ping/pong. Frame size is limited to 128 KiB and aggregated
continuations to 2 MiB. Binary messages are logged and ignored.

## Persistence and registration events

The server uses its own PostgreSQL database with `users`, `chats`, `messages`,
and `chat_members` tables. It consumes the durable RabbitMQ queue `REGISTRATION_REQ`. JSON with
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

SQL migrations are tracked in `migrations/` and embedded into the server binary.
Diesel applies the baseline schema, UUID conversion, and chat membership migration
in version order. Docker copies this same directory.

| Variable | Meaning |
| --- | --- |
| `DATABASE_URL` | Required PostgreSQL URL for the server database |
| `RABBITMQ_URL` | Defaults to `amqp://guest:guest@localhost:5672/%2F`; the resolver normalizes trailing-slash URLs for the default vhost |
| `REDIS_URL` | Redis URL including credentials and database; defaults to `redis://localhost:6379/0` when unset or empty |
| `RUST_LOG` | Optional logging filter; defaults to `info` |

With variables exported, run from `lin_server/`:

```sh
cargo run --locked
```

Database connection setup also attempts to load `.env` through `dotenvy`, then
runs pending embedded migrations from `migrations/` in version order.
Permission queries open a database connection on a blocking worker without rerunning migrations;
connection pooling is not implemented.
See the [UUID migration guide](../USER_UUID_MIGRATION.md) before upgrading existing databases. Keep local secrets out of Git; `.env` is not
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

## Adding migrations

Use Diesel's standard migration generator from `lin_server/`:

```sh
diesel migration generate add_chat_description
```

This creates a directory under the configured `migrations/` root:

```text
migrations/<timestamp>_add_chat_description/
├── up.sql
└── down.sql
```

Write the schema change in `up.sql` and its reversal in `down.sql`. Commit both
files. Keep existing migration names and versions unchanged once applied; later
changes belong in a new migration. An irreversible migration should fail explicitly
on reversal and document its recovery procedure, as the UUID conversion does.

With `DATABASE_URL` pointing to disposable development PostgreSQL, apply and
inspect the change:

```sh
diesel migration run
```

The CLI updates `src/database/schema.rs` according to `diesel.toml`; review that
output and update the matching models. For a reversible migration, use
`diesel migration redo` on the disposable database to verify rollback and reapply,
then run the relevant regression tests. Rebuild the server after changing SQL,
since migrations are embedded at compile time.

See the [official Diesel migration guide](https://diesel.rs/guides/getting-started.html).

## Development checks

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
```

Targeted unit regression tests are included. Run database migration/permission
regressions against disposable PostgreSQL with:

```sh
LIN_TEST_DATABASE_URL=postgres://localhost/lin_test cargo test --locked database:: -- --ignored
cargo test --locked websocket_membership_flow -- --ignored
# Full PostgreSQL-backed WebSocket flow (disposable database only):
DATABASE_URL=postgres://localhost/lin_test LIN_TEST_DATABASE_URL=postgres://localhost/lin_test \
  cargo test --locked postgres_websocket_membership_survives_restart -- --ignored
```

Database tests isolate changes in transaction-scoped schemas and roll them back.
`websocket_membership_flow` uses real local TCP connections with an in-memory
membership store; `postgres_websocket_membership_survives_restart` uses PostgreSQL
and committed test profiles, cleaning them up after success. Use a disposable
database because a failed assertion may leave test data behind. Neither test
exercises the auth gateway or RabbitMQ. Missing native libraries can block compilation. For integration checks, register through auth,
confirm the replicated user exists, connect two clients through the gateway,
create a chat, add the second user, join the same room, and verify messages reach
the other client. Verify an uninvited user cannot join, list, or receive the chat,
then remove a member and verify all their connections lose access. Also check
that a gateway handshake without a valid session is rejected. An HTTP listener
alone does not prove the background registration consumer is healthy.

For schema changes, add SQL migrations, regenerate the Diesel schema using
`diesel.toml`, and update models together. See the repository [agent guide](../AGENTS.md)
for service boundaries and contribution checks.

See [the production review](PRODUCTION_REVIEW.md) for security blockers, changes
and the integration validation still required before deployment.
