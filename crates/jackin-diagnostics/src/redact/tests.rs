use super::{MAX_STREAM_LINE_BYTES, StreamRedactor, redact_and_cap, redact_text};

#[test]
fn redacts_named_secret_values() {
    let input = "token=ghp_abcdefghijklmnopqrstuvwxyz0123456789 keep";
    let redacted = redact_text(input);

    assert_eq!(redacted, "<redacted> keep");
}

#[test]
fn redacts_known_token_shapes_without_keys() {
    let input = "oauth sk-abcdefghijklmnopqrstuvwxyz0123456789 done";
    let redacted = redact_text(input);

    assert_eq!(redacted, "oauth <redacted> done");
}

#[test]
fn redacts_private_key_blocks() {
    let input = "before -----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY----- after";
    let redacted = redact_text(input);

    assert_eq!(redacted, "before <redacted> after");
}

#[test]
fn redacts_long_values_after_assignment_boundary() {
    let input = "digest=0123456789abcdef0123456789abcdef01234567 commit 5d3661cff";
    let redacted = redact_text(input);

    assert_eq!(redacted, "digest<redacted> commit 5d3661cff");
}

#[test]
fn leaves_short_git_shas_alone() {
    let input = "commit 5d3661cff fixed regression";
    let redacted = redact_text(input);

    assert!(matches!(redacted, std::borrow::Cow::Borrowed(_)));
    assert_eq!(redacted, input);
}

#[test]
fn redacts_before_capping() {
    let input = format!(
        "prefix token=ghp_abcdefghijklmnopqrstuvwxyz0123456789 {} tail",
        "x".repeat(128)
    );
    let capped = redact_and_cap(&input, 64);

    assert!(!capped.contains("ghp_"));
    assert!(capped.starts_with("(truncated to 64 bytes)\n"));
    assert!(capped.ends_with("tail"));
    assert!(capped.len() <= 64);
}

#[test]
fn stream_redacts_block_scalars_and_crlf_across_chunks() {
    let mut redactor = StreamRedactor::default();
    assert!(redactor.push_bytes(b"api_key: |").is_empty());
    assert!(redactor.push_bytes(b"\r").is_empty());
    assert_eq!(redactor.push_bytes(b"\n"), vec!["<redacted>"]);
    assert!(redactor
        .push_bytes(b"  multiline-canary-part-one\r\n  multiline-canary-part-two\r\n")
        .is_empty());
    assert_eq!(redactor.push_bytes(b"visible: retained\r\n"), vec!["visible: retained"]);
    assert!(!redactor.finish().iter().any(|line| line.contains("canary")));
}

#[test]
fn stream_hides_pem_body_until_footer_after_arbitrary_splits() {
    let mut redactor = StreamRedactor::default();
    let mut output = Vec::new();
    for chunk in [
        b"prefix -----BEG".as_slice(),
        b"IN PRIVATE KEY-----\r".as_slice(),
        b"\nprivate-key-canary\r\nsecond-secret-line\r\n".as_slice(),
        b"-----END PRIVATE KEY----- suffix\r\nvisible\n".as_slice(),
    ] {
        output.extend(redactor.push_bytes(chunk));
        assert!(!output.join("\n").contains("private-key-canary"));
        assert!(!output.join("\n").contains("second-secret-line"));
    }
    assert_eq!(output, vec!["prefix <redacted>", " suffix", "visible"]);
}

#[test]
fn stream_holds_multiline_quoted_and_structured_values_until_closed() {
    let mut redactor = StreamRedactor::default();
    let first = redactor.push_bytes(b"{\"client_token\": \"quoted-secret-part");
    assert!(first.is_empty());
    let first = redactor.push_bytes(b"-one\nquoted-secret-part-two");
    assert_eq!(first, vec!["{<redacted>"]);
    let middle = redactor.push_bytes(b"-three\n\" , \"safe\": \"visible\"}\n");
    assert_eq!(middle, vec![" , \"safe\": \"visible\"}"]);
    assert!(!middle.join("\n").contains("quoted-secret"));

    let object = redactor.push_bytes(b"token: {\n  nested: \"object-canary\"\n}\nbenign: yes\n");
    assert_eq!(object, vec!["<redacted>", "benign: yes"]);
    assert!(!object.join("\n").contains("object-canary"));

    let folded = redactor.push_bytes(b"password: first-part\n  second-part-canary\nvisible: yes\n");
    assert_eq!(folded, vec!["<redacted>", "visible: yes"]);
    assert!(!folded.join("\n").contains("second-part-canary"));
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
    assert!(overflow
        .push_bytes(b"following-token=must-also-stay-hidden\n")
        .is_empty());
    assert!(overflow.finish().is_empty());
    assert!(!output.join("\n").contains("must-also-stay-hidden"));
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
    assert_eq!(redactor.push_bytes(b"next build output\n"), vec!["next build output"]);
}
