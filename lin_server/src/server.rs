// The single command processor owns live sessions and serializes permission changes.
use crate::{
    ConnId, Msg, RoomId,
    database::membership::{
        AccessError, HistoryPage, MAX_HISTORY_LIMIT, MembershipStore, PostgresMembership,
    },
    handler::message_json,
};
use rand::RngExt;
use std::{
    collections::{HashMap, HashSet},
    io,
    sync::Arc,
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

type Reply<T> = oneshot::Sender<Result<T, AccessError>>;

#[derive(Debug)]
enum Command {
    Connect {
        user: Uuid,
        tx: mpsc::Sender<Msg>,
        reply: oneshot::Sender<ConnId>,
    },
    Disconnect {
        conn: ConnId,
    },
    List {
        conn: ConnId,
        reply: Reply<Vec<RoomId>>,
    },
    Join {
        conn: ConnId,
        room: RoomId,
        reply: Reply<()>,
    },
    History {
        conn: ConnId,
        room: RoomId,
        after: i64,
        limit: usize,
        resume: bool,
        reply: Reply<HistoryPage>,
    },
    Change {
        conn: ConnId,
        room: RoomId,
        target: Uuid,
        add: bool,
        reply: Reply<()>,
    },
    Message {
        conn: ConnId,
        msg: Msg,
        reply: Reply<()>,
    },
}

#[derive(Debug)]
struct Session {
    user: Uuid,
    structured: bool,
    tx: mpsc::Sender<Msg>,
}

#[derive(Debug)]
pub struct ChatServer {
    sessions: HashMap<ConnId, Session>,
    rooms: HashMap<RoomId, HashSet<ConnId>>,
    store: Arc<dyn MembershipStore>,
    cmd_rx: mpsc::UnboundedReceiver<Command>,
}

impl ChatServer {
    pub fn new() -> (Self, ChatServerHandle) {
        Self::with_store(Arc::new(PostgresMembership))
    }

    fn with_store(store: Arc<dyn MembershipStore>) -> (Self, ChatServerHandle) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        (
            Self {
                sessions: HashMap::new(),
                rooms: HashMap::new(),
                store,
                cmd_rx,
            },
            ChatServerHandle { cmd_tx },
        )
    }

    fn user(&self, conn: ConnId) -> Result<Uuid, AccessError> {
        self.sessions
            .get(&conn)
            .map(|s| s.user)
            .ok_or(AccessError::Denied)
    }

    async fn query<T: Send + 'static>(
        &self,
        f: impl FnOnce(&dyn MembershipStore) -> Result<T, AccessError> + Send + 'static,
    ) -> Result<T, AccessError> {
        let store = self.store.clone();
        actix_web::web::block(move || f(store.as_ref()))
            .await
            .map_err(|_| AccessError::Unavailable)?
    }

    fn connect(&mut self, user: Uuid, tx: mpsc::Sender<Msg>) -> ConnId {
        let id = loop {
            let id = rand::rng().random();
            if !self.sessions.contains_key(&id) {
                break id;
            }
        };
        self.sessions.insert(
            id,
            Session {
                user,
                tx,
                structured: false,
            },
        );
        // No implicit public lobby: a new connection has no chat access yet.
        id
    }

    fn leave(&mut self, conn: ConnId) {
        for participants in self.rooms.values_mut() {
            participants.remove(&conn);
        }
        self.rooms
            .retain(|_, participants| !participants.is_empty());
    }

    fn disconnect(&mut self, conn: ConnId) {
        self.sessions.remove(&conn);
        self.leave(conn);
    }

    async fn list_rooms(&self, conn: ConnId) -> Result<Vec<RoomId>, AccessError> {
        let user = self.user(conn)?;
        self.query(move |store| store.rooms(user)).await
    }

    async fn join_room(&mut self, conn: ConnId, room: RoomId) -> Result<(), AccessError> {
        let user = self.user(conn)?;
        let members = self.query(move |store| store.members(room)).await?;
        if !members.contains(&user) {
            return Err(AccessError::Denied);
        }
        // A failed authorization leaves the previous room unchanged.
        self.leave(conn);
        self.rooms.entry(room).or_default().insert(conn);
        self.sessions.get_mut(&conn).unwrap().structured = false;
        Ok(())
    }

    async fn history(
        &mut self,
        conn: ConnId,
        room: RoomId,
        after: i64,
        limit: usize,
        resume: bool,
    ) -> Result<HistoryPage, AccessError> {
        let user = self.user(conn)?;
        if after < 0 || !(1..=MAX_HISTORY_LIMIT).contains(&limit) {
            return Err(AccessError::InvalidCursor);
        }
        let page = self
            .query(move |store| store.history(room, user, after, limit))
            .await?;
        if resume {
            // No Message command can interleave between the final query and joining.
            // Intermediate pages leave the connection unsubscribed so live frames cannot
            // advance a client's cursor past messages still waiting to be replayed.
            self.leave(conn);
            self.sessions.get_mut(&conn).unwrap().structured = true;
            if !page.has_more {
                self.rooms.entry(room).or_default().insert(conn);
            }
        }
        Ok(page)
    }

    async fn change_member(
        &mut self,
        conn: ConnId,
        room: RoomId,
        target: Uuid,
        add: bool,
    ) -> Result<(), AccessError> {
        let actor = self.user(conn)?;
        self.query(move |store| store.change(room, actor, target, add))
            .await?;
        if !add {
            if let Some(participants) = self.rooms.get_mut(&room) {
                participants.retain(|id| self.sessions.get(id).is_some_and(|s| s.user != target));
            }
            self.rooms
                .retain(|_, participants| !participants.is_empty());
        }
        Ok(())
    }

    async fn send_message(&mut self, conn: ConnId, msg: Msg) -> Result<(), AccessError> {
        let user = self.user(conn)?;
        let room = self
            .rooms
            .iter()
            .find_map(|(room, ids)| ids.contains(&conn).then_some(*room))
            .ok_or(AccessError::NoRoom)?;
        // Recheck sender AND recipients; stale connections cannot keep access after revocation.
        let members = self.query(move |store| store.members(room)).await?;
        let participants = self
            .rooms
            .get_mut(&room)
            .expect("room belongs to command processor");
        participants.retain(|id| {
            self.sessions
                .get(id)
                .is_some_and(|s| members.contains(&s.user))
        });
        let recipients: Vec<_> = participants
            .iter()
            .copied()
            .filter(|id| *id != conn || self.sessions.get(id).is_some_and(|s| s.structured))
            .collect();
        self.rooms
            .retain(|_, participants| !participants.is_empty());
        if !members.contains(&user) {
            return Err(AccessError::Denied);
        }
        let body = msg.clone();
        let saved = self
            .query(move |store| store.save_message(room, user, body))
            .await?;
        let structured = message_json(&saved).to_string();
        for id in recipients {
            if let Some(session) = self.sessions.get(&id) {
                let frame = if session.structured {
                    structured.clone()
                } else {
                    msg.clone()
                };
                if session.tx.try_send(frame).is_err() {
                    self.disconnect(id);
                }
            }
        }
        Ok(())
    }

    pub async fn run(mut self) -> io::Result<()> {
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                Command::Connect { user, tx, reply } => {
                    let id = self.connect(user, tx);
                    if reply.send(id).is_err() {
                        self.disconnect(id);
                    }
                }
                Command::Disconnect { conn } => self.disconnect(conn),
                Command::List { conn, reply } => {
                    let _ = reply.send(self.list_rooms(conn).await);
                }
                Command::Join { conn, room, reply } => {
                    let _ = reply.send(self.join_room(conn, room).await);
                }
                Command::History {
                    conn,
                    room,
                    after,
                    limit,
                    resume,
                    reply,
                } => {
                    let result = self.history(conn, room, after, limit, resume).await;
                    if reply.send(result).is_err() && resume {
                        self.disconnect(conn);
                    }
                }
                Command::Change {
                    conn,
                    room,
                    target,
                    add,
                    reply,
                } => {
                    let _ = reply.send(self.change_member(conn, room, target, add).await);
                }
                Command::Message { conn, msg, reply } => {
                    let _ = reply.send(self.send_message(conn, msg).await);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ChatServerHandle {
    cmd_tx: mpsc::UnboundedSender<Command>,
}

impl ChatServerHandle {
    pub async fn connect(&self, user: Uuid, tx: mpsc::Sender<Msg>) -> ConnId {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(Command::Connect { user, tx, reply })
            .unwrap();
        rx.await.unwrap()
    }

    async fn request<T>(
        &self,
        command: impl FnOnce(Reply<T>) -> Command,
    ) -> Result<T, AccessError> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(command(reply))
            .map_err(|_| AccessError::Unavailable)?;
        rx.await.map_err(|_| AccessError::Unavailable)?
    }

    pub async fn list_rooms(&self, conn: ConnId) -> Result<Vec<RoomId>, AccessError> {
        self.request(|reply| Command::List { conn, reply }).await
    }
    pub async fn join_room(&self, conn: ConnId, room: RoomId) -> Result<(), AccessError> {
        self.request(|reply| Command::Join { conn, room, reply })
            .await
    }
    pub async fn history(
        &self,
        conn: ConnId,
        room: RoomId,
        after: i64,
        limit: usize,
        resume: bool,
    ) -> Result<HistoryPage, AccessError> {
        self.request(|reply| Command::History {
            conn,
            room,
            after,
            limit,
            resume,
            reply,
        })
        .await
    }
    pub async fn change_member(
        &self,
        conn: ConnId,
        room: RoomId,
        target: Uuid,
        add: bool,
    ) -> Result<(), AccessError> {
        self.request(|reply| Command::Change {
            conn,
            room,
            target,
            add,
            reply,
        })
        .await
    }
    pub async fn send_message(&self, conn: ConnId, msg: Msg) -> Result<(), AccessError> {
        self.request(|reply| Command::Message { conn, msg, reply })
            .await
    }
    pub fn disconnect(&self, conn: ConnId) {
        let _ = self.cmd_tx.send(Command::Disconnect { conn });
    }
}

#[cfg(test)]
#[path = "server_test.rs"]
mod tests;
