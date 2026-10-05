// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `build_log`.
use super::*;
use crate::redact::StreamRedactor;

// The buffer is process-global, so every test below takes the same lock.
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
