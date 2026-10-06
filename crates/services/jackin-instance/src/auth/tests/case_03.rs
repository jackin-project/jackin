// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn opencode_source_validation_is_provider_bound_and_rejects_ambiguous_or_db_only_layouts() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("opencode");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("auth.json"),
        r#"{
            "anthropic":{"type":"api","key":"anthropic-sentinel"},
            "opencode-go":{"type":"api","key":"opencode-sentinel"}
        }"#,
    )
    .unwrap();

    for provider in [
        Some(AiProvider::Anthropic),
        Some(AiProvider::Opencode),
        None,
    ] {
        let error =
            validate_sync_source_dir_for_provider(Agent::Opencode, provider, &source, temp.path())
                .unwrap_err();
        assert!(
            error.to_string().contains("multiple provider entries"),
            "{error}"
        );
    }

    std::fs::write(
        source.join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"opencode-sentinel"}}"#,
    )
    .unwrap();
    validate_sync_source_dir_for_provider(
        Agent::Opencode,
        Some(AiProvider::Opencode),
        &source,
        temp.path(),
    )
    .unwrap();
    let error = validate_sync_source_dir_for_provider(
        Agent::Opencode,
        Some(AiProvider::Anthropic),
        &source,
        temp.path(),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("only the opencode-go"),
        "{error}"
    );
    validate_sync_source_dir(Agent::Opencode, &source, temp.path()).unwrap();

    std::fs::write(
        source.join("auth.json"),
        r#"{"zai":{"type":"api","key":"zai-sentinel"}}"#,
    )
    .unwrap();
    let error = validate_sync_source_dir_for_provider(
        Agent::Opencode,
        Some(AiProvider::Zai),
        &source,
        temp.path(),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("only the opencode-go"),
        "{error}"
    );

    std::fs::write(
        source.join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"opencode-sentinel"}}"#,
    )
    .unwrap();
    std::fs::write(source.join("opencode.db"), b"database fixture").unwrap();
    validate_sync_source_dir_for_provider(
        Agent::Opencode,
        Some(AiProvider::Opencode),
        &source,
        temp.path(),
    )
    .unwrap();

    std::fs::remove_file(source.join("auth.json")).unwrap();
    let error = validate_sync_source_dir_for_provider(
        Agent::Opencode,
        Some(AiProvider::Opencode),
        &source,
        temp.path(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("expected auth.json"), "{error}");
}

#[test]
fn hermes_sync_stages_only_the_selected_account_store() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.hermes");
    let target_dir = temp.path().join("role/.hermes");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join("config.yaml"),
        "profiles:\n  work:\n    provider: openai\n",
    )
    .unwrap();
    std::fs::write(
        source_dir.join("auth.json"),
        r#"{"openai":{"type":"api","key":"selected-sentinel"}}"#,
    )
    .unwrap();

    let (outcome, forward_auth) = RoleState::provision_hermes_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
        Some(AiProvider::OpenAi),
        Some(&ProfileSelector {
            entry: "openai".to_owned(),
            profile: Some("work".to_owned()),
        }),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    let staged = std::fs::read_to_string(target_dir.join("auth.json")).unwrap();
    assert!(staged.contains("selected-sentinel"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&staged)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn hermes_sync_rejects_ambiguous_store_before_touching_role_state() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.hermes");
    let target_dir = temp.path().join("role/.hermes");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(
        source_dir.join("config.yaml"),
        "profiles:\n  personal:\n    provider: anthropic\n  work:\n    provider: openai\n",
    )
    .unwrap();
    std::fs::write(
        source_dir.join("auth.json"),
        r#"{"anthropic":{"type":"api","key":"personal-sentinel"},"openai":{"type":"api","key":"work-sentinel"}}"#,
    )
    .unwrap();
    let stale = r#"{"stale":{"type":"api","key":"stale-sentinel"}}"#;
    std::fs::write(target_dir.join("auth.json"), stale).unwrap();

    let error = RoleState::provision_hermes_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
        Some(AiProvider::OpenAi),
        Some(&ProfileSelector {
            entry: "openai".to_owned(),
            profile: Some("work".to_owned()),
        }),
    )
    .unwrap_err();
    assert!(error.to_string().contains("multiple profiles"), "{error}");
    assert_eq!(
        std::fs::read_to_string(target_dir.join("auth.json")).unwrap(),
        stale
    );
    assert!(!target_dir.join("config.yaml").exists());
}

#[test]
fn hermes_sync_replaces_removed_entries_and_revokes_missing_source() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.hermes");
    let target_dir = temp.path().join("role/.hermes");
    std::fs::create_dir_all(source_dir.join("profiles")).unwrap();
    std::fs::write(
        source_dir.join("config.yaml"),
        "profiles:\n  work:\n    provider: openai\n",
    )
    .unwrap();
    std::fs::write(
        source_dir.join("auth.json"),
        r#"{"openai":{"type":"api","key":"selected-sentinel"}}"#,
    )
    .unwrap();
    std::fs::write(source_dir.join(".env"), "STALE_ENV=1\n").unwrap();
    std::fs::write(source_dir.join("profiles/work.yaml"), "provider: openai\n").unwrap();

    let selector = ProfileSelector {
        entry: "openai".to_owned(),
        profile: Some("work".to_owned()),
    };
    let (outcome, forward_auth) = RoleState::provision_hermes_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
        Some(AiProvider::OpenAi),
        Some(&selector),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(target_dir.join(".env").exists());
    assert!(target_dir.join("profiles/work.yaml").exists());

    std::fs::remove_file(source_dir.join(".env")).unwrap();
    std::fs::remove_file(source_dir.join("profiles/work.yaml")).unwrap();
    let (outcome, forward_auth) = RoleState::provision_hermes_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
        Some(AiProvider::OpenAi),
        Some(&selector),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert!(!target_dir.join(".env").exists());
    assert!(!target_dir.join("profiles/work.yaml").exists());
    assert!(target_dir.join("auth.json").exists());

    std::fs::remove_dir_all(&source_dir).unwrap();
    let (outcome, forward_auth) = RoleState::provision_hermes_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
        Some(AiProvider::OpenAi),
        Some(&selector),
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    assert!(forward_auth);
    assert!(target_dir.is_dir());
    assert!(std::fs::read_dir(&target_dir).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn directory_swap_recovers_journal_and_cleans_target_scoped_orphans_on_retry() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.kimi-code");
    let target_dir = temp.path().join("role/.kimi-code");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    std::fs::write(source_dir.join("config.toml"), "version = \"old\"\n").unwrap();
    std::fs::write(source_dir.join("credentials/token"), "old-token").unwrap();
    RoleState::provision_kimi_auth_from_source_dir(&target_dir, AuthForwardMode::Sync, &source_dir)
        .unwrap();

    std::fs::write(source_dir.join("config.toml"), "version = \"new\"\n").unwrap();
    std::fs::write(source_dir.join("credentials/token"), "new-token").unwrap();
    let crash = inject_failure(FailurePoint::Backup);
    let error = RoleState::provision_kimi_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap_err();
    assert!(error.to_string().contains("injected auth directory crash"));
    drop(crash);

    let parent = target_dir.parent().unwrap();
    let current_key = std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .find_map(|name| {
            name.to_str()
                .and_then(|name| name.strip_prefix(".jackin-auth-stage-"))
                .and_then(|suffix| suffix.split('-').next())
                .filter(|key| key.len() == 64)
                .map(str::to_owned)
        })
        .expect("failed swap must leave a target-scoped stage");
    let unrelated_key = "0".repeat(64);
    assert_ne!(current_key, unrelated_key);
    let unrelated_stage = parent.join(format!(
        ".jackin-auth-stage-{unrelated_key}-unrelated/nested"
    ));
    std::fs::create_dir_all(&unrelated_stage).unwrap();
    // Pre-846f984 legacy names have no target identity. They are retained
    // rather than risking deletion of another target's credential tree.
    let unscoped_legacy = parent.join(".jackin-auth-stage-legacy-orphan/nested");
    std::fs::create_dir_all(&unscoped_legacy).unwrap();

    let (outcome, forward_auth) = RoleState::provision_kimi_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap();
    assert_eq!(outcome, AuthProvisionOutcome::Synced);
    assert!(forward_auth);
    assert_eq!(
        std::fs::read_to_string(target_dir.join("config.toml")).unwrap(),
        "version = \"new\"\n"
    );
    assert_eq!(
        std::fs::read_to_string(target_dir.join("credentials/token")).unwrap(),
        "new-token"
    );
    let leftovers = std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| {
            let name = name.to_string_lossy();
            name.starts_with(&format!(".jackin-auth-stage-{current_key}-"))
                || name.starts_with(&format!(".jackin-auth-previous-{current_key}-"))
                || name.starts_with(&format!(".jackin-auth-journal-{current_key}-"))
        })
        .collect::<Vec<_>>();
    assert!(
        leftovers.is_empty(),
        "orphan auth transaction entries: {leftovers:?}"
    );
    assert!(unrelated_stage.parent().unwrap().exists());
    assert!(unscoped_legacy.parent().unwrap().exists());
}

#[cfg(unix)]
#[test]
fn journal_rewrite_failure_preserves_valid_record_for_recovery() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("host/.kimi-code");
    let target_dir = temp.path().join("role/.kimi-code");
    std::fs::create_dir_all(source_dir.join("credentials")).unwrap();
    std::fs::write(source_dir.join("config.toml"), "version = \"old\"\n").unwrap();
    std::fs::write(source_dir.join("credentials/token"), "old-token").unwrap();
    RoleState::provision_kimi_auth_from_source_dir(&target_dir, AuthForwardMode::Sync, &source_dir)
        .unwrap();

    std::fs::write(source_dir.join("config.toml"), "version = \"new\"\n").unwrap();
    std::fs::write(source_dir.join("credentials/token"), "new-token").unwrap();
    let crash = inject_failure(FailurePoint::JournalRewrite);
    let error = RoleState::provision_kimi_auth_from_source_dir(
        &target_dir,
        AuthForwardMode::Sync,
        &source_dir,
    )
    .unwrap_err();
    assert!(error.to_string().contains("injected auth directory crash"));
    drop(crash);

    assert!(
        !target_dir.exists(),
        "backup boundary must leave target absent"
    );
    let parent = target_dir.parent().unwrap();
    let journal = std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".jackin-auth-journal-"))
        })
        .expect("journal must survive failed atomic rewrite");
    let journal_bytes = std::fs::read(&journal).unwrap();
    serde_json::from_slice::<serde_json::Value>(&journal_bytes)
        .expect("journal remains valid JSON after failed rewrite");
    assert!(
        !std::fs::read_dir(parent).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("-tmp-")),
        "failed journal rewrite must remove its temporary file"
    );

    RoleState::provision_kimi_auth_from_source_dir(&target_dir, AuthForwardMode::Sync, &source_dir)
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(target_dir.join("config.toml")).unwrap(),
        "version = \"new\"\n"
    );
    assert_eq!(
        std::fs::read_to_string(target_dir.join("credentials/token")).unwrap(),
        "new-token"
    );
    assert!(
        !std::fs::read_dir(parent).unwrap().any(|entry| {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            name.starts_with(".jackin-auth-stage-")
                || name.starts_with(".jackin-auth-previous-")
                || name.starts_with(".jackin-auth-journal-")
        }),
        "retry must remove all transaction sidecars"
    );
}
