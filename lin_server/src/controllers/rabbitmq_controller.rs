use diesel::PgConnection;
use futures_util::StreamExt;
use lapin::{
    Connection, ConnectionProperties, Error, options::*, types::{FieldTable, ShortString}
};

use crate::{controllers::models::UserRabbit, database::db::new_user};

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

pub async fn rabbit_consumer(db_conn: &mut PgConnection,queue_name: ShortString)-> Result<String, Error>{
    let conn = rabbit_connection().await.unwrap();
    let channel = conn.create_channel().await?;
    log::info!("{:?}",conn.status());

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
    log::info!("{:?}",conn.status());
    log::info!("Declared queue: {:?}", queue);

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
        log::info!("received message: {:?}", delivery);
        
        //let data = &delivery.data;
        
        let payload = &delivery.unwrap().data;
        match serde_json::from_slice::<UserRabbit>(payload){
            Ok(json_body) => {
                if json_body.interaction_type == "insert" {
                    log::info!("{:?}",new_user(db_conn, json_body.id, json_body.username, json_body.email))
                }
            }
            Err(e) => {
                log::error!("Failed to parse JSON: {}", e)
            }
        }
        //if let Ok(delivery) = delivery {
        //    //let delivery = delivery;
        //    let json_data:  = serde_json::from_slice(&delivery.data);
        //    delivery.ack(BasicAckOptions::default()).await.expect("Failed to ack");
        //}
    }

    Ok("RabbitMQ consumer initialized".to_string()) 
}