use super::*;
use std::sync::Mutex;

#[derive(Debug, Default)]
struct Store {
    members: Mutex<HashMap<RoomId, Vec<Uuid>>>,
    unavailable: std::sync::atomic::AtomicBool,
    save_failed: std::sync::atomic::AtomicBool,
    messages: Mutex<Vec<(RoomId, Uuid, String)>>,
}
impl MembershipStore for Store {
    fn save_message(&self, room: RoomId, sender: Uuid, body: String) -> Result<(), AccessError> {
        if self.save_failed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(AccessError::SaveFailed);
        }
        self.messages.lock().unwrap().push((room, sender, body));
        Ok(())
    }

    fn rooms(&self, user: Uuid) -> Result<Vec<RoomId>, AccessError> {
        if self.unavailable.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(AccessError::Unavailable);
        }
        Ok(self
            .members
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, users)| users.contains(&user).then_some(*id))
            .collect())
    }
    fn members(&self, room: RoomId) -> Result<Vec<Uuid>, AccessError> {
        if self.unavailable.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(AccessError::Unavailable);
        }
        Ok(self
            .members
            .lock()
            .unwrap()
            .get(&room)
            .cloned()
            .unwrap_or_default())
    }
    fn change(
        &self,
        room: RoomId,
        actor: Uuid,
        target: Uuid,
        add: bool,
    ) -> Result<(), AccessError> {
        let mut rooms = self.members.lock().unwrap();
        let members = rooms.get_mut(&room).ok_or(AccessError::Denied)?;
        if members.first() != Some(&actor) || (!add && actor == target) {
            return Err(AccessError::Denied);
        }
        if add {
            members.push(target);
        } else {
            members.retain(|id| *id != target);
        }
        Ok(())
    }
}
fn connect(server: &mut ChatServer, user: u128) -> (ConnId, mpsc::Receiver<Msg>) {
    let (tx, rx) = mpsc::channel(4);
    (server.connect(Uuid::from_u128(user), tx), rx)
}
fn fixture() -> (ChatServer, Arc<Store>) {
    let store = Arc::new(Store::default());
    store.members.lock().unwrap().extend([
        (10, vec![Uuid::from_u128(1), Uuid::from_u128(2)]),
        (20, vec![Uuid::from_u128(3)]),
    ]);
    let (server, _) = ChatServer::with_store(store.clone());
    (server, store)
}

#[tokio::test]
async fn messages_are_saved_once_before_delivery_and_failures_do_not_broadcast() {
    let (mut server, store) = fixture();
    let (a, mut ar) = connect(&mut server, 1);
    let (b, mut br) = connect(&mut server, 2);
    assert_eq!(
        server.send_message(a, "no room".into()).await,
        Err(AccessError::NoRoom)
    );
    assert!(store.messages.lock().unwrap().is_empty());
    server.join_room(a, 10).await.unwrap();
    server.join_room(b, 10).await.unwrap();
    server.send_message(a, "Alice: hello".into()).await.unwrap();
    assert_eq!(
        *store.messages.lock().unwrap(),
        vec![(10, Uuid::from_u128(1), "Alice: hello".into())]
    );
    assert_eq!(br.try_recv().unwrap(), "Alice: hello");
    assert!(ar.try_recv().is_err());
    store
        .save_failed
        .store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        server.send_message(a, "lost".into()).await,
        Err(AccessError::SaveFailed)
    );
    assert!(br.try_recv().is_err());
    assert_eq!(store.messages.lock().unwrap().len(), 1);
    store
        .save_failed
        .store(false, std::sync::atomic::Ordering::Relaxed);
    server.disconnect(b);
    server.send_message(a, "alone".into()).await.unwrap();
    assert_eq!(store.messages.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn no_lobby_and_denied_joins_preserve_current_room() {
    let (mut server, _) = fixture();
    let (a, mut rx) = connect(&mut server, 1);
    assert!(server.rooms.is_empty());
    assert!(rx.try_recv().is_err());
    assert_eq!(
        server.send_message(a, "hello".into()).await,
        Err(AccessError::NoRoom)
    );
    assert_eq!(server.list_rooms(a).await.unwrap(), vec![10]);
    server.join_room(a, 10).await.unwrap();
    for room in [0, 20, 999, u64::MAX] {
        assert_eq!(server.join_room(a, room).await, Err(AccessError::Denied));
        assert_eq!(server.rooms[&10], HashSet::from([a]));
    }
    server.disconnect(a);
    assert!(server.rooms.is_empty());
    assert_eq!(server.join_room(a, 10).await, Err(AccessError::Denied));
}

#[tokio::test]
async fn broadcasts_are_isolated_and_removal_evicts_all_user_connections() {
    let (mut server, _) = fixture();
    let (a, mut ar) = connect(&mut server, 1);
    let (b, mut br) = connect(&mut server, 2);
    let (b2, mut b2r) = connect(&mut server, 2);
    let (c, mut cr) = connect(&mut server, 3);
    for id in [a, b, b2] {
        server.join_room(id, 10).await.unwrap();
    }
    server.join_room(c, 20).await.unwrap();
    server.send_message(a, "hello".into()).await.unwrap();
    assert_eq!(br.try_recv().unwrap(), "hello");
    assert_eq!(b2r.try_recv().unwrap(), "hello");
    assert!(ar.try_recv().is_err());
    assert!(cr.try_recv().is_err());
    assert_eq!(
        server.change_member(b, 10, Uuid::from_u128(3), true).await,
        Err(AccessError::Denied)
    );
    assert_eq!(
        server.change_member(a, 10, Uuid::from_u128(1), false).await,
        Err(AccessError::Denied)
    );
    server
        .change_member(a, 10, Uuid::from_u128(2), false)
        .await
        .unwrap();
    assert_eq!(server.rooms[&10], HashSet::from([a]));
    assert_eq!(server.join_room(b, 10).await, Err(AccessError::Denied));
    assert_eq!(
        server.send_message(b2, "blocked".into()).await,
        Err(AccessError::NoRoom)
    );
    server.send_message(a, "private".into()).await.unwrap();
    assert!(br.try_recv().is_err());
    assert!(b2r.try_recv().is_err());
    server
        .change_member(a, 10, Uuid::from_u128(3), true)
        .await
        .unwrap();
    server.join_room(c, 10).await.unwrap();
    server.send_message(c, "invited".into()).await.unwrap();
    assert_eq!(ar.try_recv().unwrap(), "invited");
}

#[tokio::test]
async fn unavailable_permissions_fail_closed_and_stale_recipients_are_pruned() {
    let (mut server, store) = fixture();
    let (a, _ar) = connect(&mut server, 1);
    let (b, mut br) = connect(&mut server, 2);
    server.join_room(a, 10).await.unwrap();
    server.join_room(b, 10).await.unwrap();
    store
        .unavailable
        .store(true, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        server.send_message(a, "secret".into()).await,
        Err(AccessError::Unavailable)
    );
    assert_eq!(server.join_room(a, 10).await, Err(AccessError::Unavailable));
    assert_eq!(server.list_rooms(a).await, Err(AccessError::Unavailable));
    assert!(br.try_recv().is_err());
    store
        .unavailable
        .store(false, std::sync::atomic::Ordering::Relaxed);
    store
        .members
        .lock()
        .unwrap()
        .get_mut(&10)
        .unwrap()
        .retain(|id| *id != Uuid::from_u128(2));
    assert_eq!(
        server.send_message(b, "stale sender".into()).await,
        Err(AccessError::Denied)
    );
    server.send_message(a, "private".into()).await.unwrap();
    assert!(br.try_recv().is_err());
    assert_eq!(server.rooms[&10], HashSet::from([a]));
}

#[tokio::test]
async fn slow_clients_do_not_block_other_members() {
    let (mut server, _) = fixture();
    let (a, _ar) = connect(&mut server, 1);
    let (b, _br) = connect(&mut server, 2);
    let (c, mut cr) = connect(&mut server, 2);
    for id in [a, b, c] {
        server.join_room(id, 10).await.unwrap();
    }
    for _ in 0..4 {
        server.sessions[&b].tx.try_send("full".into()).unwrap();
    }
    server.send_message(a, "hello".into()).await.unwrap();
    assert_eq!(cr.try_recv().unwrap(), "hello");
    assert!(!server.sessions.contains_key(&b));
    assert!(!server.rooms[&10].contains(&b));
}
// Real WebSocket frames exercise routing, identity binding, command parsing,
// and actor authorization together. Persistence is covered by the DB tests.
#[actix_web::test]
#[ignore = "requires local TCP sockets"]
async fn websocket_membership_flow() {
    use actix_web::{App, HttpServer, web};
    let store = Arc::new(Store::default());
    store
        .members
        .lock()
        .unwrap()
        .insert(10, vec![Uuid::from_u128(1)]);
    let (server, handle) = ChatServer::with_store(store);
    let actor = tokio::spawn(server.run());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let http = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(handle.clone()))
            .route("/chat", web::get().to(crate::chat_ws))
    })
    .listen(listener)
    .unwrap()
    .run();
    let stop = http.handle();
    let http_task = tokio::spawn(http);
    let result = web::block(move || {
        let mut alice = WsClient::connect(address, 1);
        let mut bob = WsClient::connect(address, 2);
        let mut bob_other_device = WsClient::connect(address, 2);
        alice.command("/join 10", "joined 10");
        bob.command("/join 10", "!!! chat access denied");
        // /list has no end marker: a subsequent error delimits its output.
        bob.send("/list");
        bob.command("/join 999", "!!! chat access denied");
        bob.command("hello", "!!! join a chat first");
        bob.command(
            "/add 10 00000000-0000-0000-0000-000000000002",
            "!!! chat access denied",
        );
        alice.command(
            "/add 10 00000000-0000-0000-0000-000000000002",
            "member added",
        );
        bob.command("/list", "10");
        bob.command("/join 10", "joined 10");
        bob_other_device.command("/join 10", "joined 10");
        alice.send("hello members");
        assert_eq!(bob.text(), "hello members");
        assert_eq!(bob_other_device.text(), "hello members");
        alice.command(
            "/remove 10 00000000-0000-0000-0000-000000000002",
            "member removed",
        );
        alice.send("private");
        bob.command("/join 10", "!!! chat access denied");
        bob_other_device.command("blocked", "!!! join a chat first");
        bob.send("/list");
        bob.command("/join 10", "!!! chat access denied");
    })
    .await;
    stop.stop(true).await;
    http_task.await.unwrap().unwrap();
    actor.abort();
    result.unwrap();
}

#[actix_web::test]
#[ignore = "requires DATABASE_URL and LIN_TEST_DATABASE_URL pointing to the same disposable PostgreSQL"]
async fn postgres_websocket_membership_survives_restart() {
    use crate::database::{
        db,
        schema::{chats, messages, users},
    };
    use actix_web::{App, HttpServer, web};
    use diesel::prelude::*;

    // Never silently fall back to a developer's .env database.
    let test_url =
        std::env::var("LIN_TEST_DATABASE_URL").expect("disposable test database required");
    assert_eq!(
        std::env::var("DATABASE_URL").ok().as_deref(),
        Some(test_url.as_str())
    );
    let alice_id = rand::rng().random::<u128>().max(1);
    let bob_id = rand::rng().random::<u128>().max(1);
    web::block(move || {
        let mut conn = db::establish_connection();
        for (id, name) in [(alice_id, "alice"), (bob_id, "bob")] {
            db::new_user(
                &mut conn,
                Uuid::from_u128(id),
                format!("{name}-{id}"),
                format!("{id}@example.test"),
            )
            .unwrap();
        }
    })
    .await
    .unwrap();

    let mut persisted_room: Option<RoomId> = None;
    // Start a fresh actor and HTTP server for each phase, preserving only PostgreSQL.
    for restart in [false, true] {
        let (server, handle) = ChatServer::new();
        let actor = tokio::spawn(server.run());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let http = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(handle.clone()))
                .route("/chat", web::get().to(crate::chat_ws))
        })
        .listen(listener)
        .unwrap()
        .run();
        let stop = http.handle();
        let http_task = tokio::spawn(http);
        let result = web::block(move || {
            let mut alice = WsClient::connect(address, alice_id);
            let mut bob = WsClient::connect(address, bob_id);
            let mut bob_device = WsClient::connect(address, bob_id);
            if !restart {
                bob.command("hello", "!!! join a chat first");
                alice.send("/create docker-membership");
                let created = alice.text();
                assert!(created.starts_with("created chat "), "{created}");
                let room = created
                    .split_whitespace()
                    .nth(2)
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                alice.command("/list", &room.to_string());
                bob.send("/list");
                bob.command(&format!("/join {room}"), "!!! chat access denied");
                bob.command(
                    &format!("/add {room} {}", Uuid::from_u128(bob_id)),
                    "!!! chat access denied",
                );
                alice.command(
                    &format!("/add {room} {}", Uuid::from_u128(bob_id)),
                    "member added",
                );
                alice.command(
                    &format!("/add {room} {}", Uuid::from_u128(bob_id)),
                    "member added",
                );
                bob.command("/list", &room.to_string());
                bob.command(&format!("/join {room}"), &format!("joined {room}"));
                // Denial must preserve Alice's existing room.
                alice.command("/join 0", "!!! invalid room ID");
                bob.send("persisted membership");
                assert_eq!(alice.text(), "persisted membership");
                room
            } else {
                let room = persisted_room.unwrap();
                for client in [&mut alice, &mut bob, &mut bob_device] {
                    client.command("/list", &room.to_string());
                    client.command(&format!("/join {room}"), &format!("joined {room}"));
                }
                alice.send("after restart");
                assert_eq!(bob.text(), "after restart");
                assert_eq!(bob_device.text(), "after restart");
                bob.command(
                    &format!("/remove {room} {}", Uuid::from_u128(alice_id)),
                    "!!! chat access denied",
                );
                alice.command(
                    &format!("/remove {room} {}", Uuid::from_u128(alice_id)),
                    "!!! chat access denied",
                );
                alice.command(
                    &format!("/remove {room} {}", Uuid::from_u128(bob_id)),
                    "member removed",
                );
                // Delimit the no-echo broadcast before checking revoked receivers.
                alice.send("private after removal");
                alice.command("/list", &room.to_string());
                bob.command(&format!("/join {room}"), "!!! chat access denied");
                bob_device.command("blocked", "!!! join a chat first");
                let mut reconnected = WsClient::connect(address, bob_id);
                reconnected.send("/list");
                reconnected.command(&format!("/join {room}"), "!!! chat access denied");
                room
            }
        })
        .await;
        stop.stop(true).await;
        http_task.await.unwrap().unwrap();
        actor.abort();
        let _ = actor.await;
        persisted_room = Some(result.unwrap());
        let room = persisted_room.unwrap() as i64;
        web::block(move || {
            let mut conn = db::open_connection().unwrap();
            let saved = messages::table
                .filter(messages::chat_id.eq(room))
                .order(messages::sent_at)
                .select((messages::sender_id, messages::body))
                .load::<(Option<Uuid>, Option<String>)>(&mut conn)
                .unwrap();
            let mut expected = vec![(
                Some(Uuid::from_u128(bob_id)),
                Some("persisted membership".into()),
            )];
            if restart {
                expected.extend([
                    (
                        Some(Uuid::from_u128(alice_id)),
                        Some("after restart".into()),
                    ),
                    (
                        Some(Uuid::from_u128(alice_id)),
                        Some("private after removal".into()),
                    ),
                ]);
            }
            assert_eq!(saved, expected);
        })
        .await
        .unwrap();
    }
    // Failed assertions leave evidence only in the explicitly disposable database.
    web::block(move || {
        let mut conn = db::open_connection().unwrap();
        diesel::delete(
            messages::table.filter(messages::chat_id.eq(persisted_room.unwrap() as i64)),
        )
        .execute(&mut conn)
        .unwrap();
        diesel::delete(chats::table.find(persisted_room.unwrap() as i64))
            .execute(&mut conn)
            .unwrap();
        diesel::delete(
            users::table
                .filter(users::id.eq_any([Uuid::from_u128(alice_id), Uuid::from_u128(bob_id)])),
        )
        .execute(&mut conn)
        .unwrap();
    })
    .await
    .unwrap();
}

struct WsClient(std::net::TcpStream);
impl WsClient {
    fn connect(address: std::net::SocketAddr, user: u128) -> Self {
        use std::io::{Read, Write};
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        write!(stream, "GET /chat HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nX-User-Id: {}\r\n\r\n", Uuid::from_u128(user)).unwrap();
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            response.push(byte[0]);
        }
        assert!(response.starts_with(b"HTTP/1.1 101"));
        Self(stream)
    }
    fn frame(&mut self, opcode: u8, data: &[u8]) {
        use std::io::Write;
        assert!(data.len() < 126);
        // Client frames must be masked, including test frames.
        let mask = [1, 2, 3, 4];
        let mut frame = vec![0x80 | opcode, 0x80 | data.len() as u8];
        frame.extend(mask);
        frame.extend(data.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.0.write_all(&frame).unwrap();
    }
    fn send(&mut self, text: &str) {
        self.frame(1, text.as_bytes());
    }
    fn text(&mut self) -> String {
        use std::io::Read;
        loop {
            let mut header = [0; 2];
            self.0.read_exact(&mut header).unwrap();
            assert!(header[1] < 126);
            let mut data = vec![0; header[1] as usize];
            self.0.read_exact(&mut data).unwrap();
            match header[0] & 15 {
                1 => return String::from_utf8(data).unwrap(),
                9 => self.frame(10, &data),
                other => panic!("unexpected frame {other}"),
            }
        }
    }
    fn command(&mut self, command: &str, expected: &str) {
        self.send(command);
        assert_eq!(self.text(), expected, "command: {command}");
    }
}
