use std::time::{Duration, Instant};
use uuid::Uuid;

use actix_web::web;
use actix_ws::AggregatedMessage;
use futures_util::StreamExt as _;
use tokio::{sync::mpsc, time::interval};

use crate::{ChatServerHandle, ConnId, database};

// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

// Echo text & binary messages received from the client, respond to ping messages, and monitor
// connection health to detect network issues and free up resources.
pub async fn chat_ws(
    chat_server: ChatServerHandle,
    mut session: actix_ws::Session,
    msg_stream: actix_ws::MessageStream,
    user_id: Uuid,
) {
    log::info!("connected");

    let mut name: Option<String> = None;
    let mut last_heartbeat = Instant::now();
    let mut interval = interval(HEARTBEAT_INTERVAL);

    let (conn_tx, mut conn_rx) = mpsc::channel(64);

    // unwrap: chat server is not dropped before the HTTP server
    let conn_id = chat_server.connect(conn_tx).await;

    let mut msg_stream = msg_stream
        .max_frame_size(128 * 1024)
        .aggregate_continuations()
        .max_continuation_size(2 * 1024 * 1024);

    let close_reason = loop {
        tokio::select! {
            incoming = msg_stream.next() => {
                let Some(Ok(msg)) = incoming else { break None; };

                match msg {
                    AggregatedMessage::Ping(bytes) => {
                        last_heartbeat = Instant::now();
                        if session.pong(&bytes).await.is_err() { break None; }
                    }

                    AggregatedMessage::Pong(_) => {
                        last_heartbeat = Instant::now();
                    }

                    AggregatedMessage::Text(text) => {
                        if process_text_msg(
                            &chat_server,
                            &mut session,
                            &text,
                            conn_id,
                            &mut name,
                            user_id,
                        )
                        .await.is_err() { break None; }
                    }

                    AggregatedMessage::Binary(_bin) => {
                        log::warn!("unexpected binary message");
                    }

                    AggregatedMessage::Close(reason) => break reason,
                }
            }

            chat_msg = conn_rx.recv() => {
                 let Some(chat_msg) = chat_msg else { break None; };
                 if session.text(chat_msg).await.is_err() { break None; }
            }

            _ = interval.tick() => {
                if Instant::now().duration_since(last_heartbeat) > CLIENT_TIMEOUT {
                    break None;
                }
                if session.ping(b"").await.is_err() { break None; }
            }

            else => {
                break None;
            }
        }
    };

    chat_server.disconnect(conn_id);

    // attempt to close connection gracefully
    let _ = session.close(close_reason).await;
}

async fn process_text_msg(
    chat_server: &ChatServerHandle,
    session: &mut actix_ws::Session,
    text: &str,
    conn: ConnId,
    name: &mut Option<String>,
    user_id: Uuid,
) -> Result<(), actix_ws::Closed> {
    // strip leading and trailing whitespace (spaces, newlines, etc.)
    let msg = text.trim();

    // we check for /<cmd> type of messages
    if msg.starts_with('/') {
        let mut cmd_args = msg.splitn(2, ' ');

        // unwrap: we have guaranteed non-zero string length already
        match cmd_args.next().unwrap() {
            "/list" => {
                log::info!("conn {conn}: listing rooms");

                let rooms = chat_server.list_rooms().await;

                for room in rooms {
                    session.text(room.to_string()).await?;
                }
            }

            "/join" => match cmd_args.next() {
                Some(room) => {
                    log::info!("conn {conn}: joining room {room}");

                    match parse_room_id(room) {
                        Some(room) => {
                            chat_server.join_room(conn, room).await;
                            session.text(format!("joined {room}")).await?;
                        }
                        None => session.text("!!! invalid room ID").await?,
                    }
                }

                None => {
                    session.text("!!! room name is required").await?;
                }
            },

            "/create" => match cmd_args.next() {
                Some(title) => {
                    let title = title.trim().to_owned();

                    if title.is_empty() {
                        session.text("!!! chat title is required").await?;
                    } else {
                        log::info!("conn {conn}: creating chat '{title}'");

                        let new_id = database::db::generate_id();
                        let title_for_db = title.clone();

                        let result = web::block(move || {
                            let mut db_conn = database::db::establish_connection();
                            database::db::create_chat(&mut db_conn, new_id, title_for_db, user_id)
                        })
                        .await;

                        match result {
                            Ok(Ok(chat)) => {
                                chat_server.join_room(conn, chat.id as u64).await;
                                session
                                    .text(format!("created chat {} \"{}\"", chat.id, chat.title))
                                    .await?;
                            }
                            Ok(Err(err)) => {
                                log::error!("conn {conn}: failed to create chat: {err}");
                                session.text("failed to create chat").await?;
                            }
                            Err(err) => {
                                log::error!("conn {conn}: chat creation task failed: {err}");
                                session.text("failed to create chat").await?;
                            }
                        }
                    }
                }

                None => {
                    session.text("chat title is required").await?;
                }
            },

            "/name" => match cmd_args.next() {
                Some(new_name) => {
                    log::info!("conn {conn}: setting name to: {new_name}");
                    name.replace(new_name.to_owned());
                }
                None => {
                    session.text("!!! name is required").await?;
                }
            },

            _ => {
                session.text(format!("!!! unknown command: {msg}")).await?;
            }
        }
    } else {
        // prefix message with our name, if assigned
        let msg = match name {
            Some(name) => format!("{name}: {msg}"),
            None => msg.to_owned(),
        };

        chat_server.send_message(conn, msg).await
    }
    Ok(())
}

fn parse_room_id(value: &str) -> Option<u64> {
    value
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|id| *id <= i64::MAX as u64)
}

#[cfg(test)]
#[path = "handler_test.rs"]
mod tests;
