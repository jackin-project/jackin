// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Non-TUI config persistence services.

use jackin_config::GlobalMountRow;
use jackin_config::WorkspaceConfig;
use jackin_config::{AppConfig, BootstrapReport, RoleSource};
use jackin_console::services::config_save::{
    WorkspaceSaveDiffOp, build_workspace_edit, workspace_save_diff_plan,
};
use jackin_console::tui::screens::settings::model::AccountScanOutcome;
use jackin_core::JackinPaths;
use jackin_core::WorkspaceName;

pub(crate) use jackin_console::services::config_save::{SettingsSaveInput, save_settings};

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) fn upsert_role_source(
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: &str,
    source: &RoleSource,
) -> anyhow::Result<()> {
    *config = upsert_role_source_on_disk(paths, key, source)?;
    Ok(())
}

fn upsert_role_source_on_disk(
    paths: &JackinPaths,
    key: &str,
    source: &RoleSource,
) -> anyhow::Result<AppConfig> {
    let (mut editor_doc, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
    emit_bootstrap_report(&bootstrap);
    editor_doc.upsert_agent_source(key, source);
    Ok(editor_doc.save()?)
}

pub(crate) fn start_role_source_persist(
    paths: JackinPaths,
    origin: jackin_console::tui::subscriptions::RoleSourcePersistOrigin<RoleSource>,
) -> jackin_console::tui::runtime::BlockingSubscription<
    jackin_console::tui::state::ManagerConfigSaveResult,
> {
    let (key, source) = match &origin {
        jackin_console::tui::subscriptions::RoleSourcePersistOrigin::RoleLoad {
            key,
            source,
            ..
        }
        | jackin_console::tui::subscriptions::RoleSourcePersistOrigin::TrustConfirm {
            key,
            source,
        } => (key.clone(), source.clone()),
    };
    jackin_console::tui::runtime::spawn_blocking_subscription(move || {
        let result = upsert_role_source_on_disk(&paths, &key, &source);
        jackin_console::tui::subscriptions::ConfigSaveResult::RoleSourcePersist { result, origin }
    })
}

fn remove_workspace_from_disk(paths: &JackinPaths, name: &str) -> anyhow::Result<AppConfig> {
    let (mut editor_doc, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
    emit_bootstrap_report(&bootstrap);
    editor_doc.remove_workspace(&WorkspaceName::parse(name).map_err(anyhow::Error::from)?)?;
    Ok(editor_doc.save()?)
}

pub(crate) fn start_remove_workspace(
    paths: JackinPaths,
    cwd: std::path::PathBuf,
    name: String,
) -> jackin_console::tui::runtime::BlockingSubscription<
    jackin_console::tui::state::ManagerConfigSaveResult,
> {
    jackin_console::tui::runtime::spawn_blocking_subscription(move || {
        let result = remove_workspace_from_disk(&paths, &name);
        jackin_console::tui::subscriptions::ConfigSaveResult::RemoveWorkspace { result, cwd }
    })
}

#[cfg(test)]
pub(crate) fn save_global_mounts(
    paths: &JackinPaths,
    original: &[GlobalMountRow],
    pending: &[GlobalMountRow],
) -> anyhow::Result<AppConfig> {
    AppConfig::validate_global_mount_rows(pending)?;
    let (mut editor_doc, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
    emit_bootstrap_report(&bootstrap);
    for row in original {
        editor_doc.remove_mount(&row.name, row.scope.as_deref());
    }
    for row in pending {
        editor_doc.add_mount(&row.name, row.mount.clone(), row.scope.as_deref());
    }
    Ok(editor_doc.save()?)
}

pub(crate) enum WorkspaceSaveMode {
    Edit {
        original_name: String,
        pending_name: Option<String>,
        effective_removals: Vec<String>,
    },
    Create {
        name: String,
    },
}

pub(crate) struct WorkspaceSaveInput<'a> {
    pub mode: WorkspaceSaveMode,
    pub original: &'a WorkspaceConfig,
    pub pending: &'a WorkspaceConfig,
}

pub(crate) struct WorkspaceSaveResult {
    pub config: AppConfig,
    pub current_name: String,
    pub pending_rename: Option<String>,
}

#[expect(
    clippy::useless_let_if_seq,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(crate) fn save_workspace(
    paths: &JackinPaths,
    input: WorkspaceSaveInput<'_>,
) -> anyhow::Result<WorkspaceSaveResult> {
    let (mut editor_doc, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
    emit_bootstrap_report(&bootstrap);
    let (pending_rename, current_name) = match input.mode {
        WorkspaceSaveMode::Edit {
            original_name,
            pending_name,
            effective_removals,
        } => {
            let mut current_name = original_name;
            let mut rename_to = None;
            if let Some(new_name) = pending_name
                && new_name != current_name
            {
                editor_doc.rename_workspace(
                    &WorkspaceName::parse(&current_name).map_err(anyhow::Error::from)?,
                    &WorkspaceName::parse(&new_name).map_err(anyhow::Error::from)?,
                )?;
                current_name.clone_from(&new_name);
                rename_to = Some(new_name);
            }

            let mut edit = build_workspace_edit(input.original, input.pending);
            edit.remove_destinations = effective_removals;
            editor_doc.edit_workspace(
                &WorkspaceName::parse(&current_name).map_err(anyhow::Error::from)?,
                edit,
            )?;
            (rename_to, current_name)
        }
        WorkspaceSaveMode::Create { name } => {
            editor_doc.create_workspace(
                &WorkspaceName::parse(&name).map_err(anyhow::Error::from)?,
                input.pending.clone(),
            )?;
            (None, name)
        }
    };

    apply_workspace_save_diff_plan(
        &mut editor_doc,
        &WorkspaceName::parse(&current_name).map_err(anyhow::Error::from)?,
        input.original,
        input.pending,
    )?;
    let config = editor_doc.save()?;
    Ok(WorkspaceSaveResult {
        config,
        current_name,
        pending_rename,
    })
}

pub(crate) fn start_workspace_save(
    paths: JackinPaths,
    mode: WorkspaceSaveMode,
    original: WorkspaceConfig,
    pending: WorkspaceConfig,
    exit_on_success: bool,
) -> jackin_console::tui::runtime::BlockingSubscription<
    jackin_console::tui::state::ManagerConfigSaveResult,
> {
    jackin_console::tui::runtime::spawn_blocking_subscription(move || {
        let result = save_workspace(
            &paths,
            WorkspaceSaveInput {
                mode,
                original: &original,
                pending: &pending,
            },
        )
        .map(
            |saved| jackin_console::tui::subscriptions::WorkspaceSaveResult {
                config: saved.config,
                current_name: saved.current_name,
                pending_rename: saved.pending_rename,
            },
        );
        jackin_console::tui::subscriptions::ConfigSaveResult::Workspace {
            result,
            exit_on_success,
        }
    })
}

pub(crate) struct OwnedSettingsSaveInput {
    pub mounts_original: Vec<GlobalMountRow>,
    pub mounts_pending: Vec<GlobalMountRow>,
    pub env_original: jackin_console::tui::state::SettingsEnvConfig,
    pub env_pending: jackin_console::tui::state::SettingsEnvConfig,
    pub auth_pending: std::collections::BTreeMap<String, jackin_config::AccountConfig>,
    pub auth_original: std::collections::BTreeMap<String, jackin_config::AccountConfig>,
    pub bindings_pending: std::collections::BTreeMap<jackin_core::Agent, String>,
    pub bindings_original: std::collections::BTreeMap<jackin_core::Agent, String>,
    pub github: jackin_config::GithubAuthConfig,
    pub original_github: jackin_config::GithubAuthConfig,
    pub trust_pending: Vec<jackin_console::tui::state::SettingsTrustRow>,
    pub git_coauthor_trailer: bool,
    pub git_dco: bool,
}

impl OwnedSettingsSaveInput {
    fn as_borrowed(&self) -> SettingsSaveInput<'_> {
        SettingsSaveInput {
            mounts_original: &self.mounts_original,
            mounts_pending: &self.mounts_pending,
            env_original: &self.env_original,
            env_pending: &self.env_pending,
            auth_pending: &self.auth_pending,
            auth_original: &self.auth_original,
            bindings_pending: &self.bindings_pending,
            bindings_original: &self.bindings_original,
            github: &self.github,
            original_github: &self.original_github,
            trust_pending: &self.trust_pending,
            git_coauthor_trailer: self.git_coauthor_trailer,
            git_dco: self.git_dco,
        }
    }
}

pub(crate) fn start_settings_save(
    paths: JackinPaths,
    input: OwnedSettingsSaveInput,
) -> jackin_console::tui::runtime::BlockingSubscription<
    jackin_console::tui::state::ManagerConfigSaveResult,
> {
    jackin_console::tui::runtime::spawn_blocking_subscription(move || {
        let result = save_settings_first_run_aware(&paths, &input);
        jackin_console::tui::subscriptions::ConfigSaveResult::Settings(result)
    })
}

/// Settings save with first-run bootstrap surfaced. The pre-open
/// consumes any installer marker and runs the initial scan under the
/// config lock; [`save_settings`] then applies the UI diff on top of
/// the bootstrapped config (bootstrap IDs are absent from the UI
/// originals, so they are preserved — and the returned config carries
/// them back to the UI refresh path).
fn save_settings_first_run_aware(
    paths: &JackinPaths,
    input: &OwnedSettingsSaveInput,
) -> anyhow::Result<AppConfig> {
    let (editor, bootstrap) = jackin_config::ConfigEditor::open_detailed(paths)?;
    drop(editor);
    emit_bootstrap_report(&bootstrap);
    save_settings(paths, input.as_borrowed())
}

/// Surface a first-run bootstrap report through operator diagnostics.
/// The config-save channel carries `AppConfig` only, so the fresh
/// flag, added IDs, and discovery issues ride the diagnostics surface
/// instead. Secret-free: account IDs, counts, agents, error
/// categories, and directories — never credential values or 1Password
/// item IDs. Silent when the report is empty.
fn emit_bootstrap_report(report: &BootstrapReport) {
    if report.fresh_install {
        let added = report.added_accounts.len();
        let ids = report.added_accounts.join(", ");
        jackin_diagnostics::emit_compact_line(
            "info",
            &format!("jackin: first-run account scan imported {added} account(s): {ids}"),
        );
    }
    for issue in &report.issues {
        let agent = issue.agent;
        let error = issue.error;
        jackin_diagnostics::emit_compact_line(
            "warning",
            &format!(
                "jackin: account scan issue: {agent}: {error} ({})",
                issue.directory.display()
            ),
        );
    }
}

/// Spawn the Settings account-scan worker. Discovery is blocking
/// filesystem/Keychain I/O — it must never run on the UI thread.
/// Candidates are returned unsaved; the Accounts tab joins them into
/// the pending draft (Apply commits, Cancel preserves). The echoed
/// `generation` lets the scan reducer ignore orphaned completions.
pub(crate) fn start_account_scan(
    paths: JackinPaths,
    generation: u64,
) -> jackin_console::tui::runtime::BlockingSubscription<(u64, Result<AccountScanOutcome, String>)> {
    jackin_console::tui::runtime::spawn_blocking_subscription(move || {
        (generation, run_account_scan(&paths))
    })
}

/// Blocking scan body: open under the config lock (first-run aware),
/// scan, drop without saving. Concurrent scans serialize on the lock;
/// the loser dedupes against the winner's committed accounts. Error
/// strings carry open/scan failures only (lock, IO, TOML shape) —
/// never credential values.
fn run_account_scan(paths: &JackinPaths) -> Result<AccountScanOutcome, String> {
    let (mut editor, open_report) =
        jackin_config::ConfigEditor::open_detailed(paths).map_err(|error| format!("{error:#}"))?;
    let scan_report = editor
        .scan_for_accounts()
        .map_err(|error| format!("{error:#}"))?;
    drop(editor);
    Ok(AccountScanOutcome {
        fresh_install: open_report.fresh_install,
        committed: open_report.added,
        candidates: scan_report.added,
        issues: open_report
            .issues
            .into_iter()
            .chain(scan_report.issues)
            .collect(),
    })
}

fn apply_workspace_save_diff_plan(
    editor_doc: &mut jackin_config::ConfigEditor,
    workspace_name: &WorkspaceName,
    original: &WorkspaceConfig,
    pending: &WorkspaceConfig,
) -> anyhow::Result<()> {
    for op in workspace_save_diff_plan(workspace_name, original, pending) {
        match op {
            WorkspaceSaveDiffOp::WorkspaceAccounts { accounts } => {
                editor_doc.set_workspace_accounts(workspace_name, &accounts)?;
            }
            WorkspaceSaveDiffOp::WorkspaceAccountBinding { agent, account } => {
                editor_doc.set_account_binding(
                    Some(workspace_name),
                    None,
                    agent,
                    account.as_deref(),
                )?;
            }
            WorkspaceSaveDiffOp::WorkspaceRoleAccountBinding {
                role,
                agent,
                account,
            } => {
                editor_doc.set_account_binding(
                    Some(workspace_name),
                    Some(&role),
                    agent,
                    account.as_deref(),
                )?;
            }
            WorkspaceSaveDiffOp::WorkspaceGithubAuthForward { mode } => {
                editor_doc.set_workspace_github_auth_forward(workspace_name, mode);
            }
            WorkspaceSaveDiffOp::WorkspaceRoleGithubAuthForward { role, mode } => {
                editor_doc.set_workspace_role_github_auth_forward(workspace_name, &role, mode);
            }
            WorkspaceSaveDiffOp::EnvSet { scope, key, value } => {
                editor_doc.set_env_var(&scope, &key, value)?;
            }
            WorkspaceSaveDiffOp::EnvRemove { scope, key } => {
                let _ = editor_doc.remove_env_var(&scope, &key);
            }
        }
    }
    Ok(())
}
