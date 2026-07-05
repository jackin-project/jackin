use super::resolve_run_log_location;

#[test]
fn run_log_location_empty_run_id_is_unset() {
    assert_eq!(
        resolve_run_log_location("", None),
        ("(not set)".to_owned(), None)
    );
}

#[test]
fn run_log_location_uses_explicit_host_path() {
    assert_eq!(
        resolve_run_log_location("abc123", Some("/tmp/abc123.jsonl")),
        (
            "/tmp/abc123.jsonl".to_owned(),
            Some("file:///tmp/abc123.jsonl".to_owned())
        )
    );
}

#[test]
fn run_log_location_without_path_is_backend_only() {
    assert_eq!(
        resolve_run_log_location("abc123", None),
        ("(backend only - no local file)".to_owned(), None)
    );
}
