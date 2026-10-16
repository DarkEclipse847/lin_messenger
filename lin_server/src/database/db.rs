use chrono::Utc;
use diesel::{prelude::*, result::Error};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use dotenvy::dotenv;
use rand::RngExt;
use std::env;
use uuid::Uuid;

use crate::database::models::{Chat, Message, NewChat, NewMessage, NewUser, User};

pub const UUID_MIGRATIONS: EmbeddedMigrations = embed_migrations!("uuid_migrations");

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

/// Generates a random, positive `i64` for use as a `chats`/`messages` primary key.
///
/// These tables use manually-assigned `BIGINT` ids rather than a `SERIAL`/`BIGSERIAL`
/// column (see the global TODO about eventually moving to UUIDs), so callers are
/// responsible for picking an id before inserting.
pub fn generate_id() -> i64 {
    let bits: u64 = rand::rng().random();
    ((bits >> 1) as i64).max(1)
}

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let mut conn = PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to server database"));

    conn.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run database migrations");

    conn.run_pending_migrations(UUID_MIGRATIONS)
        .expect("Failed to migrate user identities to UUID");

    conn
}

// NOTE: .returning() and .get_results actually needed in just one case
// (or if i'll switch to UUIDs), on messages to get auto-generated ID
// [check this in diesel docs](https://diesel.rs/guides/all-about-inserts/#the-returning-clause)
// Returning clause can be useful to avoid extra SELECT stmt
//
//
//
//  DISADVANTAGES(ai summary):
//
//  * Memory Overhead:   For large batch operations, PostgreSQL must maintain both old and new row
//                       versions in memory to return them, which can increase memory pressure.
//  * Data Volume:       Using `RETURNING *` on tables with many or large columns
//                       (like long JSON or TEXT fields) can increase network
//                       transit time and deserialization overhead in the application layer.
//  * Trigger Execution: Since RETURNING provides the row after triggers have run,
//                       the overall DML statement may appear slower if complex triggers are firing.

pub fn new_user(
    conn: &mut PgConnection,
    new_id: Uuid,
    new_username: String,
    new_email: String,
) -> Result<User, Error> {
    use crate::database::schema::users;

    let new_user = NewUser {
        id: &new_id,
        username: &new_username,
        email: &new_email,
        registred_at: &Utc::now().naive_utc(),
    };

    return diesel::insert_into(users::table)
        .values(&new_user)
        .returning(User::as_returning())
        .get_result(conn);
}

pub fn create_chat(
    conn: &mut PgConnection,
    new_id: i64,
    new_title: String,
    new_admin: Uuid,
) -> Result<Chat, Error> {
    use crate::database::schema::chats;

    let new_chat = NewChat {
        id: &new_id,
        title: &new_title,
        admin: &new_admin,
        created_at: &Utc::now().naive_utc(),
    };

    return diesel::insert_into(chats::table)
        .values(&new_chat)
        .returning(Chat::as_returning())
        .get_result(conn);
}

pub fn create_message(
    conn: &mut PgConnection,
    new_id: i64,
    new_chat_id: i64,
    new_sender_id: Uuid,
    new_body: String,
    new_attachment: Option<&str>,
) -> Result<Message, Error> {
    use crate::database::schema::messages;

    let new_message = NewMessage {
        id: &new_id,
        chat_id: &new_chat_id,
        sender_id: &new_sender_id,
        body: &new_body,
        attachment: new_attachment,
        sent_at: &Utc::now().naive_utc(),
    };

    return diesel::insert_into(messages::table)
        .values(&new_message)
        .returning(Message::as_returning())
        .get_result(conn);
}

// капучино большой 262 -> 300 наличными
// американо средний 195 -> 1000 наличными

/// Registration redelivery is safe only when the existing identity matches.
pub fn replicate_user(
    conn: &mut PgConnection,
    id: Uuid,
    username: String,
    email: String,
) -> Result<(), Error> {
    use crate::database::schema::users;
    conn.transaction(|conn| {
        let registered = Utc::now().naive_utc();
        let user = NewUser {
            id: &id,
            username: &username,
            email: &email,
            registred_at: &registered,
        };
        diesel::insert_into(users::table)
            .values(&user)
            .on_conflict(users::id)
            .do_nothing()
            .execute(conn)?;
        let existing = users::table
            .find(id)
            .select(User::as_select())
            .first::<User>(conn)?;
        if existing.username != username || existing.email != email {
            return Err(Error::RollbackTransaction);
        }
        Ok(())
    })
}

#[cfg(test)]
mod uuid_migration_tests {
    use super::*;
    use diesel::connection::SimpleConnection;

    #[test]
    #[ignore = "requires LIN_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
    fn migrate_existing_users_and_references() {
        let mut conn =
            PgConnection::establish(&std::env::var("LIN_TEST_DATABASE_URL").unwrap()).unwrap();
        let result = conn.transaction::<(), Error, _>(|conn| {
            conn.batch_execute(&format!("CREATE SCHEMA uuid_test_{}; SET LOCAL search_path TO uuid_test_{}", std::process::id(), std::process::id()))?;
            conn.batch_execute(include_str!("../../migrations/2026-04-27-103940-0000_create_users/up.sql"))?;
            conn.batch_execute(include_str!("../../migrations/2026-04-27-122747-0000_create_chats/up.sql"))?;
            conn.batch_execute(include_str!("../../migrations/2026-04-27-142258-0000_create_messages/up.sql"))?;
            conn.batch_execute("INSERT INTO users(id,username,email,registred_at) VALUES (42,'alice','alice@example.test',now()),(9223372036854775807,'bob','bob@example.test',now());
                INSERT INTO chats(id,title,admin,created_at) VALUES (1,'legacy',42,now()),(2,'nullable',NULL,now());
                INSERT INTO messages(id,chat_id,sender_id,body,sent_at) VALUES (1,1,9223372036854775807,'preserved',now())")?;
            conn.run_pending_migrations(UUID_MIGRATIONS).unwrap();
            conn.run_pending_migrations(UUID_MIGRATIONS).unwrap();
            use crate::database::schema::{chats, messages, users};
            let alice = Uuid::parse_str("016939df-e261-3cd7-8cd6-4ae01aa40c54").unwrap();
            let bob = Uuid::parse_str("96af6c64-b564-3e3d-831b-d2cbd5a82f28").unwrap();
            assert_eq!(users::table.find(alice).select(users::username).first::<String>(conn)?, "alice");
            assert_eq!(chats::table.find(1_i64).select(chats::admin).first::<Option<Uuid>>(conn)?, Some(alice));
            assert_eq!(chats::table.find(2_i64).select(chats::admin).first::<Option<Uuid>>(conn)?, None);
            let message = messages::table.find(1_i64).select(Message::as_select()).first::<Message>(conn)?;
            assert_eq!(message.sender_id, Some(bob));
            assert_eq!(message.body.as_deref(), Some("preserved"));
            let new_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
            replicate_user(conn, new_id, "charlie".into(), "charlie@example.test".into())?;
            replicate_user(conn, new_id, "charlie".into(), "charlie@example.test".into())?;
            create_chat(conn, 3, "new".into(), new_id)?;
            create_message(conn, 2, 3, new_id, "new message".into(), None)?;
            assert!(create_chat(conn, 4, "invalid admin".into(), Uuid::nil()).is_err());
            Err(Error::RollbackTransaction)
        });
        assert!(matches!(result, Err(Error::RollbackTransaction)));
    }
}
