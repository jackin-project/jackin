// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn disabling_account_via_upsert_prunes_bindings_across_all_scopes() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("work", &profile_account()).unwrap();
    editor
        .create_workspace(&wn("project"), account_workspace(temp.path()))
        .unwrap();
    editor
        .set_workspace_accounts(&wn("project"), &["work".into()])
        .unwrap();
    editor
        .set_account_binding(None, None, Agent::Claude, Some("work"))
        .unwrap();
    editor
        .set_account_binding(Some(&wn("project")), None, Agent::Claude, Some("work"))
        .unwrap();
    editor
        .set_account_binding(
            Some(&wn("project")),
            Some("smith"),
            Agent::Claude,
            Some("work"),
        )
        .unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let mut disabled = profile_account();
    disabled.enabled = false;
    editor.upsert_account("work", &disabled).unwrap();
    let config = editor.save().unwrap();

    assert!(!config.accounts["work"].enabled);
    assert!(config.account_bindings.is_empty());
    assert!(config.workspaces["project"].account_bindings.is_empty());
    assert!(
        config.workspaces["project"].roles["smith"]
            .account_bindings
            .is_empty()
    );
}

#[test]
fn open_detailed_fresh_install_scans_and_stamps_sentinel() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let (editor, report) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(report.fresh_install);
    let config = editor.save().unwrap();
    assert_eq!(config.bootstrap, Some(crate::BootstrapState::initialized()));
    // Every reported ID exists in the registry (no phantom additions).
    for id in &report.added_accounts {
        assert!(config.accounts.contains_key(id), "missing {id}");
    }
    // Reopening is not a fresh install and rescans nothing.
    let (_, second) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(!second.fresh_install);
    assert!(second.added_accounts.is_empty());
}

#[test]
fn open_detailed_consumes_installer_marker_exactly_once() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        format!(
            "version = \"{}\"\n\n[bootstrap]\nversion = 1\nfresh_install = true\n",
            crate::CURRENT_CONFIG_VERSION
        ),
    )
    .unwrap();
    let (editor, report) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(report.fresh_install);
    let config = editor.save().unwrap();
    assert_eq!(config.bootstrap, Some(crate::BootstrapState::initialized()));
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!raw.contains("fresh_install = true"), "{raw}");
}

#[test]
fn failed_fresh_install_bootstrap_keeps_marker_for_retry() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        format!(
            "version = \"{}\"\n\n[bootstrap]\nversion = 1\nfresh_install = true\n\n[accounts.bad]\nname = \"\"\nprovider = \"anthropic\"\n\n[accounts.bad.credential]\ntype = \"api_key\"\nvalue = \"$BAD\"\n",
            crate::CURRENT_CONFIG_VERSION
        ),
    )
    .unwrap();

    let bootstrap_failure = ConfigEditor::open_detailed(&paths).err();
    assert!(bootstrap_failure.is_some());
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(raw.contains("fresh_install = true"), "{raw}");

    std::fs::write(
        &paths.config_file,
        format!(
            "version = \"{}\"\n\n[bootstrap]\nversion = 1\nfresh_install = true\n",
            crate::CURRENT_CONFIG_VERSION
        ),
    )
    .unwrap();
    let (editor, report) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(report.fresh_install);
    editor.save().unwrap();
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!raw.contains("fresh_install = true"), "{raw}");
}

#[test]
fn open_detailed_upgrade_never_resurrects_or_rescans() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    // Pre-sentinel config with one deliberate account and no marker.
    std::fs::write(
        &paths.config_file,
        "version = \"v1alpha10\"\n\n[accounts.kept]\nenabled = true\nname = \"Kept\"\nprovider = \"anthropic\"\n\n[accounts.kept.credential]\ntype = \"api_key\"\nvalue = \"${ANTHROPIC_API_KEY}\"\n",
    )
    .unwrap();
    let (editor, report) = ConfigEditor::open_detailed(&paths).unwrap();
    assert!(!report.fresh_install);
    assert!(report.added_accounts.is_empty());
    let config = editor.save().unwrap();
    // Exactly the deliberate account survives: nothing resurrected, nothing added.
    assert_eq!(
        config.accounts.keys().collect::<Vec<_>>(),
        vec![&"kept".to_owned()]
    );
    assert_eq!(config.bootstrap, Some(crate::BootstrapState::initialized()));
}

#[test]
fn scan_for_accounts_imports_profiles_with_bootstrap_naming_and_dedupes() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(report.added_accounts.contains(&"default-claude".to_owned()));
    assert_eq!(report.added_accounts.len(), report.added.len());
    let (_, account) = report
        .added
        .iter()
        .find(|(id, _)| id == "default-claude")
        .unwrap();
    assert_eq!(account.name, "Claude default");
    assert_eq!(account.provider, crate::AiProvider::Anthropic);
    let config = editor.save().unwrap();
    assert!(config.accounts.contains_key("default-claude"));

    // Re-scan dedupes to a no-op: same IDs, same sources, nothing added.
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let second = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(second.added_accounts.is_empty(), "{second:?}");
}

#[test]
fn scan_for_accounts_uses_injected_codex_home_instead_of_ambient_override() {
    const CHILD_ROOT_ENV: &str = "JACKIN_CODEX_HOME_ROUTE_TEST_ROOT";
    let Some(root) = std::env::var_os(CHILD_ROOT_ENV) else {
        let fixture = tempdir().unwrap();
        #[expect(
            clippy::disallowed_methods,
            reason = "test re-execs itself in a child process for env isolation"
        )]
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(
                "editor::tests::case_07::scan_for_accounts_uses_injected_codex_home_instead_of_ambient_override",
            )
            .env(CHILD_ROOT_ENV, fixture.path())
            .env("CODEX_HOME", fixture.path().join("host-codex"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated discovery child failed; stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("running 1 test"),
            "isolated discovery child did not run the expected fixture: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        return;
    };

    let root = PathBuf::from(root);
    let host_codex = root.join("host-codex");
    let injected_codex = root.join("injected-codex");
    for (directory, token) in [
        (&host_codex, "host-codex-sentinel"),
        (&injected_codex, "injected-codex-sentinel"),
    ] {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(
            directory.join("auth.json"),
            format!(r#"{{"tokens":{{"access_token":"{token}"}}}}"#),
        )
        .unwrap();
    }

    let paths = JackinPaths::for_tests(&root.join("jackin"));
    minimal_config_file(&paths);
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let environment = BTreeMap::from([(
        "CODEX_HOME".to_owned(),
        injected_codex.to_string_lossy().into_owned(),
    )]);
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &environment)
        .unwrap();
    let (_, account) = report
        .added
        .iter()
        .find(|(id, _)| id == "default-codex")
        .expect("injected Codex profile is registered");
    match &account.credential {
        crate::AccountCredential::Profile {
            agent: Agent::Codex,
            directory,
            ..
        } => assert_eq!(directory, &injected_codex),
        credential => panic!("unexpected Codex credential route: {credential:?}"),
    }
    let rendered = format!("{report:?}");
    assert!(!rendered.contains("host-codex-sentinel"));
    assert!(!rendered.contains("injected-codex-sentinel"));
}

#[test]
fn removed_account_stays_excluded_from_scan_after_reload() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    claude_credentials_fixture(&paths.home_dir);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let first = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(first.added_accounts.contains(&"default-claude".to_owned()));
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("default-claude").unwrap();
    let removed = editor.save().unwrap();
    assert!(!removed.accounts.contains_key("default-claude"));
    assert_eq!(removed.account_scan_exclusions.len(), 1);

    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor
        .scan_for_accounts_with(&paths.home_dir, &BTreeMap::new())
        .unwrap();
    assert!(report.added_accounts.is_empty(), "{report:?}");
    let reloaded = editor.save().unwrap();
    assert!(!reloaded.accounts.contains_key("default-claude"));
    assert!(
        !AppConfig::load_or_init(&paths)
            .unwrap()
            .accounts
            .contains_key("default-claude")
    );
}

#[cfg(unix)]
#[test]
fn removed_amp_xdg_account_stays_excluded_after_symlinked_shell_scan() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    minimal_config_file(&paths);
    let root = temp.path().join("xdg");
    let alias = temp.path().join("xdg-alias");
    let data = root.join("data");
    let config = root.join("config");
    let cache = root.join("cache");
    std::fs::create_dir_all(data.join("amp")).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        data.join("amp/secrets.json"),
        r#"{"apiKey@https://ampcode.com/":"fixture-key"}"#,
    )
    .unwrap();
    symlink(&root, &alias).unwrap();

    let account = crate::AccountConfig {
        enabled: true,
        name: "Amp removed".into(),
        provider: crate::AiProvider::Amp,
        credential: crate::AccountCredential::Profile {
            agent: Agent::Amp,
            directory: data.join("amp"),
            xdg_roots: Some(crate::XdgRoots {
                data: data.clone(),
                config: config.clone(),
                cache: cache.clone(),
            }),
            source_selector: None,
        },
    };
    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.upsert_account("custom-amp", &account).unwrap();
    editor.save().unwrap();

    let mut editor = ConfigEditor::open(&paths).unwrap();
    editor.remove_account("custom-amp").unwrap();
    editor.save().unwrap();

    let plan = crate::import_plan(&crate::parse_zshrc_source(&format!(
        "XDG_DATA_HOME={}/./data\nXDG_CONFIG_HOME={}/config/..//config\nXDG_CACHE_HOME={}/cache\n",
        alias.display(),
        alias.display(),
        alias.display()
    )));
    let mut editor = ConfigEditor::open(&paths).unwrap();
    let report = editor.apply_zshrc_plan(&plan).unwrap();
    assert!(report.added_accounts.is_empty(), "{report:?}");
    assert!(report.unapplied_zshrc_xdg_roots.is_empty(), "{report:?}");
    assert!(!editor.save().unwrap().accounts.contains_key("custom-amp"));
}
