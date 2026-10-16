# Production readiness review

Reviewed 2026-10-15. This is a source review and focused hardening pass, not a
production certification. The current service must not host private conversations.

## Release blockers

1. **Critical: no conversation authorization** (`handler.rs`, `server.rs`).
   `/join` accepts an arbitrary room ID and creates it in memory; `/list` exposes
   all active rooms. Every connection enters a shared lobby. A logged-in attacker
   can join another user's conversation. Add persisted membership with roles,
   invitation/removal rules and authorization for listing, joining, sending and
   history. Carry authenticated user identity into the command processor; connection
   IDs are not identities. Remove automatic lobby membership for private messaging.
   This needs a tracked SQL migration and coordinated protocol design; checking
   only chat existence or restricting access to admins would not implement membership.
2. **Critical deployment boundary: trusted identity header** (`main.rs`). Direct
   access permits impersonation using `X-User-Id`. Restrict the listener to the
   authenticated gateway using deployment network policy and test spoofed headers
   through that gateway. Positive-ID validation does not authenticate a caller.
   Session revocation/expiry after upgrade and browser Origin policy also need design.
3. **High: no durable delivery** (`handler.rs`). Messages are broadcast in memory,
   with no persisted send, acknowledgement, retry deduplication, history or offline
   delivery. Persist authorized sends before acknowledging them; define client message
   IDs, ordering and reconnect/resume behavior. Display names are freely chosen and
   must not be presented as verified sender identities.
4. **High: incomplete registration recovery** (`rabbitmq_controller.rs`, `main.rs`).
   Successful events are now acknowledged after an idempotent transaction. Database
   failure stops consumption and leaves the delivery unacknowledged for redelivery
   when the channel closes. There is still no reconnect supervisor or readiness
   signal; operators must restart the service after repairing failure. Invalid
   events are rejected without requeue and are lost unless a dead-letter exchange
   is configured. Auth publishing remains best effort: use an auth-owned outbox,
   monitored retries, dead letters and reconciliation.
5. **High: unbounded aggregate work** (`server.rs`, `handler.rs`). Per-client queues
   are bounded now, but the actor command queue, total sessions, incoming command
   rate and database concurrency are not. A 64-entry client queue with 2 MiB messages
   can still consume substantial memory. Add byte budgets, rate/admission limits,
   database pooling/timeouts, and socket write deadlines. Slow clients are evicted
   when a queue fills; buffered messages may drain before their socket closes.
6. **High: builds and deployment are not reproducible**. Migrations, gateway and
   Compose configuration are ignored. Supply reviewed, secret-free deployment
   templates and tracked migrations through a deliberate repository change. Do not
   infer migration history from schema.rs. Add CI using disposable PostgreSQL and
   RabbitMQ, backups/restore drills, observability and load/failure testing.

## Changes made

- Reject malformed and out-of-range room IDs instead of panicking.
- Exit WebSocket handling on read errors, EOF or failed writes and unregister the
  session; remove raw frame logging to avoid logging private message bodies.
- Bound each outgoing queue to 64 entries; remove closed/overloaded recipients
  without blocking broadcasts. Remove empty rooms, reject joins by stale connection
  IDs, avoid connection-ID collisions and clean up cancelled connect requests.
- Acknowledge registration only after database success; prefetch at most 16 events.
  Redelivery with the same ID and identity is a no-op. Conflicting identity data
  fails instead of overwriting a profile. Stop logging broker payloads/user records.
- Include the supplied ID in message inserts; generated IDs are strictly positive.
- Remove the database URL from connection panic messages, reject nonpositive
  handshake identities and return an HTTP error for a missing index file.
- End the command processor when HTTP exits and fail the process if the processor
  stops, avoiding the previous indefinite join during shutdown.

## Verification and limits

Targeted unit tests cover room-ID parsing, handshake identity parsing, room
isolation, empty-room cleanup, stale joins, slow-recipient eviction and registration
payload validation and a shared Go/Rust registration fixture. All seven tests,
formatting and locked compilation passed locally;
compilation reports unused-code warnings.
Run `cargo fmt --check`, `cargo check --locked` and `cargo test --locked`.
Database-backed registration redelivery, transaction conflicts, broker disconnects,
full WebSocket/gateway flows, shutdown under load and delivery recovery require
integration tests; the unit tests do not establish those guarantees.

See [the auth review](../lin_auth/PRODUCTION_REVIEW.md) for coordinated JWT identity
and registration publisher fixes and the remaining cross-service failure modes.
