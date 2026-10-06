// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn buffer_caps_and_clears() {
    let _guard = TEST_LOCK.lock().unwrap();
    begin();
    for i in 0..(MAX_LINES + 10) {
        push_line(&format!("line {i}"));
    }
    assert_eq!(len(), MAX_LINES);
    let snap = snapshot();
    assert_eq!(snap.first().map(String::as_str), Some("line 10"));
    assert_eq!(
        snap.last().map(String::as_str),
        Some(&*format!("line {}", MAX_LINES + 9))
    );

    // begin() resets the buffer.
    begin();
    push_line("only");
    assert_eq!(snapshot(), vec!["only"]);

    end();
    assert!(!is_active());
}

#[test]
fn snapshots_only_receive_redacted_per_stream_lines() {
    let _guard = TEST_LOCK.lock().unwrap();
    begin();
    let mut stdout = StreamRedactor::default();
    let mut stderr = StreamRedactor::default();

    for line in stdout.push_bytes(b"private_key: |\r\n") {
        push_line(&line);
    }
    for line in stderr.push_bytes(b"stderr remains visible\r\n") {
        push_line(&line);
    }
    let before_secret_body = snapshot().join("\n");
    assert!(before_secret_body.contains("<redacted>"));
    assert!(before_secret_body.contains("stderr remains visible"));
    assert!(!before_secret_body.contains("secret-body"));

    for line in stdout.push_bytes(b"  secret-body-part-one\r\n  secret-body-part-two\r\n") {
        push_line(&line);
    }
    assert!(!snapshot().join("\n").contains("secret-body"));

    for line in stdout.push_bytes(b"safe: next\r\n") {
        push_line(&line);
    }
    assert!(snapshot().join("\n").contains("safe: next"));
    end();
}

#[test]
fn direct_line_push_is_redacted_before_snapshot() {
    let _guard = TEST_LOCK.lock().unwrap();
    begin();
    push_line("token=direct-canary visible");
    let lines = snapshot();
    assert_eq!(lines, vec!["<redacted> visible"]);
    end();
}

#[test]
fn direct_multiline_push_redacts_buildkit_prefixed_blocks() {
    let _guard = TEST_LOCK.lock().unwrap();
    begin();
    push_line("#7 0.1 api_key: |\n#7 0.2   direct-buildkit-canary\n#7 0.3 visible record");
    let lines = snapshot();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("<redacted>"));
    assert!(lines[0].contains("#7 0.3 visible record"));
    assert!(!lines[0].contains("direct-buildkit-canary"));
    end();
}

#[test]
fn separate_push_line_calls_keep_secret_context_between_snapshots() {
    let _guard = TEST_LOCK.lock().unwrap();
    begin();

    push_line("#7 0.1 token = \"\"\"");
    let header = snapshot().join("\n");
    assert!(header.contains("<redacted>"));
    assert!(!header.contains("canary"));

    push_line("#7 0.2 split-canary");
    let during = snapshot().join("\n");
    assert!(!during.contains("split-canary"));

    push_line("#7 0.3 \"\"\"");
    assert!(!snapshot().join("\n").contains("split-canary"));
    push_line("#7 0.4 visible-record");
    assert!(snapshot().join("\n").contains("visible-record"));

    end();
}

#[test]
fn capture_end_resets_open_context_before_later_sink_calls() {
    let _guard = TEST_LOCK.lock().unwrap();
    begin();
    push_line("#7 0.1 token = \"\"\"");
    push_line("#7 0.2 end-boundary-canary");
    assert!(!snapshot().join("\n").contains("end-boundary-canary"));
    end();

    push_line("#7 0.2 visible-after-end");
    let after_end = snapshot().join("\n");
    assert!(after_end.contains("visible-after-end"));
    assert!(!after_end.contains("end-boundary-canary"));

    begin();
    push_line("#7 0.3 visible-after-begin");
    assert_eq!(snapshot(), vec!["#7 0.3 visible-after-begin"]);
    end();
}

#[test]
fn snapshots_suppress_nested_and_mismatched_pem_contexts() {
    let _guard = TEST_LOCK.lock().unwrap();

    begin();
    for line in [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----END RSA PRIVATE KEY-----",
        "snapshot-nested-pem-canary",
        "-----END PRIVATE KEY-----",
    ] {
        push_line(line);
        assert!(!snapshot().join("\n").contains("snapshot-nested-pem-canary"));
    }
    end();

    begin();
    push_line(concat!(
        "-----BEGIN PRIVATE KEY----------BEGIN RSA PRIVATE KEY----------END RSA PRIVATE KEY-----",
        "snapshot-same-line-pem-canary",
        "-----END PRIVATE KEY-----",
    ));
    assert!(
        !snapshot()
            .join("\n")
            .contains("snapshot-same-line-pem-canary")
    );
    end();

    begin();
    push_line("-----BEGIN PRIVATE KEY-----");
    push_line("-----END RSA PRIVATE KEY-----");
    push_line("snapshot-mismatched-pem-canary");
    push_line("-----END PRIVATE KEY-----");
    assert!(
        !snapshot()
            .join("\n")
            .contains("snapshot-mismatched-pem-canary")
    );
    end();
}
