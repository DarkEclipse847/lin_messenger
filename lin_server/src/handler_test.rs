use super::parse_room_id;

#[test]
fn room_ids_reject_malformed_and_out_of_database_range_values() {
    for value in [
        "",
        "abc",
        "-1",
        "18446744073709551615",
        "18446744073709551616",
    ] {
        assert_eq!(parse_room_id(value), None);
    }
    assert_eq!(parse_room_id(" 42 "), Some(42));
    assert_eq!(parse_room_id("9223372036854775807"), Some(i64::MAX as u64));
}
