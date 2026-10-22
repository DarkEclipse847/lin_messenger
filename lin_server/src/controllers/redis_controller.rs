use std::{io, time::Duration};

use redis::{
    Client,
    aio::{ConnectionManager, ConnectionManagerConfig},
};

const OPERATION_TIMEOUT: Duration = Duration::from_secs(3);
const DEFAULT_URL: &str = "redis://localhost:6379/0";

fn redis_client(url: &str) -> io::Result<Client> {
    let url = if url.is_empty() { DEFAULT_URL } else { url };
    // Do not propagate parser errors that could contain credentials from the URL.
    Client::open(url).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid REDIS_URL"))
}

/// Creates one reconnecting, multiplexed connection shared by all HTTP workers.
/// Auth owns access_token:<user_id> values and their TTLs; setup never changes them.
pub async fn redis_connection() -> io::Result<ConnectionManager> {
    let url = match std::env::var("REDIS_URL") {
        Ok(url) => url,
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "REDIS_URL is not valid Unicode",
            ));
        }
    };
    connect(redis_client(&url)?).await
}

async fn connect(client: Client) -> io::Result<ConnectionManager> {
    let config = ConnectionManagerConfig::new()
        .set_connection_timeout(Some(OPERATION_TIMEOUT))
        .set_response_timeout(Some(OPERATION_TIMEOUT));
    tokio::time::timeout(OPERATION_TIMEOUT, async {
        let mut connection = ConnectionManager::new_with_config(client, config)
            .await
            .map_err(|_| io::Error::other("Redis connection failed"))?;
        let pong: String = redis::cmd("PING")
            .query_async(&mut connection)
            .await
            .map_err(|_| io::Error::other("Redis startup PING failed"))?;
        if pong != "PONG" {
            return Err(io::Error::other("unexpected Redis PING response"));
        }
        Ok(connection)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Redis startup timed out"))?
}

#[cfg(test)]
#[path = "redis_controller_test.rs"]
mod tests;
