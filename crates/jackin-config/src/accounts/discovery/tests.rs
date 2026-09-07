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
            r#"{"anthropic":{"type":"oauth","refresh":"fixture"}}"#,
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
