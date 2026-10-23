//! Persisted permissions; all calls run on a blocking worker, never an HTTP worker.
use diesel::prelude::*;
use uuid::Uuid;

use super::{
    db,
    models::ChatMember,
    schema::{chat_members, chats},
};
use crate::RoomId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessError {
    Denied,
    Unavailable,
    NoRoom,
    SaveFailed,
}

impl std::fmt::Display for AccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Denied => "Access to chat denied",
            Self::Unavailable => "Chat permissions unavailable",
            Self::SaveFailed => "Message could not be saved",
            Self::NoRoom => "Please join a chat first",
        })
    }
}

pub trait MembershipStore: Send + Sync + std::fmt::Debug {
    fn save_message(&self, room: RoomId, sender: Uuid, body: String) -> Result<(), AccessError>;
    fn rooms(&self, user: Uuid) -> Result<Vec<RoomId>, AccessError>;
    fn members(&self, room: RoomId) -> Result<Vec<Uuid>, AccessError>;
    fn change(&self, room: RoomId, actor: Uuid, target: Uuid, add: bool)
    -> Result<(), AccessError>;
}

#[derive(Debug)]
pub struct PostgresMembership;

impl MembershipStore for PostgresMembership {
    fn save_message(&self, room: RoomId, sender: Uuid, body: String) -> Result<(), AccessError> {
        save_message(
            &mut db::open_connection().map_err(|_| AccessError::SaveFailed)?,
            room,
            sender,
            body,
        )
    }

    fn rooms(&self, user: Uuid) -> Result<Vec<RoomId>, AccessError> {
        chat_members::table
            .filter(chat_members::user_id.eq(user))
            .select(chat_members::chat_id)
            .order(chat_members::chat_id)
            .load::<i64>(&mut db::open_connection().map_err(|_| AccessError::Unavailable)?)
            .map(|ids| {
                ids.into_iter()
                    .filter_map(|id| u64::try_from(id).ok())
                    .collect()
            })
            .map_err(|_| AccessError::Unavailable)
    }

    fn members(&self, room: RoomId) -> Result<Vec<Uuid>, AccessError> {
        let room = i64::try_from(room).map_err(|_| AccessError::Denied)?;
        chat_members::table
            .filter(chat_members::chat_id.eq(room))
            .select(chat_members::user_id)
            .load(&mut db::open_connection().map_err(|_| AccessError::Unavailable)?)
            .map_err(|_| AccessError::Unavailable)
    }

    fn change(
        &self,
        room: RoomId,
        actor: Uuid,
        target: Uuid,
        add: bool,
    ) -> Result<(), AccessError> {
        change_member(
            &mut db::open_connection().map_err(|_| AccessError::Unavailable)?,
            room,
            actor,
            target,
            add,
        )
    }
}

// Serialize authorization and insertion with membership changes on the same chat.
pub fn save_message(
    conn: &mut PgConnection,
    room: RoomId,
    sender: Uuid,
    body: String,
) -> Result<(), AccessError> {
    let room = i64::try_from(room).map_err(|_| AccessError::Denied)?;
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        let chat = chats::table
            .find(room)
            .select(chats::id)
            .for_update()
            .first::<i64>(conn)
            .optional()?;
        if chat.is_none()
            || !diesel::select(diesel::dsl::exists(
                chat_members::table.find((room, sender)),
            ))
            .get_result::<bool>(conn)?
        {
            return Ok(Err(AccessError::Denied));
        }
        db::create_message(conn, db::generate_id(), room, sender, body, None)?;
        Ok(Ok(()))
    })
    .map_err(|_| AccessError::SaveFailed)?
}

pub fn change_member(
    conn: &mut PgConnection,
    room: RoomId,
    actor: Uuid,
    target: Uuid,
    add: bool,
) -> Result<(), AccessError> {
    let room = i64::try_from(room).map_err(|_| AccessError::Denied)?;
    // Lock the chat so authorization and mutation share one transaction.
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        let admin = chats::table
            .find(room)
            .select(chats::admin)
            .for_update()
            .first::<Option<Uuid>>(conn)
            .optional()?;
        if admin != Some(Some(actor)) || (!add && actor == target) {
            return Ok(Err(AccessError::Denied));
        }
        if add {
            // Unknown replicated users cannot be added. Do not expose FK details.
            use super::schema::users;
            if !diesel::select(diesel::dsl::exists(users::table.find(target)))
                .get_result::<bool>(conn)?
            {
                return Ok(Err(AccessError::Denied));
            }
            diesel::insert_into(chat_members::table)
                .values(ChatMember {
                    chat_id: room,
                    user_id: target,
                })
                .on_conflict_do_nothing()
                .execute(conn)?;
        } else {
            diesel::delete(
                chat_members::table
                    .filter(chat_members::chat_id.eq(room))
                    .filter(chat_members::user_id.eq(target)),
            )
            .execute(conn)?;
        }
        Ok(Ok(()))
    })
    .map_err(|_| AccessError::Unavailable)?
}

#[cfg(test)]
mod tests {
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
}
