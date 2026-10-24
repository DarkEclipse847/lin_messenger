use super::*;
use diesel::connection::SimpleConnection;

#[test]
#[ignore = "requires LIN_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
fn messages_persist_identity_body_and_time_and_reject_non_members() {
    use super::super::{models::Message, schema::messages};
    use diesel_migrations::MigrationHarness;
    let mut conn =
        PgConnection::establish(&std::env::var("LIN_TEST_DATABASE_URL").unwrap()).unwrap();
    let result = conn.transaction::<(), diesel::result::Error, _>(|conn| {
        conn.batch_execute(&format!("CREATE SCHEMA message_test_{}; SET LOCAL search_path TO message_test_{}", std::process::id(), std::process::id()))?;
        conn.run_pending_migrations(db::MIGRATIONS).unwrap();
        let alice = Uuid::from_u128(1);
        let bob = Uuid::from_u128(2);
        db::new_user(conn, alice, "alice".into(), "alice@example.test".into())?;
        db::new_user(conn, bob, "bob".into(), "bob@example.test".into())?;
        db::create_chat(conn, 10, "chat".into(), alice)?;
        assert_eq!(save_message(conn, 10, bob, "denied".into()), Err(AccessError::Denied));
        assert_eq!(save_message(conn, 999, alice, "missing".into()), Err(AccessError::Denied));
        let before = chrono::Utc::now().naive_utc();
        save_message(conn, 10, alice, "Alice: hello".into()).unwrap();
        save_message(conn, 10, alice, "second".into()).unwrap();
        let saved = messages::table.order(messages::sent_at).select(Message::as_select()).load::<Message>(conn)?;
        assert_eq!(saved.len(), 2);
        assert_ne!(saved[0].id, saved[1].id);
        assert!(saved.iter().all(|m| m.id > 0 && m.chat_id == Some(10) && m.sender_id == Some(alice) && m.sent_at.unwrap() >= before && m.attachment.is_none()));
        assert_eq!(saved[0].body.as_deref(), Some("Alice: hello"));
        conn.batch_execute("CREATE FUNCTION reject_message() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test failure'; END $$; CREATE TRIGGER reject_message BEFORE INSERT ON messages FOR EACH ROW EXECUTE FUNCTION reject_message()")?;
        assert_eq!(save_message(conn, 10, alice, "failed".into()), Err(AccessError::SaveFailed));
        assert_eq!(messages::table.count().get_result::<i64>(conn)?, 2);
        Err(diesel::result::Error::RollbackTransaction)
    });
    assert!(matches!(
        result,
        Err(diesel::result::Error::RollbackTransaction)
    ));
}

#[test]
#[ignore = "requires LIN_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
fn migration_and_persisted_permissions() {
    let mut conn =
        PgConnection::establish(&std::env::var("LIN_TEST_DATABASE_URL").unwrap()).unwrap();
    let result = conn.transaction::<(), diesel::result::Error, _>(|conn| {
        conn.batch_execute(&format!("CREATE SCHEMA membership_test_{}; SET LOCAL search_path TO membership_test_{}", std::process::id(), std::process::id()))?;
        conn.batch_execute(include_str!("../../migrations/2026-04-27-103940-0000_create_users/up.sql"))?;
        conn.batch_execute(include_str!("../../migrations/2026-04-27-122747-0000_create_chats/up.sql"))?;
        conn.batch_execute(include_str!("../../migrations/2026-04-27-142258-0000_create_messages/up.sql"))?;
        conn.batch_execute(include_str!("../../migrations/2026-10-17-000000_user_uuid/up.sql"))?;
        let alice = Uuid::from_u128(1);
        let bob = Uuid::from_u128(2);
        db::new_user(conn, alice, "alice".into(), "alice@example.test".into())?;
        db::new_user(conn, bob, "bob".into(), "bob@example.test".into())?;
        conn.batch_execute("INSERT INTO chats(id,title,admin,created_at) VALUES (10,'legacy','00000000-0000-0000-0000-000000000001',now()), (20,'orphan',NULL,now())")?;
        conn.batch_execute(include_str!("../../migrations/2026-10-18-000000_chat_members/up.sql"))?;
        assert_eq!(chat_members::table.select((chat_members::chat_id, chat_members::user_id)).load::<(i64,Uuid)>(conn)?, vec![(10,alice)]);
        db::create_chat(conn, 30, "new".into(), alice)?;
        assert_eq!(chat_members::table.find((30_i64,alice)).count().get_result::<i64>(conn)?, 1);
        assert_eq!(change_member(conn, 30, bob, bob, true), Err(AccessError::Denied));
        assert_eq!(change_member(conn, 999, alice, bob, true), Err(AccessError::Denied));
        assert_eq!(change_member(conn, 20, alice, bob, true), Err(AccessError::Denied));
        assert_eq!(change_member(conn, 30, alice, Uuid::nil(), true), Err(AccessError::Denied));
        change_member(conn, 30, alice, bob, true).unwrap();
        change_member(conn, 30, alice, bob, true).unwrap();
        assert_eq!(chat_members::table.filter(chat_members::chat_id.eq(30_i64)).count().get_result::<i64>(conn)?, 2);
        assert_eq!(change_member(conn, 30, alice, alice, false), Err(AccessError::Denied));
        assert_eq!(change_member(conn, 30, bob, alice, false), Err(AccessError::Denied));
        change_member(conn, 30, alice, bob, false).unwrap();
        change_member(conn, 30, alice, bob, false).unwrap();
        assert_eq!(chat_members::table.find((30_i64,bob)).count().get_result::<i64>(conn)?, 0);
        // Failed creator membership rolls back the chat insertion as well.
        conn.batch_execute("CREATE FUNCTION reject_membership() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test failure'; END $$; CREATE TRIGGER reject_membership BEFORE INSERT ON chat_members FOR EACH ROW EXECUTE FUNCTION reject_membership()")?;
        assert!(db::create_chat(conn, 40, "rollback".into(), alice).is_err());
        assert_eq!(chats::table.find(40_i64).count().get_result::<i64>(conn)?, 0);
        conn.batch_execute("DROP TRIGGER reject_membership ON chat_members")?;
        diesel::delete(chats::table.find(30_i64)).execute(conn)?;
        assert_eq!(chat_members::table.filter(chat_members::chat_id.eq(30_i64)).count().get_result::<i64>(conn)?, 0);
        Err(diesel::result::Error::RollbackTransaction)
    });
    assert!(matches!(
        result,
        Err(diesel::result::Error::RollbackTransaction)
    ));
}
