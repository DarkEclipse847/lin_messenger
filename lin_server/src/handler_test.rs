use super::{parse_member_args, parse_room_id};

#[test]
fn member_commands_require_exactly_a_chat_and_non_nil_user() {
    let user = "550e8400-e29b-41d4-a716-446655440000";
    assert!(parse_member_args(&format!("42 {user}")).is_some());
    for args in [
        "".to_owned(),
        "42".into(),
        format!("0 {user}"),
        "42 invalid".into(),
        "42 00000000-0000-0000-0000-000000000000".into(),
        format!("42 {user} extra"),
    ] {
        assert!(parse_member_args(&args).is_none());
    }
}

#[test]
fn room_ids_reject_malformed_and_out_of_database_range_values() {
    for value in [
        "",
        "abc",
        "0",
        "-1",
        "18446744073709551615",
        "18446744073709551616",
    ] {
        assert_eq!(parse_room_id(value), None);
    }
    assert_eq!(parse_room_id(" 42 "), Some(42));
    assert_eq!(parse_room_id("9223372036854775807"), Some(i64::MAX as u64));
}

#[test]
fn history_commands_validate_cursor_and_bounded_page_size() {
    use super::parse_history_args;
    assert_eq!(parse_history_args("42 0"), Some((42, 0, 50)));
    assert_eq!(
        parse_history_args("42 9223372036854775807 100"),
        Some((42, i64::MAX, 100))
    );
    for args in [
        "",
        "42",
        "0 0",
        "42 -1",
        "42 abc",
        "42 9223372036854775808",
        "42 0 0",
        "42 0 101",
        "42 0 1 extra",
    ] {
        assert_eq!(parse_history_args(args), None, "{args}");
    }
}
