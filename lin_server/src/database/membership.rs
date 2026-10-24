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
#[path = "membership_test.rs"]
mod tests;
