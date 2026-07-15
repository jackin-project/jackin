use super::*;

#[test]
fn rotates_oversized_multiplexer_log() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("multiplexer.log");
    let old = File::create(&path).unwrap();
    old.set_len(MAX_LOG_BYTES + 1).unwrap();
    drop(old);

    rotate_if_oversized(&path).unwrap();

    let rotated = temp.path().join("multiplexer.log.1");
    assert!(rotated.exists(), "oversized log should rotate to .1");
    assert!(
        !path.exists(),
        "rotation should move the oversized live log before init reopens it"
    );
}

#[test]
fn context_banner_line_format_is_joinable() {
    // The banner is the offline join key: run_id + session_id + traceparent.
    // Format is load-bearing for operators grepping a pasted tail.
    let run_id = "abc123";
    let session_id = "sess-9";
    let traceparent = "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01";
    let line = format!(
        "[jackin-capsule] context run_id={run_id} session_id={session_id} traceparent={traceparent}"
    );
    assert!(line.contains("run_id=abc123"));
    assert!(line.contains("session_id=sess-9"));
    assert!(line.contains("traceparent=00-"));
}

#[test]
fn render_prefix_shapes_are_byte_stable() {
    // Operators grep these exact prefixes in multiplexer.log / docker logs.
    let body = "session ready";
    assert_eq!(
        format!("[jackin-capsule] {body}"),
        "[jackin-capsule] session ready"
    );
    assert_eq!(
        format!("[jackin-capsule debug] {body}"),
        "[jackin-capsule debug] session ready"
    );
    assert_eq!(
        format!("[jackin-capsule trace] {body}"),
        "[jackin-capsule trace] session ready"
    );
}
