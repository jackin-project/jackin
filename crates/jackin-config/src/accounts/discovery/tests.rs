// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn environment_discovery_returns_names_without_secret_values() {
    let environment = [
        ("OPENAI_API_KEY".to_owned(), "sensitive-fixture".to_owned()),
        ("ANTHROPIC_API_KEY".to_owned(), "  ".to_owned()),
        (
            "UNRELATED_SECRET".to_owned(),
            "sensitive-fixture".to_owned(),
        ),
    ]
    .into_iter()
    .collect();
    let found = discover_environment_accounts(&environment);
    assert_eq!(found, [(AiProvider::OpenAi, "OPENAI_API_KEY".to_owned())]);
    assert!(!format!("{found:?}").contains("sensitive-fixture"));
}

#[test]
fn environment_candidates_keep_the_matching_endpoint_without_secret_values() {
    let environment = std::collections::BTreeMap::from([
        ("OPENAI_API_KEY".to_owned(), "sensitive-fixture".to_owned()),
        (
            "OPENAI_BASE_URL".to_owned(),
            "https://proxy.example/v1".to_owned(),
        ),
    ]);
    let found = discover_environment_account_candidates(&environment);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].provider, AiProvider::OpenAi);
    assert_eq!(found[0].variable, "OPENAI_API_KEY");
    assert_eq!(
        found[0].base_url.as_deref(),
        Some("https://proxy.example/v1")
    );
    assert!(!format!("{found:?}").contains("sensitive-fixture"));
}

#[test]
fn environment_aliases_use_first_nonempty_reference_per_provider() {
    for (provider, primary, alias) in [
        (AiProvider::Moonshot, "KIMI_API_KEY", "MOONSHOT_API_KEY"),
        (AiProvider::Zai, "ZAI_API_KEY", "ZHIPU_API_KEY"),
        (AiProvider::Minimax, "MINIMAX_API_KEY", "MINIMAX_API_TOKEN"),
    ] {
        let mut environment = [(alias.to_owned(), "alias-fixture".to_owned())]
            .into_iter()
            .collect();
        assert_eq!(
            discover_environment_accounts(&environment),
            [(provider, alias.to_owned())]
        );
        environment.insert(primary.to_owned(), "  ".to_owned());
        assert_eq!(
            discover_environment_accounts(&environment),
            [(provider, alias.to_owned())]
        );
        environment.insert(primary.to_owned(), "primary-fixture".to_owned());
        assert_eq!(
            discover_environment_accounts(&environment),
            [(provider, primary.to_owned())]
        );
    }
}

#[test]
fn recognizes_each_agents_credentials_and_rejects_metadata() {
    let fixtures = [
        (
            Agent::Claude,
            ".credentials.json",
            r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
        ),
        (
            Agent::Codex,
            "auth.json",
            r#"{"tokens":{"access_token":"fixture"}}"#,
        ),
        (
            Agent::Amp,
            "secrets.json",
            r#"{"apiKey@https://ampcode.com":"fixture"}"#,
        ),
        (
            Agent::Kimi,
            "credentials/kimi-code.json",
            r#"{"access_token":"fixture"}"#,
        ),
        (
            Agent::Opencode,
            "auth.json",
            r#"{"opencode-go":{"type":"api","key":"fixture"}}"#,
        ),
        (
            Agent::Grok,
            "auth.json",
            r#"{"https://auth.x.ai::cli":{"key":"fixture"}}"#,
        ),
    ];
    for (agent, filename, content) in fixtures {
        let home = tempfile::tempdir().unwrap();
        let directory = home
            .path()
            .join(agent.runtime().state_paths().credential_dir);
        let path = directory.join(filename);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let inspect = || inspect_directory(agent, &directory, home.path(), |_| false);
        assert_eq!(inspect().unwrap(), None, "empty directory for {agent}");
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(inspect().unwrap(), None, "metadata for {agent}");
        std::fs::write(&path, content).unwrap();
        let found = inspect().unwrap().unwrap();
        assert_eq!(found.evidence, CredentialEvidence::File(path));
        assert!(!format!("{found:?}").contains("fixture"));
    }
}

#[test]
fn opencode_default_discovery_rejects_multi_entry_before_persistence() {
    let home = tempfile::tempdir().unwrap();
    let directory = home
        .path()
        .join(Agent::Opencode.runtime().state_paths().credential_dir);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("auth.json"),
        r#"{
            "anthropic":{"type":"api","key":"anthropic-sentinel"},
            "opencode-go":{"type":"api","key":"opencode-sentinel"}
        }"#,
    )
    .unwrap();

    let report = discover_default_accounts(home.path());
    assert!(
        !report
            .accounts
            .iter()
            .any(|account| account.agent == Agent::Opencode)
    );
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.agent == Agent::Opencode)
        .expect("ambiguous OpenCode auth is reported");
    assert_eq!(
        issue.error,
        DiscoveryError::Unsupported(
            "OpenCode auth.json must contain exactly one provider credential"
        )
    );
    assert!(!format!("{issue:?}").contains("sentinel"));
}

#[test]
fn opencode_default_discovery_uses_auth_entry_when_database_coexists() {
    let home = tempfile::tempdir().unwrap();
    let directory = home
        .path()
        .join(Agent::Opencode.runtime().state_paths().credential_dir);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"opencode-sentinel"}}"#,
    )
    .unwrap();
    std::fs::write(directory.join("opencode.db"), b"database fixture").unwrap();

    let report = discover_default_accounts(home.path());
    let accounts = report
        .accounts
        .iter()
        .filter(|account| account.agent == Agent::Opencode)
        .collect::<Vec<_>>();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].provider, Some(AiProvider::Opencode));
    assert_eq!(accounts[0].directory, directory);
    assert!(
        report
            .issues
            .iter()
            .all(|issue| issue.agent != Agent::Opencode)
    );
    assert!(!format!("{accounts:?}").contains("sentinel"));
}

#[test]
fn opencode_database_only_source_fails_closed_without_registering_account() {
    let home = tempfile::tempdir().unwrap();
    let directory = home
        .path()
        .join(Agent::Opencode.runtime().state_paths().credential_dir);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("opencode.db"), b"database fixture").unwrap();

    let report = discover_default_accounts(home.path());
    assert!(
        !report
            .accounts
            .iter()
            .any(|account| account.agent == Agent::Opencode)
    );
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.agent == Agent::Opencode)
        .expect("unsupported OpenCode database is reported");
    assert_eq!(
        issue.error,
        DiscoveryError::Unsupported(
            "OpenCode database credentials require a source-bound auth.json profile"
        )
    );
    assert!(!format!("{issue:?}").contains("database fixture"));
}

#[test]
fn custom_claude_keychain_scope_never_falls_back_to_default() {
    let home = tempfile::tempdir().unwrap();
    let custom = home.path().join("claude-work");
    let expected = jackin_core::claude_keychain_scope(&custom, home.path(), home.path()).unwrap();
    let result = inspect_directory(Agent::Claude, &custom, home.path(), |service| {
        assert_eq!(service, expected.service);
        assert_ne!(service, jackin_core::CLAUDE_KEYCHAIN_SERVICE_BASE);
        true
    })
    .unwrap()
    .unwrap();
    assert_eq!(
        result.evidence,
        CredentialEvidence::Keychain(expected.service)
    );
}

#[test]
fn malformed_credentials_return_sanitized_error() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("auth.json"), "sensitive malformed value").unwrap();
    let error = inspect_directory(Agent::Codex, home.path(), home.path(), |_| false).unwrap_err();
    assert_eq!(error, DiscoveryError::Malformed);
    assert!(!format!("{error:?} {error}").contains("sensitive"));
}

#[test]
fn oversized_credentials_are_rejected_before_parsing() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("auth.json"), vec![b' '; 1024 * 1024 + 1]).unwrap();
    let error = inspect_directory(Agent::Codex, home.path(), home.path(), |_| false).unwrap_err();
    assert_eq!(error, DiscoveryError::TooLarge);
}

#[test]
fn amp_alias_root_retains_root_and_reports_nested_evidence() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".amp-work");
    let file = root.join("data/amp/secrets.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, r#"{"apiKey@https://ampcode.com":"fixture"}"#).unwrap();
    let result = inspect_directory(Agent::Amp, &root, home.path(), |_| false)
        .unwrap()
        .unwrap();
    assert_eq!(result.directory, root);
    assert_eq!(result.evidence, CredentialEvidence::File(file));
}

#[test]
fn coding_api_aliases_are_discovered() {
    for (provider, name) in [
        (AiProvider::Moonshot, "KIMI_CODE_API_KEY"),
        (AiProvider::Zai, "Z_AI_API_KEY"),
        (AiProvider::Minimax, "MINIMAX_CODING_API_KEY"),
    ] {
        let env = std::collections::BTreeMap::from([(name.into(), "fixture-key".into())]);
        assert_eq!(
            discover_environment_accounts(&env),
            [(provider, name.into())]
        );
        assert!(jackin_core::is_account_env(name));
    }
}

#[test]
fn new_provider_keys_are_discovered() {
    for (provider, name) in [
        (AiProvider::Google, "GEMINI_API_KEY"),
        (AiProvider::Google, "GOOGLE_API_KEY"),
        (AiProvider::Cursor, "CURSOR_API_KEY"),
        (AiProvider::Meta, "META_API_KEY"),
        (AiProvider::OpenRouter, "OPENROUTER_API_KEY"),
    ] {
        let env = std::collections::BTreeMap::from([(name.into(), "fixture-key".into())]);
        assert_eq!(
            discover_environment_accounts(&env),
            [(provider, name.into())]
        );
        assert!(jackin_core::is_account_env(name));
    }
    // Canonical name wins over the alias.
    let env = std::collections::BTreeMap::from([
        ("GEMINI_API_KEY".to_owned(), "primary-fixture".to_owned()),
        ("GOOGLE_API_KEY".to_owned(), "alias-fixture".to_owned()),
    ]);
    assert_eq!(
        discover_environment_accounts(&env),
        [(AiProvider::Google, "GEMINI_API_KEY".to_owned())]
    );
}

#[test]
fn recognizes_new_single_file_agents_and_rejects_metadata() {
    let fixtures = [
        (
            Agent::Gemini,
            "oauth_creds.json",
            r#"{"access_token":"fixture","refresh_token":"fixture"}"#,
        ),
        (
            Agent::Cursor,
            "auth.json",
            r#"{"accessToken":"fixture","refreshToken":"fixture"}"#,
        ),
        (
            Agent::Muse,
            "auth.json",
            r#"{"schema_version":2,"providers":{"meta":{"user_email":"op@example.com"}}}"#,
        ),
    ];
    for (agent, filename, content) in fixtures {
        let home = tempfile::tempdir().unwrap();
        let directory = home
            .path()
            .join(agent.runtime().state_paths().credential_dir);
        let path = directory.join(filename);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let inspect = || inspect_directory(agent, &directory, home.path(), |_| false);
        assert_eq!(inspect().unwrap(), None, "empty directory for {agent}");
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(inspect().unwrap(), None, "metadata for {agent}");
        std::fs::write(&path, content).unwrap();
        let found = inspect().unwrap().unwrap();
        assert_eq!(found.evidence, CredentialEvidence::File(path));
        assert!(!format!("{found:?}").contains("fixture"));
    }
}

#[test]
fn antigravity_discovery_is_keychain_only() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join(".gemini/antigravity-cli");
    std::fs::create_dir_all(&directory).unwrap();
    // settings.json holds prefs, never credentials: no evidence without the
    // Keychain singleton, even when the file exists and parses.
    std::fs::write(directory.join("settings.json"), r#"{"model":"fixture"}"#).unwrap();
    assert_eq!(
        inspect_directory(Agent::Antigravity, &directory, home.path(), |_| false).unwrap(),
        None
    );
    let found = inspect_directory(Agent::Antigravity, &directory, home.path(), |service| {
        assert_eq!(service, "gemini");
        true
    })
    .unwrap()
    .unwrap();
    assert_eq!(
        found.evidence,
        CredentialEvidence::Keychain("gemini".to_owned())
    );
}

#[test]
fn hermes_discovery_enumerates_profiles_through_stores() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join(".hermes");
    std::fs::create_dir_all(&directory).unwrap();
    let inspect = || inspect_directory(Agent::Hermes, &directory, home.path(), |_| false);
    assert_eq!(inspect().unwrap(), None, "empty directory");
    // auth.json alone, without an attributable profile, is not an account.
    std::fs::write(
        directory.join("auth.json"),
        r#"{"openai":{"type":"api","key":"fixture"}}"#,
    )
    .unwrap();
    assert_eq!(inspect().unwrap(), None, "profile-less store");
    std::fs::write(
        directory.join("config.yaml"),
        "profiles:\n  work:\n    provider: openai\n",
    )
    .unwrap();
    let found = inspect().unwrap().unwrap();
    assert_eq!(found.provider, Some(AiProvider::OpenAi));
    assert_eq!(
        found.source_selector,
        Some(ProfileSelector {
            entry: "openai".to_owned(),
            profile: Some("work".to_owned()),
        })
    );
    assert_eq!(
        found.evidence,
        CredentialEvidence::File(directory.join("auth.json"))
    );
    assert!(!format!("{found:?}").contains("fixture"));
}

#[test]
fn hermes_discovery_rejects_ambiguous_profiles_without_exposing_secrets() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join(".hermes");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("config.yaml"),
        "profiles:\n  personal:\n    provider: anthropic\n  work:\n    provider: openai\n",
    )
    .unwrap();
    std::fs::write(
        directory.join("auth.json"),
        r#"{"anthropic":{"type":"api","key":"personal-sentinel"},"openai":{"type":"api","key":"work-sentinel"}}"#,
    )
    .unwrap();

    let error = inspect_directory(Agent::Hermes, &directory, home.path(), |_| false).unwrap_err();
    assert_eq!(
        error,
        DiscoveryError::Unsupported("Hermes credential store contains multiple profiles")
    );
    assert!(!format!("{error:?}").contains("sentinel"));
}

#[test]
fn omp_discovery_without_database_is_not_an_account() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join(".omp");
    std::fs::create_dir_all(&directory).unwrap();
    assert_eq!(
        inspect_directory(Agent::Omp, &directory, home.path(), |_| false).unwrap(),
        None
    );
}

#[test]
fn kimi_default_discovery_accepts_cli_home_without_duplicate_accounts() {
    let home = tempfile::tempdir().unwrap();
    // Keep the default Claude probe filesystem-only on macOS.
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::fs::write(
        home.path().join(".claude/.credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )
    .unwrap();
    for root in [".kimi", ".kimi-code"] {
        let directory = home.path().join(root);
        std::fs::create_dir_all(directory.join("credentials")).unwrap();
        std::fs::write(
            directory.join("credentials/kimi-code.json"),
            r#"{"access_token":"fixture-kimi-token"}"#,
        )
        .unwrap();
        let report = discover_default_accounts(home.path());
        let accounts = report
            .accounts
            .iter()
            .filter(|a| a.agent == Agent::Kimi)
            .collect::<Vec<_>>();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].directory, directory);
    }
}

#[test]
fn kimi_discovery_prefers_live_env_grant_over_drained_base_file() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join(".kimi-code");
    std::fs::create_dir_all(directory.join("credentials")).unwrap();
    std::fs::write(
        directory.join("credentials/kimi-code.json"),
        r#"{"access_token":"","refresh_token":"","expires_at":0,"scope":"kimi-code"}"#,
    )
    .unwrap();
    std::fs::write(
        directory.join("credentials/kimi-code-env-fixture.json"),
        r#"{"access_token":"fixture-live","refresh_token":"fixture-refresh","expires_at":9999999999,"scope":"kimi-code"}"#,
    )
    .unwrap();
    let found = inspect_directory(Agent::Kimi, &directory, home.path(), |_| false)
        .unwrap()
        .unwrap();
    assert_eq!(
        found.evidence,
        CredentialEvidence::File(directory.join("credentials/kimi-code-env-fixture.json"))
    );
    assert!(!format!("{found:?}").contains("fixture-live"));
}

#[test]
fn kimi_discovery_ignores_newer_env_grant_directories() {
    let home = tempfile::tempdir().unwrap();
    let directory = home.path().join(".kimi-code");
    let credentials = directory.join("credentials");
    std::fs::create_dir_all(&credentials).unwrap();
    std::fs::write(
        credentials.join("kimi-code.json"),
        r#"{"access_token":"","refresh_token":"","expires_at":0,"scope":"kimi-code"}"#,
    )
    .unwrap();
    let valid = credentials.join("kimi-code-env-valid.json");
    std::fs::write(
        &valid,
        r#"{"access_token":"fixture-live","refresh_token":"fixture-refresh","expires_at":9999999999,"scope":"kimi-code"}"#,
    )
    .unwrap();
    let newer_directory = credentials.join("kimi-code-env-newer.json");
    std::fs::create_dir(&newer_directory).unwrap();
    filetime::set_file_mtime(&valid, filetime::FileTime::from_unix_time(1, 0)).unwrap();
    filetime::set_file_mtime(&newer_directory, filetime::FileTime::from_unix_time(2, 0)).unwrap();

    let found = inspect_directory(Agent::Kimi, &directory, home.path(), |_| false)
        .unwrap()
        .unwrap();
    assert_eq!(found.evidence, CredentialEvidence::File(valid));
}

#[test]
fn oauth_discovery_keeps_only_nonempty_subscription_reference() {
    let name = jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME;
    for (value, expected) in [("", false), (" ", false), ("fixture-token", true)] {
        let env = std::collections::BTreeMap::from([(name.into(), value.into())]);
        let found = discover_environment_oauth_accounts(&env);
        assert_eq!(!found.is_empty(), expected);
        if expected {
            assert_eq!(found, [(Agent::Claude, name.into())]);
            assert!(!format!("{found:?}").contains(value));
        }
    }
}
