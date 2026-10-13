// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    AccountScanOutcome, OwnedSettingsSaveInput, WorkspaceSaveInput, WorkspaceSaveMode,
    run_account_scan, save_settings_first_run_aware, save_workspace, start_account_scan,
};
use jackin_config::{
    AccountConfig, AccountCredential, AiProvider, AppConfig, CURRENT_WORKSPACE_VERSION, EnvValue,
    GithubAuthConfig, MountConfig, MountIsolation, WorkspaceConfig, WorkspaceRoleOverride,
};
use jackin_console::tui::runtime::{BlockingSubscription, SubscriptionPoll};
use jackin_core::{Agent, JackinPaths};
use std::collections::BTreeMap;

fn workspace_file_contents(paths: &JackinPaths, name: &str) -> String {
    std::fs::read_to_string(paths.workspaces_dir.join(format!("{name}.toml"))).unwrap()
}

#[test]
fn save_workspace_persists_and_clears_account_assignments_and_bindings() {
    let tmp = tempfile::tempdir().unwrap();
    let mount_src = tmp.path().join("repo");
    std::fs::create_dir(&mount_src).unwrap();
    let original = WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/workspace/proj".to_owned(),
        mounts: vec![MountConfig {
            src: mount_src.display().to_string(),
            dst: "/workspace/proj".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        ..WorkspaceConfig::default()
    };
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    config
        .workspaces
        .insert("proj".to_owned(), original.clone());
    config.accounts.insert(
        "work".into(),
        AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::ApiKey {
                value: EnvValue::Plain("test-key".into()),
                base_url: None,
                model: None,
            },
        },
    );
    std::fs::write(&paths.config_file, toml::to_string(&config).unwrap()).unwrap();

    let mut pending = original.clone();
    pending.accounts.push("work".into());
    pending
        .account_bindings
        .insert(Agent::Claude, "work".into());
    pending.roles.insert(
        "smith".into(),
        WorkspaceRoleOverride {
            account_bindings: [(Agent::Claude, "work".into())].into(),
            ..Default::default()
        },
    );

    let saved = save_workspace(
        &paths,
        WorkspaceSaveInput {
            mode: WorkspaceSaveMode::Edit {
                original_name: "proj".to_owned(),
                pending_name: None,
                effective_removals: Vec::new(),
            },
            original: &original,
            pending: &pending,
        },
    )
    .unwrap();

    let reloaded = saved.config.workspaces.get("proj").unwrap();
    assert_eq!(reloaded.accounts, ["work"]);
    assert_eq!(
        reloaded
            .account_bindings
            .get(&Agent::Claude)
            .map(String::as_str),
        Some("work")
    );
    assert_eq!(
        reloaded.roles["smith"]
            .account_bindings
            .get(&Agent::Claude)
            .map(String::as_str),
        Some("work")
    );
    let mut cleared = reloaded.clone();
    cleared.accounts.clear();
    cleared.account_bindings.clear();
    cleared.roles.clear();
    save_workspace(
        &paths,
        WorkspaceSaveInput {
            mode: WorkspaceSaveMode::Edit {
                original_name: "proj".to_owned(),
                pending_name: None,
                effective_removals: Vec::new(),
            },
            original: reloaded,
            pending: &cleared,
        },
    )
    .unwrap();

    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    let workspace = reloaded.workspaces.get("proj").unwrap();
    assert!(workspace.accounts.is_empty());
    assert!(workspace.account_bindings.is_empty());
    assert!(workspace.roles.is_empty());

    let out = workspace_file_contents(&paths, "proj");
    assert!(!out.contains("work\""), "{out}");
}

fn cursor_credentials_fixture(home: &std::path::Path) {
    std::fs::create_dir_all(home.join(".cursor")).unwrap();
    std::fs::write(
        home.join(".cursor/auth.json"),
        r#"{"accessToken":"fixture"}"#,
    )
    .unwrap();
}

fn poll_scan_to_ready(
    rx: &mut BlockingSubscription<(u64, Result<AccountScanOutcome, String>)>,
) -> (u64, Result<AccountScanOutcome, String>) {
    // Spin (no thread sleep: banned repo-wide): the worker is local
    // filesystem I/O and lands in milliseconds.
    for _ in 0..10_000_000 {
        match rx.poll_next() {
            SubscriptionPoll::Ready(result) => return result,
            SubscriptionPoll::Closed => panic!("scan worker dropped"),
            SubscriptionPoll::Pending => std::hint::spin_loop(),
        }
    }
    panic!("scan worker timed out");
}

#[test]
fn account_scan_worker_returns_candidates_without_saving() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        format!("version = \"{}\"\n", jackin_config::CURRENT_CONFIG_VERSION),
    )
    .unwrap();
    cursor_credentials_fixture(&paths.home_dir);

    let mut rx = start_account_scan(paths.clone(), 7);
    let (generation, result) = poll_scan_to_ready(&mut rx);
    assert_eq!(generation, 7);
    let outcome = result.unwrap();
    assert!(!outcome.fresh_install);
    assert!(outcome.committed.is_empty());
    assert!(
        outcome
            .candidates
            .iter()
            .any(|(id, _)| id == "default-cursor"),
        "{outcome:?}"
    );
    // Nothing saved: the draft merge owns persistence.
    let raw = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(!raw.contains("default-cursor"), "{raw}");
}

#[test]
fn account_scan_worker_reports_bootstrap_as_committed() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    // No config file: the worker's open bootstraps (fresh install).
    cursor_credentials_fixture(&paths.home_dir);

    let outcome = run_account_scan(&paths).unwrap();
    assert!(outcome.fresh_install);
    assert!(
        outcome
            .committed
            .iter()
            .any(|(id, _)| id == "default-cursor"),
        "{outcome:?}"
    );
    assert!(
        outcome
            .candidates
            .iter()
            .all(|(id, _)| id != "default-cursor"),
        "{outcome:?}"
    );
}

#[test]
fn settings_save_preserves_first_run_bootstrap_accounts() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    // Fresh install with discoverable evidence: the save bootstraps
    // first, then applies the (empty) UI diff without dropping
    // bootstrapped accounts.
    cursor_credentials_fixture(&paths.home_dir);
    let input = OwnedSettingsSaveInput {
        mounts_original: Vec::new(),
        mounts_pending: Vec::new(),
        env_original: jackin_console::tui::state::SettingsEnvConfig {
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
        },
        env_pending: jackin_console::tui::state::SettingsEnvConfig {
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
        },
        auth_pending: BTreeMap::new(),
        auth_original: BTreeMap::new(),
        bindings_pending: BTreeMap::new(),
        bindings_original: BTreeMap::new(),
        github: GithubAuthConfig::default(),
        original_github: GithubAuthConfig::default(),
        trust_pending: Vec::new(),
        git_coauthor_trailer: false,
        git_dco: false,
    };
    let saved = save_settings_first_run_aware(&paths, &input).unwrap();
    assert!(saved.accounts.contains_key("default-cursor"));
    assert_eq!(
        saved.bootstrap,
        Some(jackin_config::BootstrapState::initialized())
    );
}
