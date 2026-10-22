use super::registration_event;

#[test]
fn auth_registration_fixture_preserves_uuid_identity() {
    let event = registration_event(include_bytes!(
        "../../../contracts/registration_insert.json"
    ))
    .unwrap();
    assert_eq!(
        event.id,
        uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
    );
    assert_eq!(event.username, "alice");
    assert_eq!(event.email, "alice@example.test");
}

#[test]
fn registration_contract_requires_supported_operation_and_valid_identity() {
    let valid = br#"{"interaction_type":"insert","id":"550e8400-e29b-41d4-a716-446655440000","username":"alice","email":"alice@example.test"}"#;
    let event = registration_event(valid).unwrap();
    assert_eq!(
        event.id,
        uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
    );
    assert_eq!(event.username, "alice");
    for payload in [
        "not JSON",
        r#"{"interaction_type":"insert","id":"00000000-0000-0000-0000-000000000000","username":"alice","email":"a"}"#,
        r#"{"interaction_type":"insert","id":42,"username":"alice","email":"a"}"#,
        r#"{"interaction_type":"insert","id":"550e8400-e29b-41d4-a716-446655440000"}"#,
        r#"{"interaction_type":"delete","id":"550e8400-e29b-41d4-a716-446655440000","username":"alice","email":"a"}"#,
        r#"{"interaction_type":"insert","id":-1,"username":"alice","email":"a"}"#,
    ] {
        assert!(registration_event(payload.as_bytes()).is_none());
    }
}
