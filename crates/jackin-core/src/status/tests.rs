use super::parse_session_count;

#[test]
fn parse_session_count_extracts_header_value() {
    let body = "Sessions: 3\n[1] alpha (claude) state=running active=true\n";
    assert_eq!(parse_session_count(body), Some(3));
}

#[test]
fn parse_session_count_handles_leading_whitespace() {
    let body = "   Sessions: 7\n";
    assert_eq!(parse_session_count(body), Some(7));
}

#[test]
fn parse_session_count_missing_header_returns_none() {
    let body = "[1] alpha (claude) state=running active=true\n";
    assert_eq!(parse_session_count(body), None);
}

#[test]
fn parse_session_count_zero_is_distinguished_from_missing() {
    let body = "Sessions: 0\n";
    assert_eq!(parse_session_count(body), Some(0));
}

#[test]
fn parse_session_count_rejects_non_numeric_value() {
    let body = "Sessions: many\n";
    assert_eq!(parse_session_count(body), None);
}
