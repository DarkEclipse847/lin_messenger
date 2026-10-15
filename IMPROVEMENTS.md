# Lin improvement plan

Updated 2026-10-16 (Europe/Moscow), after rebuilding and exercising the local stack.
This plan combines source inspection with the live checks below. Passing a small
integration test does not establish production readiness or private messaging safety.

## What already works

The local auth and server images were rebuilt from the working tree with fresh
base images and an initial no-cache build. After disk cleanup, the failed runtime
packaging stage was retried using the newly compiled layers. All seven services
started; both PostgreSQL health checks passed.

Live checks through Nginx verified:

- Registration and login return 303; valid session cookies validate with 200.
- Missing/invalid credentials and replaced sessions return 401; malformed auth
  request bodies return 400. A supplied identity header does not bypass gateway auth.
- Registration creates the corresponding profile in the server database.
- Two independently authenticated clients upgrade to WebSocket with 101, create
  a persisted chat, join it, and exchange messages in both directions.
- Unicode payloads arrive intact, senders receive no echo, and messages do not
  reach a connection after it switches to another room.
- Ten consecutive messages arrive in order in this single-sender test.
- Both clients respond to heartbeat pings and can exchange messages after a
  12-second wait. Disconnect notifications and reconnect/rejoin delivery work.

**Limits:** these checks used a local HTTP gateway and disposable accounts, not
browser/Tauri clients or HTTPS. They did not test malicious conversation access,
concurrent send ordering, message recovery, broker redelivery, outages, load,
backup restoration, or session revocation on an already-open socket. Room isolation
means routing behaved correctly; it does not mean room membership is authorized.
Unit suites were not rerun as part of these live checks.

The probe scripts were temporary files under `/tmp`, not committed regression
coverage. Test data remains: `probe_ba912494c727`, `exchange_a_23a6b5133b`,
`exchange_b_23a6b5133b`, and chat `3199676397010502462`. Review and remove these exact
records and associated sessions when no longer needed; do not clear shared volumes.

## Priority 1: before hosting private conversations

### Enforce conversation membership

**Current:** `/join` accepts arbitrary room IDs, `/list` returns active rooms, and
connections automatically enter room 0. The command processor tracks connection
IDs, not authenticated membership. A successful gateway handshake alone does not
permit access to every conversation.

**Improve:** add persisted chat membership and roles in the server database, with
reviewed SQL migrations and matching models/schema. Carry authenticated user IDs
alongside connection IDs. Authorize listing, joining, sending, history, invitations
and removal. Remove automatic shared-lobby entry for private chats. Preserve the
single-task ownership of rooms and sessions.

**Done when:** a third authenticated nonmember cannot discover or join a private
chat, read its history, or send into it; a removed member loses access on an existing
connection. Test legitimate invitation and rejoin paths too.

Source: [handler.rs](lin_server/src/handler.rs), [server.rs](lin_server/src/server.rs).

### Complete the session and gateway security model

**Current:** the gateway validates the handshake; Rust trusts its `X-User-Id`.
Open sockets are not revalidated on token expiry or session replacement. Refresh
and logout routes are absent, and concurrent login writes span PostgreSQL and Redis.
The local gateway serves HTTP although auth cookies are Secure.

**Improve:** keep Rust inaccessible to untrusted clients, configure HTTPS for the
browser flow, define an allowed-Origin/CSRF policy, and decide how expiry/logout
terminates active sockets. Introduce a coherent session ID/version before adding
refresh rotation and reuse detection. Define whether sessions are per-device or
single-user; do not accidentally change that contract. Add email verification,
password recovery and a signing-key rotation procedure before public signup.

**Done when:** browser login works over HTTPS; disallowed origins and forged
identity headers fail; logout/expiry/replacement affect existing sockets according
to a documented policy; concurrent login/refresh tests cannot produce inconsistent
access and refresh sessions. Keep credentials owned by auth.

Source: [auth controller](lin_auth/controllers/authController.go),
[server entry point](lin_server/src/main.rs).

## Priority 2: reliable messaging and registration

### Define a durable message protocol

**Current:** text frames are broadcast in memory. The message insert helper is
not called by the WebSocket handler. There is no durable acknowledgement, history,
resume cursor or retry deduplication. `/name` is a freely chosen display prefix,
not verified sender identity.

**Improve:** define versioned message/ack/error events with authenticated sender
identity, client message ID, server message ID and an ordering/resume strategy.
Persist authorized sends before acknowledging success. Add cursor-based history
and idempotent retry, then reconnect catch-up. Represent i64 IDs as strings in a
JavaScript-facing JSON protocol to avoid precision loss above 2^53. Coordinate
protocol changes with the client rather than silently replacing text commands.

**Done when:** a committed message survives server restart, an uncertain send can
be retried without duplicates, offline recipients catch up, and history access is
membership-checked. Specify ordering under multiple senders; the ten-message test
only established ordering for one connection.

Source: [handler.rs](lin_server/src/handler.rs),
[database helpers](lin_server/src/database/db.rs).

### Recover registration synchronization automatically

**Current:** publisher confirms and idempotent consumer persistence help, but auth
registration still succeeds when profile publication fails. The consumer can stop
while the HTTP service remains up. Invalid events need a configured dead-letter
route to remain available for inspection.

**Improve:** write an outbox event in the auth user-creation transaction, deliver
it with monitored retries, and supervise consumer reconnection with bounded backoff.
Add dead-letter handling and an explicit reconciliation procedure. Reuse broker
connections where appropriate. Do not query the other service's database to repair
normal request handling.

**Done when:** registration during a broker outage eventually creates exactly one
matching server profile after recovery; duplicate events are harmless; conflicting
or invalid events are inspectable; loss of the consumer changes readiness and
raises an operational signal.

Source: [auth publisher](lin_auth/controllers/rabbitmqController.go),
[server consumer](lin_server/src/controllers/rabbitmq_controller.rs).

## Priority 3: reproducible development and operations

| Improvement | Evidence / reason | Acceptance check |
| --- | --- | --- |
| Deliver secret-free deployment templates and reviewed migrations | Compose, Nginx configuration and Rust migrations exist locally but are ignored; a fresh clone cannot reproduce this stack | A fresh checkout builds and boots using documented placeholder configuration and disposable volumes; no local credentials are committed |
| Turn live probes into integration tests and CI | Successful manual scripts currently live only in `/tmp` | CI runs Go/Rust checks and gateway registration, replication and two-client exchange with readiness polling, isolated data and automatic cleanup |
| Separate liveness from readiness | HTTP can remain available after consumer failure; `/` returns 404 and is not a health endpoint | Health endpoints have documented semantics; dependency/consumer failures affect readiness without unnecessary liveness restart loops |
| Pool and bound database work | Chat creation opens a new synchronous connection and runs migration checks through `establish_connection` | Run migrations in a controlled startup/deploy step; use bounded pools, statement/connection timeouts and blocking workers; DB outages return controlled errors |
| Bound resource use | The actor command queue is unbounded; frame limits and a 64-message outbound queue are not total memory limits | Add admission/rate limits, queue byte budgets, bounded DB/hash concurrency and write deadlines; slow-client and burst tests keep memory bounded |
| Protect authentication from abuse | Argon2 verification is intentionally expensive | Account/IP-aware throttling and bounded hashing work resist concurrent attempts without blocking ordinary users; trust only gateway-controlled forwarding headers |
| Improve Docker build discipline | Disk reached 100%, causing misleading APT signature errors; classic builder was used because Buildx was absent | Monitor free space, document scoped artifact cleanup, install the intended builder, copy Go lock data before dependency download, use locked Rust builds, and review/pin toolchain/base-image updates |
| Add operational evidence | Running containers alone do not prove useful service | Track consumer status, queue age/depth, outbox lag, DB latency, auth failures and socket/queue counts without logging tokens or message bodies; rehearse backups and restores |
| Limit infrastructure exposure | Local Compose publishes Redis and RabbitMQ ports on all interfaces | Remove unnecessary host mappings or bind development-only access to loopback; verify required access explicitly |

Do not force-add the existing ignored deployment files: create reviewed templates
and deliberately include the actual migration history. Do not substitute generated
`schema.rs` for migrations. Preserve separate auth/server databases and keep
synchronous Diesel calls off async workers.

The failed build recovered after deleting three old, unused images; Docker refused
to delete an image still used by a container. Only about 1.5 GB remained immediately
afterward. Capacity planning is still needed; broad volume pruning is not a fix.

## Priority 4: make the client useful

The [client](lin_client/src/App.tsx) is still the Tauri greeting template. Once
membership and message contracts are defined, build a small vertical slice:
login, authorized chat list, chat creation/invitation, conversation view, explicit
sending/sent/failed states, and reconnect/catch-up. Decide how Tauri handles the
session flow and test it on supported platforms; curl cookie success is not proof
that an embedded browser behaves identically. Existing redirects to `/<username>`
do not currently lead to a functioning application page.

Keep attachments, notifications and multi-instance room distribution behind this
basic slice. End-to-end encryption needs an explicit threat model, device/key
management and recovery design; HTTPS and password hashing do not provide it.
Avoid choosing a durable message/attachment design that assumes plaintext must
always be available to the server if E2EE remains a product requirement.

## Suggested sequence

1. Make a fresh clone runnable and preserve the smoke tests in CI.
2. Implement membership authorization and the HTTPS/session boundary.
3. Add durable messages, acknowledgements, history and retry semantics.
4. Add registration recovery, dependency readiness and bounded resource use.
5. Build the client slice against those contracts, then test outages and load.

Detailed earlier source findings remain in the
[auth review](lin_auth/PRODUCTION_REVIEW.md) and
[server review](lin_server/PRODUCTION_REVIEW.md). Their original verification
sections describe that earlier review; the live evidence above supplements them.
