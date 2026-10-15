use diesel::PgConnection;
use futures_util::StreamExt;
use lapin::{
    Connection, ConnectionProperties, Error,
    options::*,
    types::{FieldTable, ShortString},
};

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

pub async fn rabbit_consumer(
    db_conn: &mut PgConnection,
    queue_name: ShortString,
) -> Result<String, Error> {
    let conn = rabbit_connection().await?;
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
                        // Leave the delivery unacknowledged and stop; closing the channel
                        // requeues it without a tight retry loop or silently losing it.
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
        .filter(|event| event.interaction_type == "insert" && event.id > 0)
}

#[cfg(test)]
mod tests {
    use super::registration_event;

    #[test]
    fn auth_registration_fixture_preserves_full_i64_identity() {
        let event = registration_event(include_bytes!(
            "../../../contracts/registration_insert.json"
        ))
        .unwrap();
        assert_eq!(event.id, i64::MAX);
        assert_eq!(event.username, "alice");
        assert_eq!(event.email, "alice@example.test");
    }

    #[test]
    fn registration_contract_requires_supported_operation_and_valid_identity() {
        let valid = br#"{"interaction_type":"insert","id":42,"username":"alice","email":"alice@example.test"}"#;
        let event = registration_event(valid).unwrap();
        assert_eq!(event.id, 42);
        assert_eq!(event.username, "alice");
        for payload in [
            "not JSON",
            r#"{"interaction_type":"insert","id":42}"#,
            r#"{"interaction_type":"delete","id":42,"username":"alice","email":"a"}"#,
            r#"{"interaction_type":"insert","id":-1,"username":"alice","email":"a"}"#,
        ] {
            assert!(registration_event(payload.as_bytes()).is_none());
        }
    }
}
