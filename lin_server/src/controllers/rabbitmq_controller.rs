use diesel::PgConnection;
use futures_util::StreamExt;
use lapin::{
    BasicProperties, Connection, ConnectionProperties, Error, options::*, types::{FieldTable, ShortString}
};
use serde_json::Value;
use tokio;

use crate::{controllers::models::UserRabbit, database::db::new_user};
//use database::db;

// https://youtu.be/QwoF1-1QgwA?si=H3IZSgPmXDgnpk-c
// lol i've coded this drunk af on a party, and when my playlist suddenly played this song, i saved it here
// enjoy :)
// i just opened link above, hoping that this is some kind of tutorial on how to use lapin and found this

pub async fn RabbitConnection() -> Result<Connection, Error>{
    let addr = "amqp://user:password@localhost:5672/%2f";
    let connection = Connection::connect(&addr, ConnectionProperties::default())
        .await
        .expect("Failed to connect to rabbitmq");
    Ok(connection)
}

pub async fn RabbitConsumer(db_conn: &mut PgConnection,queue_name: ShortString)-> Result<String, Error>{
    let conn = RabbitConnection().await.unwrap();
    let channel = conn.create_channel().await?;
    log::info!("{:?}",conn.status());

    let queue = channel.queue_declare(queue_name.clone(), QueueDeclareOptions::default(), FieldTable::default(),).await?;
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

    Ok("".to_string()) 
}