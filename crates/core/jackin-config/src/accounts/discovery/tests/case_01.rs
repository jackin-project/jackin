// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn codex_discovery_uses_explicit_home_without_probing_default() {
    let home = tempfile::tempdir().unwrap();
    let default = home.path().join(".codex");
    let explicit = home.path().join("codex-work");
    write_codex_fixture(&default, "default-sentinel");
    write_codex_fixture(&explicit, "override-sentinel");

    let report = discover_default_accounts_with_codex_home(home.path(), Some(explicit.as_os_str()));
    let accounts = codex_accounts(&report);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].directory, explicit);
    assert_eq!(
        accounts[0].evidence,
        CredentialEvidence::File(home.path().join("codex-work/auth.json"))
    );
    let rendered = format!("{report:?}");
    assert!(!rendered.contains("default-sentinel"));
    assert!(!rendered.contains("override-sentinel"));
}

#[test]
fn codex_discovery_resolves_relative_home_from_working_directory() {
    let home = tempfile::tempdir().unwrap();
    let working_directory = std::env::current_dir().unwrap();
    let relative = tempfile::Builder::new()
        .prefix("jackin-codex-home-relative-")
        .tempdir_in(&working_directory)
        .unwrap();
    let relative_path = relative
        .path()
        .strip_prefix(&working_directory)
        .expect("relative fixture is beneath working directory");
    write_codex_fixture(relative.path(), "relative-override-sentinel");

    let report =
        discover_default_accounts_with_codex_home(home.path(), Some(relative_path.as_os_str()));
    let accounts = codex_accounts(&report);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].directory, working_directory.join(relative_path));
    assert!(!format!("{report:?}").contains("relative-override-sentinel"));
}

#[test]
fn codex_discovery_uses_home_default_only_when_override_is_unset() {
    let home = tempfile::tempdir().unwrap();
    let default = home.path().join(".codex");
    write_codex_fixture(&default, "default-sentinel");

    let report = discover_default_accounts_with_codex_home(home.path(), None);
    let accounts = codex_accounts(&report);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].directory, default);
    assert!(!format!("{report:?}").contains("default-sentinel"));
}

#[test]
fn missing_explicit_codex_home_does_not_fall_back_to_default() {
    let home = tempfile::tempdir().unwrap();
    write_codex_fixture(&home.path().join(".codex"), "fallback-sentinel");
    let missing = home.path().join("missing-codex-home");

    let report = discover_default_accounts_with_codex_home(home.path(), Some(missing.as_os_str()));
    assert!(codex_accounts(&report).is_empty());
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.agent == Agent::Codex)
        .expect("missing explicit home is reported");
    assert_eq!(issue.directory, PathBuf::from("CODEX_HOME"));
    assert_eq!(
        issue.error,
        DiscoveryError::Unsupported("CODEX_HOME path does not exist")
    );
    assert!(!format!("{report:?}").contains("fallback-sentinel"));
}

#[test]
fn explicit_codex_file_path_is_rejected_without_fallback() {
    let home = tempfile::tempdir().unwrap();
    write_codex_fixture(&home.path().join(".codex"), "fallback-sentinel");
    let explicit_file = home.path().join("codex-home-file");
    std::fs::write(&explicit_file, "synthetic non-directory").unwrap();

    let report =
        discover_default_accounts_with_codex_home(home.path(), Some(explicit_file.as_os_str()));
    assert!(codex_accounts(&report).is_empty());
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.agent == Agent::Codex)
        .expect("non-directory explicit home is reported");
    assert_eq!(issue.directory, PathBuf::from("CODEX_HOME"));
    assert_eq!(
        issue.error,
        DiscoveryError::Unsupported("CODEX_HOME path is not a directory")
    );
    let rendered = format!("{report:?}");
    assert!(!rendered.contains("synthetic non-directory"));
    assert!(!rendered.contains("fallback-sentinel"));
}

#[test]
fn malformed_explicit_codex_home_does_not_fall_back_to_default() {
    let home = tempfile::tempdir().unwrap();
    write_codex_fixture(&home.path().join(".codex"), "fallback-sentinel");
    let explicit = home.path().join("codex-invalid");
    std::fs::create_dir_all(&explicit).unwrap();
    std::fs::write(
        explicit.join("auth.json"),
        "synthetic malformed credential document",
    )
    .unwrap();

    let report = discover_default_accounts_with_codex_home(home.path(), Some(explicit.as_os_str()));
    assert!(codex_accounts(&report).is_empty());
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.agent == Agent::Codex)
        .expect("malformed explicit source is reported");
    assert_eq!(issue.directory, PathBuf::from("CODEX_HOME"));
    assert_eq!(issue.error, DiscoveryError::Malformed);
    assert!(!format!("{report:?}").contains("synthetic malformed"));
    assert!(!format!("{report:?}").contains("fallback-sentinel"));
}

#[test]
fn unreadable_explicit_codex_file_does_not_fall_back_to_default() {
    let home = tempfile::tempdir().unwrap();
    write_codex_fixture(&home.path().join(".codex"), "fallback-sentinel");
    let explicit = home.path().join("codex-unreadable");
    std::fs::create_dir_all(explicit.join("auth.json")).unwrap();

    let report = discover_default_accounts_with_codex_home(home.path(), Some(explicit.as_os_str()));
    assert!(codex_accounts(&report).is_empty());
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.agent == Agent::Codex)
        .expect("unreadable explicit source is reported");
    assert_eq!(issue.directory, PathBuf::from("CODEX_HOME"));
    assert_eq!(issue.error, DiscoveryError::Unreadable);
    assert!(!format!("{report:?}").contains("fallback-sentinel"));
}

#[test]
fn codex_discovery_does_not_reject_refreshable_expired_token_fixture() {
    let home = tempfile::tempdir().unwrap();
    let explicit = home.path().join("codex-expired-access-token");
    std::fs::create_dir_all(&explicit).unwrap();
    std::fs::write(
        explicit.join("auth.json"),
        r#"{"tokens":{"access_token":"synthetic-expired-access","refresh_token":"synthetic-refresh","expires_at":0}}"#,
    )
    .unwrap();

    let report = discover_default_accounts_with_codex_home(home.path(), Some(explicit.as_os_str()));
    let accounts = codex_accounts(&report);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].directory, explicit);
    assert!(!format!("{report:?}").contains("synthetic-expired-access"));
    assert!(!format!("{report:?}").contains("synthetic-refresh"));
}

#[test]
fn empty_explicit_codex_home_uses_default_like_codex_cli() {
    let home = tempfile::tempdir().unwrap();
    let default = home.path().join(".codex");
    write_codex_fixture(&default, "default-sentinel");

    let report = discover_default_accounts_with_codex_home(home.path(), Some(OsStr::new("")));
    let accounts = codex_accounts(&report);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].directory, default);
    assert!(!format!("{report:?}").contains("default-sentinel"));
}

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
