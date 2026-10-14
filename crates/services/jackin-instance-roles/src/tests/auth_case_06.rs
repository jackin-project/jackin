// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn sync_source_dir_rejects_multi_entry_without_writing() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let source_dir = temp.path().join("opencode-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(&auth_json, b"existing-staged-auth").unwrap();
    std::fs::write(
        source_dir.join("auth.json"),
        r#"{"openai":{"type":"api","key":"openai-sentinel"},"zai":{"type":"api","key":"zai-sentinel"}}"#,
    )
    .unwrap();

    let error = provision_opencode_auth_from_source_dir(
        &auth_json,
        AuthForwardMode::Sync,
        &source_dir,
        Some(AiProvider::Zai),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("multiple provider entries"),
        "{error}"
    );
    assert_eq!(std::fs::read(&auth_json).unwrap(), b"existing-staged-auth");
}

#[test]
fn sync_source_dir_copies_direct_grok_auth_json() {
    let temp = tempdir().unwrap();
    let auth_json = temp.path().join("auth.json");
    let source_dir = temp.path().join("grok-work");
    std::fs::create_dir_all(&source_dir).unwrap();
    let expected = r#"{"https://auth.x.ai::workspace":{"key":"jwt"}}"#;
    std::fs::write(source_dir.join("auth.json"), expected).unwrap();

    let (outcome, mounted) =
        provision_grok_auth_from_source_dir(&auth_json, AuthForwardMode::Sync, &source_dir)
            .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(auth_json.as_path()));
    assert_eq!(std::fs::read_to_string(&auth_json).unwrap(), expected);
}

#[test]
fn sync_refreshes_changed_single_file_provider_credentials() {
    let temp = tempdir().unwrap();

    let codex_target = temp.path().join("codex-auth.json");
    let codex_source = temp.path().join("codex-source");
    std::fs::create_dir_all(&codex_source).unwrap();
    std::fs::write(codex_source.join("auth.json"), r#"{"token":"old-codex"}"#).unwrap();
    let (outcome, mounted) =
        provision_codex_auth_from_source_dir(&codex_target, AuthForwardMode::Sync, &codex_source)
            .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(codex_target.as_path()));
    std::fs::write(codex_source.join("auth.json"), r#"{"token":"new-codex"}"#).unwrap();
    let (outcome, mounted) =
        provision_codex_auth_from_source_dir(&codex_target, AuthForwardMode::Sync, &codex_source)
            .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(codex_target.as_path()));
    assert_eq!(
        std::fs::read_to_string(&codex_target).unwrap(),
        r#"{"token":"new-codex"}"#
    );

    let amp_target = temp.path().join("secrets.json");
    let amp_source = temp.path().join("amp-source");
    std::fs::create_dir_all(&amp_source).unwrap();
    std::fs::write(amp_source.join("secrets.json"), r#"{"amp":"old"}"#).unwrap();
    provision_amp_auth_from_source_dir(&amp_target, AuthForwardMode::Sync, &amp_source).unwrap();
    std::fs::write(amp_source.join("secrets.json"), r#"{"amp":"new"}"#).unwrap();
    let (outcome, mounted) =
        provision_amp_auth_from_source_dir(&amp_target, AuthForwardMode::Sync, &amp_source)
            .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(amp_target.as_path()));
    assert_eq!(
        std::fs::read_to_string(&amp_target).unwrap(),
        r#"{"amp":"new"}"#
    );

    let grok_target = temp.path().join("grok-auth.json");
    let grok_source = temp.path().join("grok-source");
    std::fs::create_dir_all(&grok_source).unwrap();
    std::fs::write(grok_source.join("auth.json"), r#"{"grok":"old"}"#).unwrap();
    provision_grok_auth_from_source_dir(&grok_target, AuthForwardMode::Sync, &grok_source).unwrap();
    std::fs::write(grok_source.join("auth.json"), r#"{"grok":"new"}"#).unwrap();
    let (outcome, mounted) =
        provision_grok_auth_from_source_dir(&grok_target, AuthForwardMode::Sync, &grok_source)
            .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert_eq!(mounted.as_deref(), Some(grok_target.as_path()));
    assert_eq!(
        std::fs::read_to_string(&grok_target).unwrap(),
        r#"{"grok":"new"}"#
    );
}

#[test]
fn sync_mode_overwrites_existing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest = simple_manifest(&temp);

    seed_host_auth(&temp);
    let (state, outcome1) = RoleState::prepare(
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
    assert_eq!(outcome1, AuthProvisionOutcome::Synced);

    std::fs::write(
        state.claude_account_json().unwrap(),
        r#"{"container":"data"}"#,
    )
    .unwrap();

    let updated_creds = r#"{"claudeAiOauth":{"accessToken":"new","refreshToken":"new"}}"#;
    std::fs::write(temp.path().join(".claude/.credentials.json"), updated_creds).unwrap();
    drop(state);

    let (state2, outcome2) = RoleState::prepare(
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
        std::fs::read_to_string(state2.claude_credentials_json().unwrap()).unwrap(),
        updated_creds
    );
    assert_eq!(outcome2, AuthProvisionOutcome::Synced);
}

#[test]
fn switching_from_sync_to_ignore_revokes_forwarded_credentials() {
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
    assert!(state.claude_credentials_json().unwrap().exists());
    drop(state);

    let (state2, _) = RoleState::prepare(
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
}

#[test]
fn token_mode_writes_onboarding_skeleton_and_no_credentials() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    seed_host_auth(&temp);
    let manifest = simple_manifest(&temp);

    let (state, outcome) = RoleState::prepare(
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
    assert!(
        !state.claude_credentials_json().unwrap().exists(),
        "token mode must not write .credentials.json"
    );
    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
}

#[test]
fn api_key_mode_wipes_credentials_and_writes_empty_json() {
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
    assert!(
        state.claude_credentials_json().unwrap().exists(),
        "precondition: sync seeded .credentials.json"
    );
    drop(state);

    let (state2, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| AuthForwardMode::ApiKey,
            sync_source_dirs: &|_| None,
        },
        &GithubAuthContext::default(),
        temp.path(),
        Agent::Claude,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
        "{}",
        "api_key mode must reset .claude.json to empty object"
    );
    assert!(
        !state2.claude_credentials_json().unwrap().exists(),
        "api_key mode must wipe .credentials.json"
    );
    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
}

#[test]
fn switching_from_sync_to_token_revokes_forwarded_credentials() {
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
    assert!(state.claude_credentials_json().unwrap().exists());
    drop(state);

    let (state2, outcome) = RoleState::prepare(
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
        std::fs::read_to_string(state2.claude_account_json().unwrap()).unwrap(),
        r#"{"hasCompletedOnboarding":true}"#
    );
    assert!(!state2.claude_credentials_json().unwrap().exists());
    assert_eq!(outcome, AuthProvisionOutcome::TokenMode);
}
