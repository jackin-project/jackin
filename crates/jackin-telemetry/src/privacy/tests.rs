use super::*;

#[test]
fn privacy_is_allowlist_first() {
    validate_key(schema::attrs::OUTCOME).unwrap();
    assert_eq!(
        validate_key("user.secret"),
        Err(Rejection::UnknownAttribute)
    );
}

#[test]
fn prohibited_values_are_privacy_rejections() {
    for value in [
        "/home/operator/workspace",
        "https://example.invalid/api?token=secret",
        "password=secret",
        "\u{1b}[31mterminal payload",
    ] {
        assert_eq!(validate_string(value), Err(Rejection::Privacy), "{value:?}");
    }
    assert_eq!(
        validate_value(&Value::StrArray(&["safe", "token=secret"])),
        Err(Rejection::Privacy)
    );
    validate_string("workspace_not_found").unwrap();
}
