use super::*;

#[test]
fn privacy_is_allowlist_first() {
    validate_key(schema::attrs::OUTCOME).unwrap();
    assert_eq!(
        validate_key("user.secret"),
        Err(Rejection::UnknownAttribute)
    );
}
