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
mod tests {
    use super::*;

    #[test]
    fn empty_url_uses_auth_default_and_invalid_urls_are_redacted() {
        assert_eq!(
            redis_client("")
                .unwrap()
                .get_connection_info()
                .redis_settings()
                .db(),
            0
        );
        let error = redis_client("https://user:secret@localhost").unwrap_err();
        assert_eq!(error.to_string(), "invalid REDIS_URL");
    }

    #[tokio::test]
    async fn unavailable_redis_fails_startup() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        // An open TCP socket that never answers Redis commands exercises the timeout.
        let result = connect(redis_client(&format!("redis://{address}/0")).unwrap()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "requires a disposable Redis instance at TEST_REDIS_URL"]
    async fn live_connection_can_be_shared_by_workers() {
        let url = std::env::var("TEST_REDIS_URL").expect("set TEST_REDIS_URL");
        let connection = connect(redis_client(&url).unwrap()).await.unwrap();
        let mut first = connection.clone();
        let mut second = connection.clone();
        for connection in [&mut first, &mut second] {
            let pong: String = redis::cmd("PING").query_async(connection).await.unwrap();
            assert_eq!(pong, "PONG");
        }
    }
}
