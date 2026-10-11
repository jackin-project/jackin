// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(unix)]
#[test]
fn removed_opencode_account_stays_excluded_after_symlinked_home_scan() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let real_home = paths.home_dir.clone();
    let alias_home = temp.path().join("home-alias");
    let directory = real_home.join(".local/share/opencode");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"fixture-key"}}"#,
    )
    .unwrap();
    symlink(&real_home, &alias_home).unwrap();

    let account = crate::AccountConfig {
        enabled: true,
        name: "OpenCode removed".into(),
        provider: crate::AiProvider::Opencode,
        credential: crate::AccountCredential::Profile {
            agent: Agent::Opencode,
            directory: real_home.join(".local/share/./opencode"),
            xdg_roots: None,
            source_selector: None,
        },
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("removed-opencode", &account).unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("removed-opencode").unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(&alias_home.join("."), &BTreeMap::new())
        .unwrap();
    assert!(
        !report
            .added_accounts
            .contains(&"default-opencode-opencode".to_owned()),
        "{report:?}"
    );
    assert!(
        !editor
            .save()
            .unwrap()
            .accounts
            .contains_key("default-opencode-opencode")
    );
}

#[test]
fn removed_amp_xdg_profile_stays_excluded_from_shell_scan() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let data = temp.path().join("xdg-data");
    let config = temp.path().join("xdg-config");
    let cache = temp.path().join("xdg-cache");
    std::fs::create_dir_all(data.join("amp")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("amp/secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"fixture-key"}"#,
    )
    .unwrap();
    let plan = crate::import_plan(&crate::parse_zshrc_source(&format!(
        "XDG_DATA_HOME={}\nXDG_CONFIG_HOME={}\nXDG_CACHE_HOME={}\n",
        data.display(),
        config.display(),
        cache.display()
    )));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    assert!(
        editor
            .apply_zshrc_plan(&plan)
            .unwrap()
            .added_accounts
            .contains(&"custom-amp".to_owned())
    );
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("custom-amp").unwrap();
    let removed = editor.save().unwrap();
    assert_eq!(removed.account_scan_exclusions.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(report.added_accounts.is_empty(), "{report:?}");
    let reloaded = editor.save().unwrap();
    assert!(!reloaded.accounts.contains_key("custom-amp"));
}

#[test]
fn removed_api_key_endpoint_account_stays_excluded_from_environment_scan() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let account = crate::AccountConfig {
        enabled: true,
        name: "OpenAI endpoint".into(),
        provider: crate::AiProvider::OpenAi,
        credential: crate::AccountCredential::ApiKey {
            value: EnvValue::from("$OPENAI_API_KEY"),
            base_url: Some("https://proxy.example/v1".into()),
            model: Some("gpt-endpoint".into()),
        },
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("openai-api-key", &account).unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("openai-api-key").unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(
            &paths.home_dir,
            &BTreeMap::from([
                ("OPENAI_API_KEY".to_owned(), "fixture".to_owned()),
                (
                    "OPENAI_BASE_URL".to_owned(),
                    "https://proxy.example/v1".to_owned(),
                ),
            ]),
        )
        .unwrap();
    assert!(
        !report.added_accounts.contains(&"openai-api-key".to_owned()),
        "{report:?}"
    );
    let reloaded = editor.save().unwrap();
    assert!(!reloaded.accounts.contains_key("openai-api-key"));
}

#[test]
fn environment_accounts_with_distinct_endpoints_keep_distinct_sources() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let existing = crate::AccountConfig {
        enabled: true,
        name: "OpenAI proxy A".into(),
        provider: crate::AiProvider::OpenAi,
        credential: crate::AccountCredential::ApiKey {
            value: EnvValue::from("$OPENAI_API_KEY"),
            base_url: Some("https://proxy-a.example/v1".into()),
            model: Some("model-a".into()),
        },
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("openai-proxy-a", &existing).unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(
            &paths.home_dir,
            &BTreeMap::from([
                ("OPENAI_API_KEY".to_owned(), "fixture-key".to_owned()),
                (
                    "OPENAI_BASE_URL".to_owned(),
                    "https://proxy-b.example/v1".to_owned(),
                ),
            ]),
        )
        .unwrap();
    assert!(report.added_accounts.contains(&"openai-api-key".to_owned()));
}

#[test]
fn removed_environment_account_with_endpoint_stays_excluded() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let environment = BTreeMap::from([
        ("OPENAI_API_KEY".to_owned(), "fixture-key".to_owned()),
        (
            "OPENAI_BASE_URL".to_owned(),
            "https://proxy.example/v1".to_owned(),
        ),
    ]);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor
        .scan_for_accounts_with(&paths.home_dir, &environment)
        .unwrap();
    assert!(first.added_accounts.contains(&"openai-api-key".to_owned()));
    let (_, account) = first
        .added
        .iter()
        .find(|(id, _)| id == "openai-api-key")
        .unwrap();
    assert!(matches!(
        &account.credential,
        crate::AccountCredential::ApiKey {
            base_url: Some(url),
            ..
        } if url == "https://proxy.example/v1"
    ));
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("openai-api-key").unwrap();
    let removed = editor.save().unwrap();
    assert_eq!(removed.account_scan_exclusions.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor
        .scan_for_accounts_with(&paths.home_dir, &environment)
        .unwrap();
    assert!(second.added_accounts.is_empty(), "{second:?}");
    assert!(!second.changed);
    assert!(
        !editor
            .save()
            .unwrap()
            .accounts
            .contains_key("openai-api-key")
    );
}

#[test]
fn scan_for_accounts_reads_live_home_and_environment() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);
    let mut editor = ConfigEditor::open(&paths).unwrap();
    // Ambient process environment may add further accounts; the fixture
    // profile must always be among them.
    let report = editor.scan_for_accounts().unwrap();
    assert!(report.added_accounts.contains(&"default-claude".to_owned()));
}

#[test]
fn scan_for_accounts_never_overwrites_operator_id_registrations() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let mut operator = profile_account();
    operator.name = "Operator".into();
    operator.credential = crate::AccountCredential::Profile {
        agent: Agent::Claude,
        directory: temp.path().join("elsewhere"),
        xdg_roots: None,
        source_selector: None,
    };
    editor.upsert_account("default-claude", &operator).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(!report.added_accounts.contains(&"default-claude".to_owned()));
    let config = editor.save().unwrap();
    assert_eq!(config.accounts["default-claude"].name, "Operator");
}

#[test]
fn scan_for_accounts_skips_sources_registered_under_other_ids() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    // Same credential source as the discovered default, registered under
    // an operator-chosen ID: the scan must skip, not error.
    let mut renamed = profile_account();
    renamed.credential = crate::AccountCredential::Profile {
        agent: Agent::Claude,
        directory: paths.home_dir.join(".claude"),
        xdg_roots: None,
        source_selector: None,
    };
    editor.upsert_account("mine", &renamed).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(!report.added_accounts.contains(&"default-claude".to_owned()));
}

#[test]
fn scan_for_accounts_imports_environment_references_without_values() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);

    let oauth_var = jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME;
    let environment = BTreeMap::from([
        ("ANTHROPIC_API_KEY".to_owned(), "live-secret".to_owned()),
        (oauth_var.to_owned(), "live-token".to_owned()),
    ]);
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &environment)
        .unwrap();
    for (id, expected) in [
        ("anthropic-api-key", "$ANTHROPIC_API_KEY".to_owned()),
        ("claude-oauth-token", format!("${oauth_var}")),
    ] {
        let (_, account) = report.added.iter().find(|(found, _)| found == id).unwrap();
        let persisted = match &account.credential {
            crate::AccountCredential::ApiKey { value, .. }
            | crate::AccountCredential::OAuthToken { value, .. } => value.as_persisted_str(),
            other @ crate::AccountCredential::Profile { .. } => {
                panic!("unexpected credential for {id}: {other:?}")
            }
        };
        assert_eq!(persisted, expected);
    }
    // Values never enter the report, even under Debug.
    let dumped = format!("{report:?}");
    assert!(!dumped.contains("live-secret"), "{dumped}");
    assert!(!dumped.contains("live-token"), "{dumped}");
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("anthropic-api-key"));
}

#[test]
fn profile_scan_candidate_skips_agents_without_native_billing() {
    for agent in [Agent::Omp, Agent::Hermes] {
        let discovered = crate::DiscoveredAccount {
            agent,
            provider: crate::AiProvider::for_agent(agent),
            directory: "/tmp/store".into(),
            source_selector: None,
            evidence: crate::CredentialEvidence::File("/tmp/store/auth.json".into()),
        };
        assert!(profile_scan_candidate(&discovered).is_none());
    }
    let discovered = crate::DiscoveredAccount {
        agent: Agent::Claude,
        provider: Some(crate::AiProvider::Anthropic),
        directory: "/tmp/claude".into(),
        source_selector: None,
        evidence: crate::CredentialEvidence::File("/tmp/claude/.credentials.json".into()),
    };
    let (id, account) = profile_scan_candidate(&discovered).unwrap();
    assert_eq!(id, "default-claude");
    assert_eq!(account.name, "Claude default");
}

#[test]
fn profile_scan_candidate_preserves_opencode_store_provider_identity() {
    let discovered = crate::DiscoveredAccount {
        agent: Agent::Opencode,
        provider: Some(crate::AiProvider::Zai),
        directory: "/tmp/opencode".into(),
        source_selector: None,
        evidence: crate::CredentialEvidence::File("/tmp/opencode/auth.json".into()),
    };
    let (id, account) = profile_scan_candidate(&discovered).unwrap();
    assert_eq!(id, "default-opencode-zai");
    assert_eq!(account.provider, crate::AiProvider::Zai);
    assert_eq!(account.name, "OpenCode zai default");
}

#[test]
fn zshrc_provider_accepts_only_canonical_catalog_slugs() {
    for provider in crate::AiProvider::ALL {
        assert_eq!(zshrc_provider(provider.slug()), Some(*provider));
    }
    assert_eq!(zshrc_provider("kimi"), None);
    assert_eq!(zshrc_provider("gemini"), None);
}
