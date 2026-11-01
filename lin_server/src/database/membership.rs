//! Persisted permissions; all calls run on a blocking worker, never an HTTP worker.
use diesel::prelude::*;
use uuid::Uuid;

use super::{
    db,
    models::{ChatMember, Message},
    schema::{chat_members, chats, messages},
};
use crate::RoomId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessError {
    Denied,
    Unavailable,
    NoRoom,
    SaveFailed,
    InvalidCursor,
}

impl std::fmt::Display for AccessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidCursor => "Invalid history cursor or limit",
            Self::Denied => "Access to chat denied",
            Self::Unavailable => "Chat permissions unavailable",
            Self::SaveFailed => "Message could not be saved",
            Self::NoRoom => "Please join a chat first",
        })
    }
}

pub trait MembershipStore: Send + Sync + std::fmt::Debug {
    fn save_message(
        &self,
        room: RoomId,
        sender: Uuid,
        body: String,
    ) -> Result<Message, AccessError>;
    fn history(
        &self,
        room: RoomId,
        user: Uuid,
        after: i64,
        limit: usize,
    ) -> Result<HistoryPage, AccessError>;
    fn rooms(&self, user: Uuid) -> Result<Vec<RoomId>, AccessError>;
    fn members(&self, room: RoomId) -> Result<Vec<Uuid>, AccessError>;
    fn change(&self, room: RoomId, actor: Uuid, target: Uuid, add: bool)
    -> Result<(), AccessError>;
}

#[derive(Debug)]
pub struct PostgresMembership;

impl MembershipStore for PostgresMembership {
    fn save_message(
        &self,
        room: RoomId,
        sender: Uuid,
        body: String,
    ) -> Result<Message, AccessError> {
        save_message(
            &mut db::open_connection().map_err(|_| AccessError::SaveFailed)?,
            room,
            sender,
            body,
        )
    }

    fn history(
        &self,
        room: RoomId,
        user: Uuid,
        after: i64,
        limit: usize,
    ) -> Result<HistoryPage, AccessError> {
        history(
            &mut db::open_connection().map_err(|_| AccessError::Unavailable)?,
            room,
            user,
            after,
            limit,
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
) -> Result<Message, AccessError> {
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
        let message = db::create_message(conn, db::generate_id(), room, sender, body, None)?;
        Ok(Ok(message))
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

/// Keep each response bounded by both row count and serialized message payload size.
pub const MAX_HISTORY_LIMIT: usize = 100;
const HISTORY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct HistoryPage {
    pub messages: Vec<Message>,
    pub next_cursor: i64,
    pub has_more: bool,
}

pub fn history(
    conn: &mut PgConnection,
    room: RoomId,
    user: Uuid,
    after: i64,
    limit: usize,
) -> Result<HistoryPage, AccessError> {
    if after < 0 || !(1..=MAX_HISTORY_LIMIT).contains(&limit) {
        return Err(AccessError::InvalidCursor);
    }
    let room = i64::try_from(room).map_err(|_| AccessError::Denied)?;
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        // Same lock as writers: the page and membership check share a stable boundary.
        let chat = chats::table
            .find(room)
            .select(chats::id)
            .for_update()
            .first::<i64>(conn)
            .optional()?;
        if chat.is_none()
            || !diesel::select(diesel::dsl::exists(chat_members::table.find((room, user))))
                .get_result::<bool>(conn)?
        {
            return Ok(Err(AccessError::Denied));
        }
        // Cursors must belong to this chat; a future cursor must not silently skip messages.
        if after != 0
            && !diesel::select(diesel::dsl::exists(
                messages::table
                    .filter(messages::chat_id.eq(room))
                    .filter(messages::sequence.eq(after)),
            ))
            .get_result::<bool>(conn)?
        {
            return Ok(Err(AccessError::InvalidCursor));
        }
        let mut rows = messages::table
            .filter(messages::chat_id.eq(room))
            .filter(messages::sequence.gt(after))
            .order(messages::sequence)
            .limit((limit + 1) as i64)
            .select(Message::as_select())
            .load::<Message>(conn)?;
        let mut bytes = 0;
        let mut count = 0;
        for row in rows.iter().take(limit) {
            // Reserve envelope space and count escaped strings, without coupling
            // persistence to the WebSocket response format.
            let size = 512
                + serde_json::to_string(&row.body)
                    .expect("string serialization")
                    .len()
                + serde_json::to_string(&row.attachment)
                    .expect("string serialization")
                    .len();
            if count > 0 && bytes + size > HISTORY_BYTES {
                break;
            }
            bytes += size;
            count += 1;
        }
        let has_more = rows.len() > count;
        rows.truncate(count);
        let next_cursor = rows.last().map_or(after, |m| m.sequence);
        Ok(Ok(HistoryPage {
            messages: rows,
            next_cursor,
            has_more,
        }))
    })
    .map_err(|_| AccessError::Unavailable)?
}
