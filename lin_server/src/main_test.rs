use super::*;
use actix_web::test::TestRequest;

#[test]
fn identity_requires_a_non_nil_uuid() {
    assert_eq!(
        authenticated_user_id(&TestRequest::default().to_http_request()),
        None
    );
    for value in [
        "0",
        "-1",
        "abc",
        "9223372036854775808",
        "00000000-0000-0000-0000-000000000000",
    ] {
        let req = TestRequest::default()
            .insert_header(("X-User-Id", value))
            .to_http_request();
        assert_eq!(authenticated_user_id(&req), None);
    }
    let req = TestRequest::default()
        .insert_header(("X-User-Id", "550e8400-e29b-41d4-a716-446655440000"))
        .to_http_request();
    assert_eq!(
        authenticated_user_id(&req),
        Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap())
    );
}
