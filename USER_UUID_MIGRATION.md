# User UUID migration

User identities are PostgreSQL `uuid` values in both services. Auth generates
random UUIDv4 IDs. Registration JSON, JWT `user_id`, Redis access-token keys,
and gateway `X-User-Id` carry UUID strings. Nil UUIDs and numeric identities are
rejected. Chat/message primary keys, connection/room IDs, and auth-session record
primary keys remain integers; they are not user identities.

## Existing databases

The migrations convert `users.id` in each database, auth's `auth_creds.user_id`,
and the server's `chats.admin` and `messages.sender_id`. They preserve records,
nullable references, indexes, and existing foreign-key definitions. Each database
migration is transactional; the two databases do not share a transaction.

Both migrations derive the same UUID from the same legacy bigint, without
cross-service database access or an extension. They hash `lin:legacy-user:<decimal id>`
with MD5, then set the version nibble to 3 and variant nibble to 8. This is a
project-specific deterministic mapping, not randomization of existing identities
or an authorization mechanism. For example, legacy `42` becomes
`016939df-e261-3cd7-8cd6-4ae01aa40c54`. New UUIDv4 IDs occupy a distinct version space.
Existing mismatched or missing replicated profiles are not reconciled by this migration.

1. Stop incoming traffic and auth writes. Let the old server drain
   `REGISTRATION_REQ`, including unacknowledged events, then stop both applications.
   Numeric events remaining in the queue will be rejected by the new consumer.
   Disposable mocked queues may instead be purged explicitly.
2. Back up both databases if their contents need preserving. Deploy both updated
   binaries/images together. This is a maintenance-window change, not a rolling upgrade.
3. Start the updated services while keeping public traffic stopped. Auth applies
   `lin_auth/database/migrations/001_user_uuid.sql` before GORM AutoMigrate.
   Server applies the tracked `lin_server/migrations/` through Diesel, including
   `2026-10-17-000000_user_uuid/` after the baseline schema.
   Auth detects an already converted column; Diesel records the applied version.
4. Verify the user/reference columns are UUIDs in both databases, check the
   registration consumer is alive, and exercise registration/login and chat creation.
   Then reopen traffic. Users must log in again: old numeric JWTs are rejected,
   refresh-token hashes are cleared, and old Redis keys expire naturally.

An empty database is supported: auth creates UUID columns through AutoMigrate;
server creates its baseline schema and immediately converts it. All server migrations are tracked in
`lin_server/migrations/`, which Docker also copies. Consolidating the directories
does not change migration version IDs: existing Diesel migration records remain
valid, so already applied migrations are skipped.

UUIDs created after migration have no integer equivalent. The server down migration
fails deliberately; rollback requires stopping both updated services and restoring
both coordinated backups before starting old binaries.

## Validation

Run the normal Go and Rust checks from their service directories. Database tests
require a disposable PostgreSQL URL in `LIN_TEST_DATABASE_URL`; they use isolated
schemas and roll back their changes:

```sh
# lin_auth/
LIN_TEST_DATABASE_URL="$TEST_AUTH_DATABASE_URL" go test ./...

# lin_server/
LIN_TEST_DATABASE_URL="$TEST_SERVER_DATABASE_URL" cargo test --locked migrate_existing_users_and_references -- --ignored
```

The database regressions cover existing maximum-bigint identities, deterministic
cross-service mapping, references, nullable chat admins, repeated migrations,
new UUID users, auth-session replacement, and foreign-key enforcement. The shared
registration fixture and HTTP/JWT tests check the UUID wire contract.
