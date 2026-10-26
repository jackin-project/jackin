// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn redacts_named_secret_values() {
    let input = "token=ghp_abcdefghijklmnopqrstuvwxyz0123456789 keep";
    let redacted = redact_text(input);

    assert_eq!(redacted, "<redacted> keep");
}

#[test]
fn redacts_compound_credential_assignments() {
    for key in [
        "GH_TOKEN",
        "XAI_API_KEY",
        "client_token",
        "db_password",
        "access_key",
        "service-client-token",
        "private_key",
        "oauth_token_secret",
    ] {
        let input = format!("before {key}=short-canary after");
        assert_eq!(redact_text(&input), "before <redacted> after", "{key}");
    }
}

#[test]
fn redacts_quoted_credential_keys_and_complete_values() {
    for input in [
        r#"{"client_token": "short canary", "safe": "keep"}"#,
        r#"{"db_password": "canary\"continued", "safe": "keep"}"#,
        "{'db_password': 'short canary', 'safe': 'keep'}",
    ] {
        let redacted = redact_text(input);
        assert!(!redacted.contains("canary"), "{redacted}");
        assert!(!redacted.contains("continued"), "{redacted}");
        assert!(redacted.contains("<redacted>"));
        assert!(redacted.contains("keep"));
    }
}

#[test]
fn redacts_quoted_authorization_bearer_value() {
    let input = r#"{"Authorization": "Bearer short-canary", "safe": "keep"}"#;
    assert_eq!(redact_text(input), r#"{<redacted>, "safe": "keep"}"#);
}

#[test]
fn redacts_unclosed_quoted_credential_values() {
    for input in [r#"client_token="short canary"#, "db_password='short canary"] {
        assert_eq!(redact_text(input), "<redacted>");
    }
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

    assert_eq!(redacted, "before <redacted>\n after");
}

#[test]
fn sequential_complete_pem_blocks_keep_their_individual_boundaries() {
    let input = concat!(
        "-----BEGIN PRIVATE KEY-----first-body-----END PRIVATE KEY-----",
        " safe text ",
        "-----BEGIN RSA PRIVATE KEY-----second-body-----END RSA PRIVATE KEY-----",
        " visible text",
    );
    let expected = "<redacted> safe text <redacted> visible text";
    assert_eq!(redact_text(input), expected);

    let mut redactor = StreamRedactor::default();
    assert_eq!(
        redactor.push_bytes(format!("{input}\n").as_bytes()),
        vec![expected]
    );
}

#[test]
fn nested_and_mismatched_pem_markers_fail_closed_in_whole_text_sinks() {
    let nested = concat!(
        "-----BEGIN PRIVATE KEY-----\n",
        "-----BEGIN RSA PRIVATE KEY-----\n",
        "-----END RSA PRIVATE KEY-----\n",
        "nested-pem-canary\n",
        "-----END PRIVATE KEY-----\n",
    );
    assert!(!redact_text(nested).contains("nested-pem-canary"));
    assert!(!redact_and_cap(nested, 4096).contains("nested-pem-canary"));

    let same_line = concat!(
        "-----BEGIN PRIVATE KEY----------BEGIN RSA PRIVATE KEY----------END RSA PRIVATE KEY-----",
        "same-line-pem-canary",
        "-----END PRIVATE KEY-----",
    );
    assert!(!redact_text(same_line).contains("same-line-pem-canary"));

    let mismatched = concat!(
        "-----BEGIN PRIVATE KEY-----\n",
        "-----END RSA PRIVATE KEY-----\n",
        "mismatched-footer-canary\n",
        "-----END PRIVATE KEY-----\n",
    );
    assert!(!redact_text(mismatched).contains("mismatched-footer-canary"));
    assert!(!redact_and_cap(mismatched, 4096).contains("mismatched-footer-canary"));
}

#[test]
fn quote_and_pem_contexts_cannot_close_each_other() {
    let quote_inside_pem = concat!(
        "-----BEGIN PRIVATE KEY-----\n",
        "token = \"\"\"\n",
        "-----END PRIVATE KEY-----\n",
        "quote-inside-pem-canary\n",
        "\"\"\"\n",
        "-----END PRIVATE KEY-----\n",
    );
    assert!(!redact_text(quote_inside_pem).contains("quote-inside-pem-canary"));

    let pem_inside_quote = concat!(
        "token = \"\"\"\n",
        "-----BEGIN PRIVATE KEY-----\n",
        "pem-inside-quote-body\n",
        "-----END PRIVATE KEY-----\n",
        "pem-inside-quote-canary\n",
        "\"\"\"\n",
        "visible: retained\n",
    );
    let redacted = redact_text(pem_inside_quote);
    assert!(!redacted.contains("pem-inside-quote-body"));
    assert!(!redacted.contains("pem-inside-quote-canary"));
    assert!(redacted.contains("visible: retained"));
}

#[test]
fn pem_label_overflow_fails_closed() {
    let label = format!("{}PRIVATE KEY", "A".repeat(MAX_PEM_LABEL_BYTES + 1));
    let input = format!("-----BEGIN {label}-----\noverflow-label-canary\n-----END {label}-----\n");
    assert!(!redact_text(&input).contains("overflow-label-canary"));
    assert!(!redact_and_cap(&input, 4096).contains("overflow-label-canary"));
}

#[test]
fn whole_text_redaction_consumes_authorization_schemes_and_yaml_blocks() {
    assert_eq!(redact_text("Authorization=Basic canary"), "<redacted>");

    let redacted =
        redact_text("api_key: |2\n    first-canary\n  second-canary\nvisible: retained\n");
    assert!(!redacted.contains("first-canary"));
    assert!(!redacted.contains("second-canary"));
    assert!(redacted.contains("<redacted>"));
    assert!(redacted.contains("visible: retained"));

    let under_indented =
        redact_text("api_key: |2\n first-under-indented-canary\nvisible: retained\n");
    assert!(!under_indented.contains("first-under-indented-canary"));
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
    assert!(
        redactor
            .push_bytes(b"  multiline-canary-part-one\r\n  multiline-canary-part-two\r\n")
            .is_empty()
    );
    assert_eq!(
        redactor.push_bytes(b"visible: retained\r\n"),
        vec!["visible: retained"]
    );
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
fn stream_pem_fails_closed_on_nested_and_mismatched_markers() {
    let mut nested = StreamRedactor::default();
    let mut output = Vec::new();
    for line in [
        b"-----BEGIN PRIVATE KEY-----\n".as_slice(),
        b"-----BEGIN RSA PRIVATE KEY-----\n".as_slice(),
        b"-----END RSA PRIVATE KEY-----\n".as_slice(),
        b"nested-stream-pem-canary\n".as_slice(),
        b"-----END PRIVATE KEY-----\n".as_slice(),
    ] {
        output.extend(nested.push_bytes(line));
        assert!(!output.join("\n").contains("nested-stream-pem-canary"));
    }
    assert_eq!(output, vec!["<redacted>"]);

    let mut same_line = StreamRedactor::default();
    let output = same_line.push_bytes(concat!(
        "-----BEGIN PRIVATE KEY----------BEGIN RSA PRIVATE KEY----------END RSA PRIVATE KEY-----",
        "same-line-stream-canary",
        "-----END PRIVATE KEY-----\n",
    ).as_bytes());
    assert_eq!(output, vec!["<redacted>"]);
    assert!(!output.join("\n").contains("same-line-stream-canary"));

    let mut mismatched = StreamRedactor::default();
    let output = mismatched.push_bytes(
        concat!(
            "-----BEGIN PRIVATE KEY-----\n",
            "-----END RSA PRIVATE KEY-----\n",
            "mismatched-stream-canary\n",
        )
        .as_bytes(),
    );
    assert_eq!(output, vec!["<redacted>"]);
    assert!(!output.join("\n").contains("mismatched-stream-canary"));
}

#[test]
fn stream_fails_closed_when_pem_label_exceeds_its_bound() {
    let label = format!("{}PRIVATE KEY", "A".repeat(MAX_PEM_LABEL_BYTES + 1));
    let input =
        format!("-----BEGIN {label}-----\noverflow-stream-pem-canary\n-----END {label}-----\n");
    let mut redactor = StreamRedactor::default();
    let output = redactor.push_bytes(input.as_bytes());
    assert_eq!(output, vec!["<redacted>"]);
    assert!(!output.join("\n").contains("overflow-stream-pem-canary"));
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
fn stream_redacts_authorization_bearer_as_one_value() {
    let mut redactor = StreamRedactor::default();
    assert_eq!(
        redactor.push_bytes(b"Authorization=Bearer canary\n"),
        vec!["<redacted>"]
    );
    assert_eq!(
        redactor.push_bytes(b"Authorization=Bearer\n"),
        vec!["<redacted>"]
    );
    assert!(
        redactor
            .push_bytes(b"split-bearer-canary\nvisible-but-suppressed\n")
            .is_empty()
    );
    assert_eq!(redact_text("Authorization=Bearer canary"), "<redacted>");
}

#[test]
fn stream_holds_triple_quoted_values_until_the_full_delimiter() {
    let mut redactor = StreamRedactor::default();
    assert_eq!(redactor.push_bytes(b"token = \"\"\"\n"), vec!["<redacted>"]);
    assert!(redactor.push_bytes(b"canary\n").is_empty());
    assert_eq!(
        redactor.push_bytes(b"\"\"\"\nvisible: retained\n"),
        vec!["visible: retained"]
    );
    assert_eq!(redact_text("token = \"\"\"\ncanary\n\"\"\""), "<redacted>");
}

#[test]
fn pem_footer_cannot_close_an_outer_triple_quoted_secret() {
    let input = concat!(
        "token = \"\"\"\n",
        "-----END PRIVATE KEY-----\n",
        "remaining-token-canary\n",
        "\"\"\"\n",
        "visible: retained\n",
    );
    let redacted = redact_text(input);
    assert!(!redacted.contains("remaining-token-canary"));
    assert!(redacted.contains("visible: retained"));
}

#[test]
fn ambiguous_suffixes_and_pem_values_remain_suppressed() {
    assert!(!redact_text("token=\"quoted-secret\"canary").contains("canary"));

    let redacted = redact_text(concat!(
        "token=prefix -----BEGIN PRIVATE KEY-----\n",
        "embedded-pem-canary\n",
        "-----END PRIVATE KEY----- suffix",
    ));
    assert!(!redacted.contains("prefix"));
    assert!(!redacted.contains("embedded-pem-canary"));
}

#[test]
fn buildkit_block_secret_stays_bound_to_its_step() {
    let mut redactor = StreamRedactor::default();
    let mut output = Vec::new();
    for chunk in [
        b"#7 0.1 api_key: |\r".as_slice(),
        b"\n#7 0.2   canary\r\n".as_slice(),
        b"#8 0.1 harmless-other-step\r\n".as_slice(),
        b"unframed-ambiguous-canary\r\n".as_slice(),
        b"#7 malformed-envelope-canary\r\n".as_slice(),
        b"#7 0.3   canary-continuation\r\n".as_slice(),
        b"#7 0.4 next-safe-record\r\n".as_slice(),
    ] {
        output.extend(redactor.push_bytes(chunk));
        assert!(!output.join("\n").contains("canary"));
    }
    assert_eq!(
        output,
        vec![
            "#7 0.1 <redacted>",
            "#8 0.1 harmless-other-step",
            "#7 0.4 next-safe-record"
        ]
    );
}
