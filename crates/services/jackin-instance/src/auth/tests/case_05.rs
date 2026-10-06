// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn sync_mode_copies_host_auth_on_first_run() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert!(
        std::fs::read_to_string(state.claude_account_json().unwrap())
            .unwrap()
            .contains("test@example.com")
    );
    assert_eq!(
        std::fs::read_to_string(state.claude_credentials_json().unwrap()).unwrap(),
        TEST_CREDENTIALS
    );
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
}

#[test]
fn copy_host_claude_json_copies_present_file() {
    let temp = tempdir().unwrap();
    let host = temp.path().join(".claude.json");
    let dest_dir = temp.path().join("state");
    let dest = dest_dir.join(".claude.json");
    std::fs::create_dir_all(&dest_dir).unwrap();
    std::fs::write(
        &host,
        r#"{"oauthAccount":{"emailAddress":"test@example.com"}}"#,
    )
    .unwrap();

    copy_host_claude_json(&host, &dest).unwrap();

    assert!(
        std::fs::read_to_string(dest)
            .unwrap()
            .contains("test@example.com")
    );
}

#[test]
fn copy_host_claude_json_writes_empty_object_when_absent() {
    let temp = tempdir().unwrap();
    let host = temp.path().join("missing.claude.json");
    let dest_dir = temp.path().join("state");
    let dest = dest_dir.join(".claude.json");
    std::fs::create_dir_all(&dest_dir).unwrap();

    copy_host_claude_json(&host, &dest).unwrap();

    assert_eq!(std::fs::read_to_string(dest).unwrap(), "{}");
}

#[test]
fn copy_host_claude_json_propagates_read_errors_without_writing_empty_object() {
    let temp = tempdir().unwrap();
    let host = temp.path().join(".claude.json");
    let dest_dir = temp.path().join("state");
    let dest = dest_dir.join(".claude.json");
    std::fs::create_dir_all(&host).unwrap();
    std::fs::create_dir_all(&dest_dir).unwrap();

    let err = copy_host_claude_json(&host, &dest).unwrap_err();

    assert!(
        err.to_string().contains("reading Claude account metadata"),
        "error should preserve context: {err}"
    );
    assert!(
        !dest.exists(),
        "read errors must not write a synthetic empty account file"
    );
}

#[test]
fn sync_source_dir_copies_claude_config_dir_without_nested_home_layout() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let source_dir = temp.path().join("claude-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"workspace@example.com"}}"#,
    )
    .unwrap();
    std::fs::write(source_dir.join(".credentials.json"), TEST_CREDENTIALS).unwrap();
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| Some(source_dir.clone()),
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert!(
        std::fs::read_to_string(state.claude_account_json().unwrap())
            .unwrap()
            .contains("workspace@example.com")
    );
    assert_eq!(
        std::fs::read_to_string(state.claude_credentials_json().unwrap()).unwrap(),
        TEST_CREDENTIALS
    );
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
}

#[test]
fn sync_source_dir_does_not_fall_back_to_default_host_credentials() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let source_dir = temp.path().join("claude-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"workspace@example.com"}}"#,
    )
    .unwrap();
    // Default host credentials exist — but the source folder has none, so
    // these must be ignored rather than leaked into the capsule.
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(
        temp.path().join(".claude/.credentials.json"),
        TEST_CREDENTIALS,
    )
    .unwrap();
    let manifest = simple_manifest(&temp);

    let error = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| Some(source_dir.clone()),
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap_err();
    assert!(error.to_string().contains("Not a Claude config folder"));
    let written = std::fs::read_to_string(
        paths
            .data_dir
            .join("jk-agent-smith/claude/credentials.json"),
    )
    .unwrap_or_default();
    assert!(
        !written.contains("accessToken"),
        "invalid selected accounts must never copy default host credentials"
    );
}

#[test]
fn sync_source_dir_uses_source_folder_own_credentials() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let source_dir = temp.path().join("claude-chainargos");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"enterprise@chainargos.com"}}"#,
    )
    .unwrap();
    let source_creds =
        r#"{"claudeAiOauth":{"accessToken":"enterprise","refreshToken":"enterprise"}}"#;
    std::fs::write(source_dir.join(".credentials.json"), source_creds).unwrap();
    // A different default host account is present and must be ignored.
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(
        temp.path().join(".claude/.credentials.json"),
        TEST_CREDENTIALS,
    )
    .unwrap();
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| Some(source_dir.clone()),
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(
        std::fs::read_to_string(state.claude_account_json().unwrap())
            .unwrap()
            .contains("enterprise@chainargos.com")
    );
    assert_eq!(
        std::fs::read_to_string(state.claude_credentials_json().unwrap()).unwrap(),
        source_creds,
        "source folder credentials must win over the default host account"
    );
}

#[test]
fn sync_source_dir_empty_credentials_file_is_not_treated_as_valid() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let source_dir = temp.path().join("claude-empty");
    std::fs::create_dir_all(&source_dir).unwrap();
    // Present but blank — the bug guard: whitespace-only must not count.
    std::fs::write(source_dir.join(".credentials.json"), "   \n").unwrap();
    // A different default host account is present and must never leak in.
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(
        temp.path().join(".claude/.credentials.json"),
        TEST_CREDENTIALS,
    )
    .unwrap();
    let manifest = simple_manifest(&temp);

    let error = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| Some(source_dir.clone()),
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap_err();
    assert!(error.to_string().contains("Not a Claude config folder"));
    let written = std::fs::read_to_string(
        paths
            .data_dir
            .join("jk-agent-smith/claude/credentials.json"),
    )
    .unwrap_or_default();
    assert!(
        !written.contains("accessToken"),
        "invalid selected accounts must never copy default host credentials"
    );
}

#[test]
fn sync_mode_falls_back_to_empty_json_when_host_has_none() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    // No host auth seeded
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
        "{}"
    );
    assert!(!state.claude_credentials_json().unwrap().exists());
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
}

#[test]
fn sync_source_dir_copies_direct_opencode_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let source_dir = temp.path().join("opencode-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"workspace"}}"#,
    )
    .unwrap();
    std::fs::write(source_dir.join("opencode.db"), b"database fixture").unwrap();

    let (outcome, mounted) = RoleState::provision_opencode_auth_from_source_dir(
        &auth_json,
        AuthForwardMode::Sync,
        &source_dir,
        Some(AiProvider::Opencode),
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(auth_json.as_path()));
    let staged: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&auth_json).unwrap()).unwrap();
    assert_eq!(
        staged.pointer("/opencode-go/key").and_then(|v| v.as_str()),
        Some("workspace")
    );
}

#[test]
fn ambient_opencode_whitespace_invalidates_persisted_role_auth() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    std::fs::write(
        &auth_json,
        r#"{"opencode-go":{"type":"api","key":"stale"}}"#,
    )
    .unwrap();
    let host_home = temp.path().join("host-home");
    let source = host_home.join(".local/share/opencode/auth.json");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(source, " \n\t").unwrap();

    let (outcome, mounted) =
        RoleState::provision_opencode_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(mounted.is_none());
    assert!(!auth_json.exists());
}

#[cfg(unix)]
#[test]
fn opencode_valid_credentials_keep_private_file_inode() {
    use std::os::unix::fs::MetadataExt as _;

    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let host_home = temp.path().join("host-home");
    let source = host_home.join(".local/share/opencode/auth.json");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, r#"{"opencode-go":{"type":"api","key":"valid"}}"#).unwrap();

    let (_, mounted) =
        RoleState::provision_opencode_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(mounted, Some(auth_json.clone()));
    let first_inode = std::fs::metadata(&auth_json).unwrap().ino();

    let (_, mounted) =
        RoleState::provision_opencode_auth(&auth_json, AuthForwardMode::Sync, &host_home).unwrap();
    assert_eq!(mounted, Some(auth_json.clone()));
    assert_eq!(std::fs::metadata(auth_json).unwrap().ino(), first_inode);
}
