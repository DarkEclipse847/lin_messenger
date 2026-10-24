# Server readiness

This document describes current safeguards and remaining gaps. It is not a
production certification or a record of a completed integration test run.
See the [server README](README.md) for the protocol, configuration, and test commands.

## Current safeguards

- Handshake identities must be non-nil UUIDs. The server trusts `X-User-Id` supplied
  by the authenticated gateway; parsing the header is not authentication.
- Persisted membership controls listing, joining, and message delivery. New
  connections have no shared lobby. Only the chat administrator can add or remove
  members; removal evicts the user's active connections from that chat.
- Messages are saved before broadcast with the authenticated sender UUID.
  Authorization and insertion share a transaction that locks the chat against
  membership changes. Failed inserts prevent broadcast.
- Each outgoing client queue is bounded to 64 entries. Closed or overloaded
  recipients are removed without blocking broadcasts, and empty rooms are removed.
- Registration is acknowledged after persistence. Matching redeliveries are
  idempotent; conflicting identities are rejected. Invalid events are rejected
  without requeue; persistence failures stop the consumer with the delivery
  unacknowledged.
- SQL migrations are tracked and embedded in the binary. User IDs are UUIDs;
  chat/message IDs remain positive i64 values.

## Remaining gaps

- **Gateway and session enforcement:** Direct access allows impersonation through
  `X-User-Id`. Keep the listener private behind the gateway. Session expiry and
  revocation after upgrade, and browser Origin validation, are not implemented.
- **Delivery recovery:** Persisted messages have no history API, offline delivery,
  acknowledgments, retry deduplication, or reconnect/resume protocol. A committed
  message may never reach a live recipient. Connection-local display names are
  freely chosen and are not verified identities. End-to-end encryption is absent.
- **Registration recovery:** The consumer has no reconnect supervisor or readiness
  signal. Auth publication is best effort. Dead-letter handling, monitored retries,
  an auth-owned outbox, and reconciliation remain needed.
- **Resource limits:** The command queue, total session count, incoming command
  rate, and aggregate database work are not bounded. Database pooling/timeouts and
  socket write deadlines remain absent. Entry-count queue limits do not bound
  aggregate bytes; buffered frames may arrive after member removal.
- **Deployment and operations:** Compose and gateway configuration are untracked.
  There is no bundled HTML UI, dependency health endpoint, or CI pipeline. Backups,
  restore drills, observability, and load/failure testing remain necessary.

## Validation scope

Tests cover parsing, UUID identity handling, room isolation, membership changes,
message-save failures, slow recipients, registration payloads, and database
migrations. Database-backed tests require disposable PostgreSQL; WebSocket tests
are opt-in. See the [development checks](README.md#development-checks).

Unit tests do not establish gateway spoofing protection, broker recovery, or
end-to-end delivery. Exercise registration through auth, profile replication,
two-client chat exchange, denied joins, member removal across connections, and
restart behavior through the deployed gateway. Validate persisted message records
as well as live broadcasts. Test failure recovery and shutdown separately.

See the [auth readiness notes](../lin_auth/PRODUCTION_REVIEW.md) for session and
cross-service consistency limitations.
