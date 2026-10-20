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
