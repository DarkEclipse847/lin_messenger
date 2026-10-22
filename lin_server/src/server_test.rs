use super::*;

#[tokio::test]
async fn slow_clients_are_removed_without_blocking_healthy_clients() {
    let (mut server, _) = ChatServer::new();
    let (slow, _slow_rx) = mpsc::channel(1);
    let (healthy, mut healthy_rx) = mpsc::channel(4);
    slow.try_send("full".into()).unwrap();
    server.sessions.insert(10, slow);
    server.sessions.insert(11, healthy);
    server.rooms.insert(42, HashSet::from([10, 11]));
    server.send_system_message(&42, 0, "hello").await;
    assert_eq!(healthy_rx.recv().await.as_deref(), Some("hello"));
    assert!(!server.sessions.contains_key(&10));
    assert_eq!(server.rooms[&42], HashSet::from([11]));
}

#[tokio::test]
async fn leaving_last_room_removes_it_and_unknown_connections_cannot_join() {
    let (mut server, _) = ChatServer::new();
    let (tx, _rx) = mpsc::channel(16);
    let id = server.connect(tx).await;
    server.join_room(id, 42).await;
    assert!(!server.rooms.contains_key(&0));
    server.disconnect(id).await;
    assert!(server.rooms.is_empty());
    server.join_room(id, 99).await;
    assert!(server.rooms.is_empty());
}

#[tokio::test]
async fn messages_only_reach_other_connections_in_the_same_room() {
    let (mut server, _) = ChatServer::new();
    let (a, mut ar) = mpsc::channel(4);
    let (b, mut br) = mpsc::channel(4);
    let (c, mut cr) = mpsc::channel(4);
    server.sessions.extend([(1, a), (2, b), (3, c)]);
    server.rooms.insert(10, HashSet::from([1, 2]));
    server.rooms.insert(20, HashSet::from([3]));
    server.send_message(1, "hello").await;
    assert_eq!(br.recv().await.as_deref(), Some("hello"));
    assert!(ar.try_recv().is_err());
    assert!(cr.try_recv().is_err());
}
