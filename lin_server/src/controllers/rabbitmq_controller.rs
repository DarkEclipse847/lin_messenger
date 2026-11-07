use diesel::PgConnection;
use diesel_migrations::MigrationHarness;
use futures_util::StreamExt;
use lapin::{
    Connection, ConnectionProperties, Error,
    options::*,
    types::{FieldTable, ShortString},
};
use std::time::{Duration, Instant};

use crate::{controllers::models::UserRabbit, database::db::replicate_user};

// https://youtu.be/QwoF1-1QgwA?si=H3IZSgPmXDgnpk-c
// lol i've coded this drunk af on a party, and when my playlist suddenly played this song, i saved it here
// enjoy :)
// i just opened link above, hoping that this is some kind of tutorial on how to use lapin and found this

/// Resolves the broker URL from `RABBITMQ_URL`, normalizing the vhost for lapin.
///
/// URLs like `amqp://user:pass@host:5672/` are parsed by lapin as an empty vhost, but
/// RabbitMQ's default vhost is `/` and must be URL-encoded as `%2F`.
fn rabbitmq_url() -> String {
    let url = std::env::var("RABBITMQ_URL")
        .unwrap_or_else(|_| "amqp://guest:guest@localhost:5672/%2F".into());

    if url.ends_with("%2F") || url.ends_with("%2f") {
        return url;
    }

    if url.ends_with('/') {
        return format!("{url}%2F");
    }

    let host_and_vhost = url.rsplit('@').next().unwrap_or(&url);
    if !host_and_vhost.contains('/') {
        return format!("{url}/%2F");
    }

    url
}

pub async fn rabbit_connection() -> Result<Connection, Error> {
    Connection::connect(&rabbitmq_url(), ConnectionProperties::default()).await
}

// Runs on the dedicated consumer thread: synchronous Diesel never blocks HTTP workers.
pub async fn run_registration_consumer() {
    let mut delay = Duration::from_secs(1);
    loop {
        let started = Instant::now();
        match crate::database::db::open_connection() {
            Ok(mut db) => {
                if db
                    .run_pending_migrations(crate::database::db::MIGRATIONS)
                    .is_ok()
                {
                    if rabbit_consumer(&mut db, "REGISTRATION_REQ".into())
                        .await
                        .is_err()
                    {
                        log::warn!("Registration broker connection or delivery failed");
                    }
                } else {
                    log::error!("Registration database migration failed");
                }
            }
            Err(_) => log::warn!("Registration database connection failed"),
        }
        // Reset only after a stable session, not merely a successful connection.
        if started.elapsed() >= Duration::from_secs(60) {
            delay = Duration::from_secs(1);
        }
        log::warn!(
            "Registration consumer disconnected; retrying in {} seconds",
            delay.as_secs()
        );
        tokio::time::sleep(delay).await;
        delay = next_retry_delay(delay);
    }
}

fn next_retry_delay(delay: Duration) -> Duration {
    (delay * 2).min(Duration::from_secs(30))
}

pub async fn rabbit_consumer(
    db_conn: &mut PgConnection,
    queue_name: ShortString,
) -> Result<String, Error> {
    let conn = tokio::time::timeout(Duration::from_secs(10), rabbit_connection())
        .await
        .map_err(|_| {
            Error::from(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "broker connection timed out",
            ))
        })??;
    let result = consume_registrations(db_conn, queue_name, &conn).await;
    // Also close on stream errors/cancellation and failed acknowledgements.
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        conn.close(200, "consumer restarting".into()),
    )
    .await;
    result
}

async fn consume_registrations(
    db_conn: &mut PgConnection,
    queue_name: ShortString,
    conn: &Connection,
) -> Result<String, Error> {
    let channel = conn.create_channel().await?;
    log::info!("{:?}", conn.status());

    let queue = channel
        .queue_declare(
            queue_name.clone(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await?;
    log::info!("{:?}", conn.status());
    log::info!("Declared queue: {:?}", queue);

    channel.basic_qos(16, BasicQosOptions::default()).await?;

    let mut consumer = channel
        .basic_consume(
            queue_name.clone().into(),
            "consumer".into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;
    log::info!("{:?}", conn.status());

    while let Some(delivery) = consumer.next().await {
        let delivery = delivery?;
        let event = registration_event(&delivery.data);
        match event {
            Some(event) => {
                match replicate_user(db_conn, event.id, event.username, event.email) {
                    Ok(()) => {
                        delivery.ack(BasicAckOptions::default()).await?;
                    }
                    Err(_) => {
                        // Explicitly close so unacknowledged deliveries are requeued
                        // before the supervisor reconnects with a fresh database connection.
                        log::error!(
                            "Registration persistence failed; consumer stopped with delivery unacknowledged"
                        );
                        break;
                    }
                }
            }
            _ => {
                // A configured dead-letter exchange receives rejected events.
                log::warn!("Rejecting invalid or unsupported registration event");
                delivery
                    .reject(BasicRejectOptions { requeue: false })
                    .await?;
            }
        }
    }

    Ok("RabbitMQ consumer stopped".to_string())
}

fn registration_event(payload: &[u8]) -> Option<UserRabbit> {
    serde_json::from_slice::<UserRabbit>(payload)
        .ok()
        .filter(|event| event.interaction_type == "insert" && !event.id.is_nil())
}

#[cfg(test)]
#[path = "rabbitmq_controller_test.rs"]
mod tests;
