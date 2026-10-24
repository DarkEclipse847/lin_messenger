# Repository guide for agents

## Purpose and principles

Lin is an early, pre-alpha messenger aiming to be fast, secure, and convenient.
Its microservice-like structure separates authentication, messaging, and the
cross-platform client so they can evolve independently. Preserve those boundaries
and prefer focused changes over speculative infrastructure or broad rewrites.
The root README explicitly asks contributors to personally review generated code:
inspect the implementation, verify behavior, and report limitations honestly.

## Repository map

| Directory | Responsibility | Stack |
| --- | --- | --- |
| `lin_auth/` | Credentials, login, registration, session validation | Go, Fiber v3, GORM, PostgreSQL, Redis |
| `lin_server/` | WebSocket connections, rooms, chat records, replicated user profiles | Rust 2024, Actix Web/WS, Tokio, Diesel, PostgreSQL, Lapin |
| `lin_client/` | Future desktop/mobile messenger UI; currently the greeting template | React 19, TypeScript, Vite, Tauri 2 |
| `nginx/` | Local gateway configuration; not tracked | Nginx |

There is no root build workspace. Run commands in the relevant service directory.
Read the service READMEs and actual source before changing behavior; root TODOs
include proposals, not implemented contracts.

## Architectural contracts

- Auth owns credentials and its PostgreSQL database. The server owns a separate
  PostgreSQL database for profiles, chats, and messages. Do not introduce direct
  cross-service database reads as a shortcut.
- Registration publishes JSON to the durable RabbitMQ queue `REGISTRATION_REQ`:
  `interaction_type`, `id`, `username`, and `email`. The implemented interaction is
  `insert`. Passwords are excluded by the Go model's JSON tag. Coordinate payload
  changes between `lin_auth/models/user.go` and
  `lin_server/src/controllers/models.rs`.
- The auth-generated user ID is preserved in the server database. Connection IDs
  and room IDs are `u64`; persisted user IDs are UUIDs, while chat/message IDs remain `i64`. A connection
  ID is not an authenticated user ID.
- Auth signs JWTs with `JWT_SECRET`. Access tokens expire after 15 minutes and must
  match Redis key `access_token:<user_id>`; refresh tokens expire after seven days
  and only their SHA-256 hashes are persisted in PostgreSQL. There is currently
  one stored access token and refresh-token record per user, not per device.
- The gateway validates `/chat` handshakes through `/api/auth/validate`, then
  overwrites `X-User-Id` with the validated identity. Rust trusts that header and
  does not validate JWTs itself. Keep the server private behind this gateway;
  never treat an arbitrary public request header as proof of identity.
- `ChatServer` owns in-memory sessions and rooms in a single command-processing
  task. Handlers use `ChatServerHandle` with MPSC and oneshot channels. Preserve
  this ownership model when extending room behavior.
- Diesel is synchronous. Keep database work off async request workers using
  `web::block` or an appropriate dedicated worker, as chat creation already does.
  The registration consumer runs on a separate thread with its own runtime.

## Working conventions

- Keep Fiber route wiring in `routes/`, auth logic in `controllers/`, database
  operations in `database/`, and password/JWT helpers in `utils/`.
- Keep WebSocket protocol handling in `handler.rs`, room/session state in
  `server.rs`, broker integration in `controllers/`, and persistence in `database/`.
- Use `gofmt` for changed Go files and Rust formatting for changed Rust code.
  Avoid unrelated formatting churn in this unfinished codebase.
- Treat `lin_server/src/database/schema.rs` as Diesel-generated. Database changes
  need SQL migrations, matching models, and regenerated schema, not just edits to
  the generated file. Auth runs its embedded legacy user-ID migration before GORM `AutoMigrate` on startup.
  The server applies all tracked SQL migrations from `lin_server/migrations/` in version order.
- Generate new server migrations from `lin_server/` with
  `diesel migration generate <descriptive_snake_case_name>`. Keep the generated
  timestamped directory under `migrations/`, with `up.sql` applying the change and
  `down.sql` reverting it. Do not introduce separate migration roots or rename
  already applied migration versions. Add a new migration for subsequent changes.
  If reversal is impossible, make `down.sql` fail explicitly and document recovery.
  Validate against disposable PostgreSQL, regenerate `schema.rs`, and update models.
- Preserve Argon2id password hashing, hashed refresh-token storage, and HTTP-only,
  Secure cookies. Do not log credentials or tokens, or copy local secrets into
  documentation, fixtures, or tracked configuration.
- Change dependencies only when needed; keep existing `go.sum` and Cargo lockfile
  consistent. Do not add a new frontend package manager as incidental cleanup.
- Add targeted regression tests for behavior changes, especially authentication,
  command parsing, and broker contracts. Go and Rust regression tests are checked
  in, including a shared registration fixture in `contracts/registration_insert.json`.
  No CI pipeline is currently checked in; a successful compilation alone does not
  verify these flows.

## Validation commands

Run applicable checks from the listed directory; report what ran and any blockers.

| Directory | Checks |
| --- | --- |
| `lin_auth/` | `go test ./...`, `go vet ./...`, `go build ./...` |
| `lin_server/` | `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` |
| `lin_client/` | `npm run build` after installing frontend dependencies |
| `lin_client/src-tauri/` | `cargo check` with platform-specific Tauri dependencies installed |

For integration changes, exercise registration/login, validation through the
gateway, RabbitMQ profile creation, and a two-client WebSocket room exchange as
applicable. Use disposable development data. Do not claim these checks passed
unless dependencies were available and the flows were actually exercised.

## Local setup and current limitations

- Root `compose.yaml` and `nginx/` are not tracked. They exist in this working
  copy but are not delivered in a fresh clone. Do not force-add local deployment
  files or secrets. Server SQL migrations are tracked in `lin_server/migrations/`;
  the generated `.diesel_lock` file is not part of the migration history.
- Rust embeds migrations at compile time and its Dockerfile copies them. Missing
  migrations block reproducible builds; `schema.rs` is not a substitute for SQL.
- Go listens on port 8000; Rust listens on 8080. `APPLICATION_PORT` is not read by
  Go. Rust connects to Redis using `REDIS_URL` at startup and shares an async connection
  manager with HTTP workers; token validation remains in auth.
- The Rust `/` handler expects `static/index.html`, which is not checked in or
  copied into its image. It is not a usable bundled chat UI or health check.
- Live rooms are in memory. `/create` persists a chat and creator membership;
  `/join`, `/list`, and message delivery enforce persisted membership. Messages
  are saved before broadcast. History and reconnect recovery are not implemented.
- Registration publishing remains best effort despite persistent messages and
  publisher confirms. The Rust consumer acknowledges successful persistence,
  accepts matching duplicate registrations, and rejects invalid events without
  requeueing. Persistence failures stop the consumer with the delivery unacknowledged;
  there is no automatic reconnect/retry loop. Do not assume reliable profile
  synchronization or atomic registration across services.
- Refresh/logout endpoints, message history, end-to-end encryption, S3 attachment
  integration, and a functional messenger client are not implemented. Keep such
  roadmap items distinct from current capabilities.
