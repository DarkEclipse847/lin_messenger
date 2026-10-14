// Multi-room WebSocket chat server.

use std::io;

use actix_files::NamedFile;
use actix_web::{App, Error, HttpRequest, HttpResponse, HttpServer, Responder, middleware, web};
use tokio::{join, task::spawn};

mod handler;
mod server;
mod controllers;
mod database;

use crate::controllers::rabbitmq_controller;

pub use self::server::{ChatServer, ChatServerHandle};

// Connection ID.
pub type ConnId = u64;

// Room ID.
pub type RoomId = u64;

// Message sent to a room/client.
pub type Msg = String;

async fn index() -> impl Responder {
    NamedFile::open_async("./static/index.html").await.unwrap()
}

// Reads the authenticated user ID injected by the nginx reverse proxy.
//
// nginx terminates `/chat` requests with an `auth_request` against lin_auth's
// `/api/auth/validate` endpoint, which checks the caller's access token against the
// copy stored in Redis and, on success, echoes the user id back as `X-User-Id`.
// nginx then forwards that header on to us (see `nginx/nginx.conf`). lin_server trusts
// this header rather than re-validating the token itself, so requests that bypass nginx
// (e.g. hitting this service directly) must supply their own `X-User-Id` header.
fn authenticated_user_id(req: &HttpRequest) -> Option<i64> {
    req.headers()
        .get("X-User-Id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
}

// Handshake and start WebSocket handler with heartbeats.
async fn chat_ws(
    req: HttpRequest,
    stream: web::Payload,
    chat_server: web::Data<ChatServerHandle>,
) -> Result<HttpResponse, Error> {
    let user_id = match authenticated_user_id(&req) {
        Some(user_id) => user_id,
        None => {
            return Ok(HttpResponse::Unauthorized().body("missing or invalid X-User-Id header"));
        }
    };

    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    actix_web::rt::spawn(handler::chat_ws(
        (**chat_server).clone(),
        session,
        msg_stream,
        user_id,
    ));

    Ok(res)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    log::info!("starting HTTP server at http://localhost:8080");

    let (chat_server, server_tx) = ChatServer::new();

    let chat_server = spawn(chat_server.run());

    std::thread::spawn(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build RabbitMQ runtime");

        runtime.block_on(async {
            let mut db_conn = database::db::establish_connection();
            if let Err(err) = rabbitmq_controller::rabbit_consumer(
                &mut db_conn,
                "REGISTRATION_REQ".into(),
            )
            .await
            {
                log::error!("RabbitMQ consumer stopped: {err:?}");
            }
        });
    });

    let http_server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(server_tx.clone()))
            // WebSocket UI HTML file
            .service(web::resource("/").to(index))
            // websocket routes (path must match the `/chat` location proxied by nginx)
            .service(web::resource("/chat").route(web::get().to(chat_ws)))
            // standard middleware
            .wrap(middleware::NormalizePath::trim())
            .wrap(middleware::Logger::default())
    })
    .workers(2)
    .bind(("0.0.0.0", 8080))?
    .run();

    let (http_result, _) = join!(
        http_server,
        async move { chat_server.await.unwrap() }
    );

    http_result
}