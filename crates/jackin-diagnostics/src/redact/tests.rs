use super::{redact_and_cap, redact_text};

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
