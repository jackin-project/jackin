// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    let mut s = "short".to_owned();
    cap_output(&mut s, 1024);
    assert_eq!(s, "short");
}

#[test]
fn redact_pem_redacts_block_and_counts() {
    let mut s = "before\n-----BEGIN PRIVATE KEY-----\nMIIsecret\n-----END PRIVATE KEY-----\nafter"
        .to_owned();
    let mut count = 0;
    redact_pem(&mut s, &mut count);
    assert!(!s.contains("MIIsecret"));
    assert!(s.contains("[key material redacted by jackin']"));
    assert_eq!(count, 1);
    assert!(s.contains("before") && s.contains("after"));
}

#[test]
fn selected_refs_wire_shape_is_stable() {
    // The host (jackin-runtime exec_host) deserializes this exact JSON via the
    // shared `jackin_protocol::CredRequest`/`ExecBinding`. A field rename breaks
    // credential resolution silently, so pin the on-the-wire shape here.
    let state = ExecPickerState {
        command: "gh".to_owned(),
        args: vec![],
        items: vec![ExecPickerItem {
            binding: jackin_protocol::ExecBinding {
                name: "GH_TOKEN".to_owned(),
                kind: jackin_protocol::ExecKind::Op,
                source: "op://vault/item/field".to_owned(),
            },
            display: "gh".to_owned(),
            selected: true,
        }],
        cursor: 0,
    };
    let req = jackin_protocol::CredRequest {
        ctx: jackin_protocol::TelemetryContext::v1(),
        refs: state.selected_refs(),
    };
    assert_eq!(
        serde_json::to_value(&req).unwrap(),
        serde_json::json!({
            "ctx": { "v": 1 },
            "refs": [{ "name": "GH_TOKEN", "kind": "op", "source": "op://vault/item/field" }]
        })
    );
}

#[test]
fn exec_command_uses_control_request_codec() {
    let framed = frame(&exec_control_request(
        "gh".to_owned(),
        vec!["auth".to_owned(), "status".to_owned()],
        jackin_protocol::TelemetryContext::v1(),
    ));
    let declared = u32::from_be_bytes(framed[..4].try_into().unwrap()) as usize;
    assert_eq!(declared, framed.len() - 4);
    let decoded: ControlRequest = serde_json::from_slice(&framed[4..]).unwrap();
    assert_eq!(decoded.ctx.v, 1);
    assert!(matches!(
        decoded.msg,
        ClientMsg::ExecCommand { command, args }
            if command == "gh" && args == ["auth", "status"]
    ));
}

#[tokio::test]
async fn execute_command_redacts_secret_straddling_1mib_cap() {
    // The redact-before-cap ordering exists so a secret straddling the 1 MiB
    // output cap can't have its tail truncated and leak its verbatim prefix.
    // Drive >1 MiB of output via a file (a 1 MiB argv arg exceeds MAX_ARG_STRLEN),
    // with the secret positioned to straddle the boundary.
    const MAX: usize = 1024 * 1024;
    let secret = "S3CR3T-STRADDLE-TOKEN";
    let mut payload = "a".repeat(MAX - 5);
    payload.push_str(secret); // starts at MAX-5, ends past MAX
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), &payload).unwrap();

    let env = std::collections::BTreeMap::new();
    let (code, stdout, _stderr, redacted) = execute_command(
        "cat",
        &[file.path().to_string_lossy().into_owned()],
        &env,
        &[secret],
    )
    .await
    .unwrap();
    assert_eq!(code, 0);
    assert!(!stdout.contains(secret));
    // Not even a prefix survives — the straddle bug would leave "S3CR3T-...".
    assert!(!stdout.contains("S3CR3T"));
    assert_eq!(redacted, 1);
}

#[tokio::test]
async fn execute_command_redacts_plain_secret() {
    let env = std::collections::BTreeMap::new();
    let (code, stdout, _stderr, redacted) = execute_command(
        "printf",
        &["%s".to_owned(), "tok-SECRET-xyz".to_owned()],
        &env,
        &["tok-SECRET-xyz"],
    )
    .await
    .unwrap();
    assert_eq!(code, 0);
    assert!(!stdout.contains("tok-SECRET-xyz"));
    assert!(stdout.contains("[redacted by jackin']"));
    assert_eq!(redacted, 1);
}
