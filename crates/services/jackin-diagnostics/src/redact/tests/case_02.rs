// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn interleaved_buildkit_secret_envelopes_keep_independent_state() {
    let mut redactor = StreamRedactor::default();
    let mut output = Vec::new();
    for line in [
        b"#7 0.1 token = \"\"\"\n".as_slice(),
        b"#8 0.1 token = \"\"\"\n".as_slice(),
        b"#7 0.2 \"\"\"\n".as_slice(),
        b"#8 0.2 interleaved-canary\n".as_slice(),
        b"#8 0.3 \"\"\"\n".as_slice(),
        b"#8 0.4 visible-record\n".as_slice(),
    ] {
        output.extend(redactor.push_bytes(line));
        assert!(!output.join("\n").contains("interleaved-canary"));
    }
    assert_eq!(
        output,
        vec![
            "#7 0.1 <redacted>",
            "#8 0.1 <redacted>",
            "#8 0.4 visible-record"
        ]
    );

    let whole_text = redact_text(concat!(
        "#7 0.1 token = \"\"\"\n",
        "#8 0.1 token = \"\"\"\n",
        "#7 0.2 \"\"\"\n",
        "#8 0.2 whole-text-interleaved-canary\n",
        "#8 0.3 \"\"\"\n",
        "#8 0.4 visible-record\n",
    ));
    assert!(!whole_text.contains("whole-text-interleaved-canary"));
    assert!(whole_text.contains("#8 0.4 visible-record"));
}

#[test]
fn repeated_step_secret_opener_cannot_close_an_existing_quote() {
    let mut redactor = StreamRedactor::default();
    let mut output = Vec::new();
    for line in [
        b"#7 0.1 token = \"\"\"\n".as_slice(),
        b"#7 0.1 token = \"\"\"\n".as_slice(),
        b"#7 0.2 \"\"\"\n".as_slice(),
        b"#7 0.3 reused-step-canary\n".as_slice(),
    ] {
        output.extend(redactor.push_bytes(line));
        assert!(!output.join("\n").contains("reused-step-canary"));
    }
    assert_eq!(output, vec!["#7 0.1 <redacted>"]);
}

#[test]
fn too_many_open_buildkit_secret_contexts_fail_closed() {
    let mut redactor = StreamRedactor::default();
    let mut output = Vec::new();
    for step in 1..=MAX_ACTIVE_ENVELOPES + 1 {
        let header = format!("#{step} 0.1 token = \"\"\"\n");
        output.extend(redactor.push_bytes(header.as_bytes()));
    }
    assert!(!output.join("\n").contains("canary"));
    assert!(
        redactor
            .push_bytes(b"#1 0.2 overflow-state-canary\n#1 0.3 \"\"\"\n")
            .is_empty()
    );
    assert!(redactor.finish().is_empty());
}

#[test]
fn stream_fails_closed_on_eof_and_unbounded_lines() {
    let mut unterminated = StreamRedactor::default();
    assert_eq!(
        unterminated.push_bytes(b"password: \"secret-that-never-closes\n"),
        vec!["<redacted>"]
    );
    assert!(unterminated.finish().is_empty());

    let mut overflow = StreamRedactor::default();
    let mut too_long = vec![b'x'; MAX_STREAM_LINE_BYTES + 1];
    too_long[..7].copy_from_slice(b"token: ");
    let output = overflow.push_bytes(&too_long);
    assert_eq!(output, vec!["<redacted>"]);
    assert!(
        overflow
            .push_bytes(b"following-token=must-also-stay-hidden\n")
            .is_empty()
    );
    assert!(overflow.finish().is_empty());
    assert!(!output.join("\n").contains("must-also-stay-hidden"));

    let mut invalid_utf8 = StreamRedactor::default();
    assert_eq!(
        invalid_utf8.push_bytes(b"safe-prefix-\xff-canary\n"),
        vec!["<redacted>"]
    );
    assert!(invalid_utf8.push_bytes(b"later-canary\n").is_empty());
    assert!(invalid_utf8.finish().is_empty());
}

#[test]
fn stream_preserves_benign_text_and_redacts_open_pem_at_eof() {
    let mut redactor = StreamRedactor::default();
    assert_eq!(
        redactor.push_bytes(b"Step 1/2: compile\r\nprogress 42%\n"),
        vec!["Step 1/2: compile", "progress 42%"]
    );
    assert_eq!(
        redactor.push_bytes(b"-----BEGIN PRIVATE KEY-----\nsecret-at-eof"),
        vec!["<redacted>"]
    );
    assert!(redactor.finish().is_empty());
    assert_eq!(
        redact_text("before -----BEGIN PRIVATE KEY-----\nprivate-body"),
        "before <redacted>"
    );
    assert_eq!(
        redactor.push_bytes(b"next build output\n"),
        vec!["next build output"]
    );
}
