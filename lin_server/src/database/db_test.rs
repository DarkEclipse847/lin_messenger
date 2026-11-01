use super::*;
use diesel::connection::SimpleConnection;

#[test]
#[ignore = "requires LIN_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
fn consolidated_migrations_initialize_a_fresh_database_once() {
    let mut conn =
        PgConnection::establish(&std::env::var("LIN_TEST_DATABASE_URL").unwrap()).unwrap();
    let result = conn.transaction::<(), Error, _>(|conn| {
        conn.batch_execute(&format!(
            "CREATE SCHEMA fresh_migrations_{}; SET LOCAL search_path TO fresh_migrations_{}",
            std::process::id(),
            std::process::id()
        ))?;
        let applied = conn.run_pending_migrations(MIGRATIONS).unwrap();
        assert_eq!(applied.len(), 7);
        assert!(conn.run_pending_migrations(MIGRATIONS).unwrap().is_empty());
        let id = Uuid::from_u128(42);
        new_user(conn, id, "fresh".into(), "fresh@example.test".into())?;
        create_chat(conn, 1, "fresh chat".into(), id)?;
        use crate::database::schema::chat_members;
        assert_eq!(
            chat_members::table
                .find((1_i64, id))
                .count()
                .get_result::<i64>(conn)?,
            1
        );
        Err(Error::RollbackTransaction)
    });
    assert!(matches!(result, Err(Error::RollbackTransaction)));
}

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
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
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

#[test]
#[ignore = "requires LIN_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
fn message_sequence_migration_backfills_and_reverts_without_losing_messages() {
    use crate::database::schema::messages;
    let mut conn =
        PgConnection::establish(&std::env::var("LIN_TEST_DATABASE_URL").unwrap()).unwrap();
    let result = conn.transaction::<(), Error, _>(|conn| {
        conn.batch_execute(&format!("CREATE SCHEMA sequence_test_{}; SET LOCAL search_path TO sequence_test_{}", std::process::id(), std::process::id()))?;
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        conn.revert_last_migration(MIGRATIONS).unwrap();
        conn.batch_execute("INSERT INTO messages(id,body,sent_at) VALUES (900,'null time',NULL), (500,'later','2026-01-02'), (100,'same time first','2026-01-01'), (200,'same time second','2026-01-01')")?;
        for _ in 0..2 {
            conn.run_pending_migrations(MIGRATIONS).unwrap();
            let rows = messages::table.order(messages::sequence).select((messages::id, messages::sequence)).load::<(i64,i64)>(conn)?;
            assert_eq!(rows, [(900,1), (100,2), (200,3), (500,4)]);
            // Default starts strictly beyond the backfill, including after rollback/reapply.
            conn.batch_execute("INSERT INTO messages(id,body) VALUES (999,'new')")?;
            assert_eq!(messages::table.find(999_i64).select(messages::sequence).first::<i64>(conn)?, 5);
            diesel::delete(messages::table.find(999_i64)).execute(conn)?;
            conn.revert_last_migration(MIGRATIONS).unwrap();
            assert_eq!(messages::table.count().get_result::<i64>(conn)?, 4);
        }
        Err(Error::RollbackTransaction)
    });
    assert!(matches!(result, Err(Error::RollbackTransaction)));
}
