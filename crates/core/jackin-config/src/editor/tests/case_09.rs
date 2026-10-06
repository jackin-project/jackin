// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn apply_zshrc_plan_seeds_verified_directories_and_op_refs() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let override_dir = temp.path().join("claude-override");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(
        override_dir.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fixture"}}"#,
    )
    .unwrap();
    let source = format!(
        "CLAUDE_CONFIG_DIR={}\nANTHROPIC_API_KEY=$(op read op://vault/item/field)\n",
        override_dir.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));
    assert_eq!(plan.directories.len(), 1);
    assert_eq!(plan.op_refs.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(report.added_accounts.contains(&"custom-claude".to_owned()));
    assert!(
        report
            .added_accounts
            .contains(&"anthropic-api-key".to_owned())
    );
    let (_, key) = report
        .added
        .iter()
        .find(|(id, _)| id == "anthropic-api-key")
        .unwrap();
    assert!(matches!(
        key.credential,
        crate::AccountCredential::ApiKey {
            value: EnvValue::OpRef(_),
            ..
        }
    ));
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("custom-claude"));
    assert!(config.accounts.contains_key("anthropic-api-key"));
}

#[test]
fn apply_zshrc_custom_profile_does_not_collide_with_default_profile() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);
    let override_dir = temp.path().join("claude-override");
    std::fs::create_dir_all(&override_dir).unwrap();
    std::fs::write(
        override_dir.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"custom-fixture"}}"#,
    )
    .unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let defaults = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(
        defaults
            .added_accounts
            .contains(&"default-claude".to_owned())
    );
    let source = format!("CLAUDE_CONFIG_DIR={}\n", override_dir.display());
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));
    let custom = editor.apply_zshrc_plan(&plan).unwrap();

    assert!(custom.added_accounts.contains(&"custom-claude".to_owned()));
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("default-claude"));
    assert!(config.accounts.contains_key("custom-claude"));
    assert_ne!(
        config.accounts["default-claude"].source_directory(),
        config.accounts["custom-claude"].source_directory()
    );
}

#[test]
fn apply_zshrc_plan_skips_unverified_directories_and_unknown_vars() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let empty_dir = temp.path().join("empty-override");
    std::fs::create_dir_all(&empty_dir).unwrap();
    let source = format!(
        "CLAUDE_CONFIG_DIR={}\nWIDGET_API_KEY=$(op read op://vault/item/field)\n",
        empty_dir.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));
    assert_eq!(plan.directories.len(), 1);
    assert_eq!(plan.op_refs.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(report.added_accounts.is_empty(), "{report:?}");
    assert!(report.issues.is_empty(), "{report:?}");
}

#[test]
fn apply_zshrc_plan_persists_canonical_model_and_reports_unsupported_wrapper() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor
        .upsert_account(
            "moonshot-api-key",
            &crate::AccountConfig {
                enabled: true,
                name: "Kimi API".into(),
                provider: crate::AiProvider::Moonshot,
                credential: crate::AccountCredential::ApiKey {
                    value: EnvValue::Plain("$KIMI_API_KEY".into()),
                    base_url: None,
                    model: None,
                },
            },
        )
        .unwrap();
    let plan = crate::import_plan(&crate::parse_zshrc_source(
        "kimi_key() { echo fixture; }\nMOONSHOT_MODEL=kimi-k2\nMOONSHOT_BASE_URL=https://api.kimi.example/v1\nKIMI_API_KEY=$(kimi_key)\n",
    ));

    let report = editor.apply_zshrc_plan(&plan).unwrap();

    assert!(report.unapplied_zshrc_models.is_empty(), "{report:?}");
    assert!(report.changed);
    assert_eq!(report.unapplied_zshrc_wrappers.len(), 1);
    assert_eq!(report.unapplied_zshrc_wrappers[0].var, "KIMI_API_KEY");
    let config = editor.save().unwrap();
    let account = &config.accounts["moonshot-api-key"];
    assert_eq!(
        account.credential,
        crate::AccountCredential::ApiKey {
            value: EnvValue::Plain("$KIMI_API_KEY".into()),
            base_url: Some("https://api.kimi.example/v1".into()),
            model: Some("kimi-k2".into()),
        }
    );
}

#[test]
fn removed_op_ref_account_keeps_model_endpoint_tombstone_identity() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let plan = crate::import_plan(&crate::parse_zshrc_source(
        "MOONSHOT_MODEL=kimi-k2\nMOONSHOT_BASE_URL=https://proxy.example/v1\nKIMI_API_KEY=$(op read op://vault/item/field)\n",
    ));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(
        first
            .added_accounts
            .contains(&"moonshot-api-key".to_owned())
    );
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("moonshot-api-key").unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(second.added_accounts.is_empty(), "{second:?}");
    assert!(
        second
            .unapplied_zshrc_models
            .iter()
            .any(|model| model.name == "moonshot")
    );
    assert!(
        !editor
            .save()
            .unwrap()
            .accounts
            .contains_key("moonshot-api-key")
    );
}

#[test]
fn apply_zshrc_plan_leaves_provider_alias_models_unapplied() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let mut editor = ConfigEditor::open(&paths).unwrap();
    for (id, name, provider, variable) in [
        (
            "moonshot-api-key",
            "Kimi API",
            crate::AiProvider::Moonshot,
            "$KIMI_API_KEY",
        ),
        (
            "google-api-key",
            "Gemini API",
            crate::AiProvider::Google,
            "$GEMINI_API_KEY",
        ),
    ] {
        editor
            .upsert_account(
                id,
                &crate::AccountConfig {
                    enabled: true,
                    name: name.into(),
                    provider,
                    credential: crate::AccountCredential::ApiKey {
                        value: EnvValue::Plain(variable.into()),
                        base_url: None,
                        model: None,
                    },
                },
            )
            .unwrap();
    }

    let plan = crate::import_plan(&crate::parse_zshrc_source(
        "KIMI_MODEL=kimi-k2\nKIMI_BASE_URL=https://api.kimi.example/v1\nGEMINI_MODEL=gemini-2.5-pro\nGEMINI_BASE_URL=https://generativelanguage.example/v1\n",
    ));
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    let names: Vec<_> = report
        .unapplied_zshrc_models
        .iter()
        .map(|model| model.name.as_str())
        .collect();
    assert_eq!(names, ["gemini", "kimi"]);

    let config = editor.save().unwrap();
    for id in ["moonshot-api-key", "google-api-key"] {
        let crate::AccountCredential::ApiKey {
            model, base_url, ..
        } = &config.accounts[id].credential
        else {
            panic!("expected API-key account for {id}");
        };
        assert!(model.is_none(), "alias model applied to {id}");
        assert!(base_url.is_none(), "alias endpoint applied to {id}");
    }
}

#[test]
fn apply_zshrc_plan_persists_amp_xdg_roots_with_discovered_credentials() {
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
    let source = format!(
        "XDG_DATA_HOME={}\nXDG_CONFIG_HOME={}\nXDG_CACHE_HOME={}\n",
        data.display(),
        config.display(),
        cache.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();

    assert!(report.unapplied_zshrc_xdg_roots.is_empty(), "{report:?}");
    assert!(report.added_accounts.contains(&"custom-amp".to_owned()));
    let account = &report
        .added
        .iter()
        .find(|(id, _)| id == "custom-amp")
        .unwrap()
        .1;
    assert!(matches!(
        &account.credential,
        crate::AccountCredential::Profile {
            agent: Agent::Amp,
            directory,
            xdg_roots: Some(_),
            source_selector: None,
        } if directory == &data.join("amp")
    ));
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("custom-amp"));
}

#[test]
fn removed_amp_xdg_account_stays_excluded_from_zshrc_scan() {
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
    let source = format!(
        "XDG_DATA_HOME={}\nXDG_CONFIG_HOME={}\nXDG_CACHE_HOME={}\n",
        data.display(),
        config.display(),
        cache.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(first.added_accounts.contains(&"custom-amp".to_owned()));
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("custom-amp").unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(second.added_accounts.is_empty(), "{second:?}");
    assert!(second.unapplied_zshrc_xdg_roots.is_empty(), "{second:?}");
    assert!(!editor.save().unwrap().accounts.contains_key("custom-amp"));
}

#[test]
fn apply_zshrc_plan_rejects_opencode_xdg_root_before_amp_persistence() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let data = temp.path().join("xdg-data");
    let config = temp.path().join("xdg-config");
    let cache = temp.path().join("xdg-cache");
    std::fs::create_dir_all(data.join("amp")).unwrap();
    std::fs::create_dir_all(data.join("opencode")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("amp/secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"fixture-amp"}"#,
    )
    .unwrap();
    std::fs::write(
        data.join("opencode/auth.json"),
        r#"{"opencode-go":{"type":"api","key":"fixture-opencode"}}"#,
    )
    .unwrap();
    let source = format!(
        "XDG_DATA_HOME={}\nXDG_CONFIG_HOME={}\nXDG_CACHE_HOME={}\n",
        data.display(),
        config.display(),
        cache.display()
    );
    let plan = crate::import_plan(&crate::parse_zshrc_source(&source));

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();

    assert!(report.added_accounts.is_empty(), "{report:?}");
    assert_eq!(
        report.unapplied_zshrc_xdg_roots,
        vec![crate::XdgRoots {
            data: data.clone(),
            config: config.clone(),
            cache: cache.clone(),
        }]
    );
    assert!(report.issues.iter().any(|issue| {
        issue.agent == Agent::Opencode
            && issue.error
                == crate::DiscoveryError::Unsupported(
                    "OpenCode XDG roots from shell imports require an explicit profile directory",
                )
    }));
    let config = editor.save().unwrap();
    assert!(!config.accounts.contains_key("custom-amp"));
    assert!(
        !config
            .accounts
            .values()
            .any(|account| account.provider == crate::AiProvider::Opencode)
    );
}
