use super::*;

#[test]
fn limits_truncate_utf8_after_redaction() {
    let input = "é".repeat(3000);
    let body = clamp_body(&input, Cow::Borrowed);
    assert!(body.len() <= MAX_BODY_BYTES);
    std::str::from_utf8(body.as_bytes()).unwrap();
}
