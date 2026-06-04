//! Tests for `exec`.
use super::*;

#[test]
fn cap_output_truncates_on_char_boundary() {
    // 'é' is 2 bytes, placed so byte index 10 falls mid-codepoint. Capping
    // at 10 must round down to a boundary (9) instead of panicking.
    let mut s = "a".repeat(9) + "é" + &"b".repeat(20);
    cap_output(&mut s, 10);
    assert!(s.starts_with("aaaaaaaaa"));
    assert!(!s.contains('é'));
    assert!(s.contains("[output truncated"));
}

#[test]
fn cap_output_leaves_short_output_untouched() {
    let mut s = "short".to_string();
    cap_output(&mut s, 1024);
    assert_eq!(s, "short");
}

#[test]
fn redact_pem_redacts_block_and_counts() {
    let mut s = "before\n-----BEGIN PRIVATE KEY-----\nMIIsecret\n-----END PRIVATE KEY-----\nafter"
        .to_string();
    let mut count = 0;
    redact_pem(&mut s, &mut count);
    assert!(!s.contains("MIIsecret"));
    assert!(s.contains("[key material redacted by jackin']"));
    assert_eq!(count, 1);
    assert!(s.contains("before") && s.contains("after"));
}

#[tokio::test]
async fn execute_command_redacts_plain_secret() {
    let env = std::collections::BTreeMap::new();
    let (code, stdout, _stderr, redacted) = execute_command(
        "printf",
        &["%s".to_string(), "tok-SECRET-xyz".to_string()],
        &env,
        &["tok-SECRET-xyz".to_string()],
    )
    .await
    .unwrap();
    assert_eq!(code, 0);
    assert!(!stdout.contains("tok-SECRET-xyz"));
    assert!(stdout.contains("[redacted by jackin']"));
    assert_eq!(redacted, 1);
}
