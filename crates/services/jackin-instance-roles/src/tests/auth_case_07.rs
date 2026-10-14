// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn switching_from_token_to_sync_forwards_fresh_host_creds() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::OAuthToken,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(state.claude_account_json().unwrap()).unwrap(),
        r#"{"hasCompletedOnboarding":true}"#
    );
    drop(state);

    let (state2, outcome) = RoleState::prepare(
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
        std::fs::read_to_string(state2.claude_account_json().unwrap())
            .unwrap()
            .contains("test@example.com")
    );
    assert_eq!(
        std::fs::read_to_string(state2.claude_credentials_json().unwrap()).unwrap(),
        TEST_CREDENTIALS
    );
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
}

#[test]
fn switching_from_token_to_ignore_remains_empty() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (_, _) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::OAuthToken,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    let (state2, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
        "{}"
    );
    assert!(!state2.claude_credentials_json().unwrap().exists());
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
}

#[test]
fn sync_mode_preserves_container_auth_when_host_file_missing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    seed_host_auth(&temp);
    let (state, _) = RoleState::prepare(
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

    std::fs::remove_file(temp.path().join(".claude.json")).unwrap();
    std::fs::remove_file(temp.path().join(".claude/.credentials.json")).unwrap();

    let container_auth = r#"{"oauthAccount":{"emailAddress":"container@example.com"}}"#;
    std::fs::write(state.claude_account_json().unwrap(), container_auth).unwrap();
    drop(state);

    let (state2, outcome) = RoleState::prepare(
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
        std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
        container_auth
    );
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
}

#[cfg(unix)]
#[test]
fn auth_file_has_restricted_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, _) = RoleState::prepare(
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

    let perms = std::fs::metadata(state.claude_account_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        perms.mode() & 0o777,
        0o600,
        "claude.json should have 0600 permissions"
    );
    let creds_perms = std::fs::metadata(state.claude_credentials_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        creds_perms.mode() & 0o777,
        0o600,
        ".credentials.json should have 0600 permissions"
    );
}

#[cfg(unix)]
#[test]
fn sync_repairs_permissions_on_legacy_permissive_file() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);
    seed_host_auth(&temp);

    let (state, _) = RoleState::prepare(
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

    std::fs::set_permissions(
        state.claude_account_json().unwrap(),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let perms = std::fs::metadata(state.claude_account_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(perms.mode() & 0o777, 0o644, "precondition: file is 0644");
    drop(state);

    let (state2, _) = RoleState::prepare(
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

    let perms = std::fs::metadata(state2.claude_account_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        perms.mode() & 0o777,
        0o600,
        "sync should repair permissions on existing file"
    );
}

#[cfg(unix)]
#[test]
fn sync_repairs_permissions_when_host_auth_missing() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    seed_host_auth(&temp);
    let (state, _) = RoleState::prepare(
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

    std::fs::set_permissions(
        state.claude_account_json().unwrap(),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let creds_path = state.claude_credentials_json().unwrap();
    std::fs::set_permissions(creds_path, std::fs::Permissions::from_mode(0o644)).unwrap();

    std::fs::remove_file(temp.path().join(".claude.json")).unwrap();
    std::fs::remove_file(temp.path().join(".claude/.credentials.json")).unwrap();
    drop(state);

    let (state2, outcome) = RoleState::prepare(
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
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);

    let json_perms = std::fs::metadata(state2.claude_account_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        json_perms.mode() & 0o777,
        0o600,
        "sync should repair .claude.json permissions even when host auth is missing"
    );
    let creds_perms = std::fs::metadata(state2.claude_credentials_json().unwrap())
        .unwrap()
        .permissions();
    assert_eq!(
        creds_perms.mode() & 0o777,
        0o600,
        "sync should repair .credentials.json permissions even when host auth is missing"
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlink_at_claude_json() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, _) = RoleState::prepare(
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

    let decoy = temp.path().join("decoy.txt");
    std::fs::write(&decoy, "original").unwrap();
    std::fs::remove_file(state.claude_account_json().unwrap()).unwrap();
    std::os::unix::fs::symlink(&decoy, state.claude_account_json().unwrap()).unwrap();
    drop(state);

    let err = RoleState::prepare(
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
    .unwrap_err();
    assert!(
        err.to_string().contains("symlink"),
        "expected symlink error, got: {err}"
    );

    assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "original");
}
