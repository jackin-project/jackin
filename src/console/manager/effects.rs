//! Workspace-manager effect executors and background polling.

use crate::config::AppConfig;
use crate::console::op_picker::OpPickerState;
use crate::console::services::instances::load_instance_refresh_snapshot;
use jackin_console::tui::effect::ConsoleEffect;
use jackin_tui::runtime::spawn_blocking_subscription;

use super::message::{ManagerMessage, update_manager};
use super::state::{
    EditorMode, EditorState, GlobalMountModal, ManagerListRow, ManagerStage, ManagerState, Modal,
    PendingDriftCheck, PendingIsolationCleanup, PendingMountInfoRefresh, PendingSaveCommit,
};

#[derive(Debug)]
pub(crate) enum ManagerEffect {
    Console(ConsoleEffect),
    StartRoleRegistration {
        raw: String,
        key: String,
        selector: crate::selector::RoleSelector,
        source: crate::config::RoleSource,
    },
    ValidateOpCommit {
        op_ref: crate::operator_env::OpRef,
        is_settings: bool,
    },
}

pub(crate) enum WorkspaceSaveEffect {
    StartDriftCheck {
        original_name: String,
        prospective_mounts: Vec<crate::workspace::MountConfig>,
        plan: PendingSaveCommit,
        exit_on_success: bool,
    },
    StartIsolationCleanup {
        records: Vec<crate::isolation::state::IsolationRecord>,
        plan: PendingSaveCommit,
        exit_on_success: bool,
    },
}

pub(crate) enum WorkspaceSaveWriteMode {
    Edit {
        original_name: String,
        pending_name: Option<String>,
        effective_removals: Vec<String>,
    },
    Create {
        name: String,
    },
}

pub(crate) struct WorkspaceSaveWriteInput<'a> {
    pub(crate) mode: WorkspaceSaveWriteMode,
    pub(crate) original: &'a crate::workspace::WorkspaceConfig,
    pub(crate) pending: &'a crate::workspace::WorkspaceConfig,
}

impl From<ConsoleEffect> for ManagerEffect {
    fn from(effect: ConsoleEffect) -> Self {
        Self::Console(effect)
    }
}

pub(crate) fn execute_manager_effect(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    effect: ManagerEffect,
) {
    match effect {
        ManagerEffect::Console(ConsoleEffect::RequestActiveMountInfoRefresh) => {
            if state.mount_info_refresh_in_flight() {
                return;
            }
            let Some((target, sources)) = state.active_mount_info_sources(config) else {
                return;
            };
            if tokio::runtime::Handle::try_current().is_err() {
                let entries = jackin_console::services::mount_info::inspect_entries(sources);
                let _ = update_manager(
                    state,
                    ManagerMessage::MountInfoRefreshed(PendingMountInfoRefresh {
                        target,
                        entries,
                    }),
                );
                return;
            }
            let rx = spawn_blocking_subscription(move || {
                let entries = jackin_console::services::mount_info::inspect_entries(sources);
                PendingMountInfoRefresh { target, entries }
            });
            state.begin_mount_info_refresh(rx);
        }
        ManagerEffect::Console(ConsoleEffect::RequestInstanceRefresh) => {
            let Some(generation) = state.next_instance_refresh_generation_if_due() else {
                return;
            };
            let paths = paths.clone();
            let rx = spawn_blocking_subscription(move || {
                let result = load_instance_refresh_snapshot(&paths);
                (generation, result)
            });
            state.begin_instance_refresh(rx);
        }
        ManagerEffect::Console(ConsoleEffect::SaveSettings) => execute_settings_save(state, config, paths),
        ManagerEffect::StartRoleRegistration {
            raw,
            key,
            selector,
            source,
        } => execute_role_registration_start(state, paths, raw, key, selector, source),
        ManagerEffect::ValidateOpCommit {
            op_ref,
            is_settings,
        } => execute_op_commit_validation(state, op_ref, is_settings),
    }
}

fn execute_role_registration_start(
    state: &mut ManagerState<'_>,
    paths: &crate::paths::JackinPaths,
    raw: String,
    key: String,
    selector: crate::selector::RoleSelector,
    source: crate::config::RoleSource,
) {
    crate::debug_log!(
        "role",
        "registering role repo for key={key:?} git={git:?}",
        git = source.git.as_str()
    );
    let rx = crate::console::services::role_load::start_role_registration(
        paths.clone(),
        selector,
        source.git.clone(),
    );
    if let ManagerStage::Editor(editor) = &mut state.stage {
        editor.pending_role_load = Some(super::state::PendingRoleLoad {
            raw,
            key: key.clone(),
            source,
            rx,
        });
        editor.modal = Some(Modal::StatusPopup {
            state: jackin_tui::components::StatusPopupState::new(
                "Loading role",
                format!("Loading role {key}"),
            ),
        });
    }
}

fn execute_op_commit_validation(
    state: &mut ManagerState<'_>,
    op_ref: crate::operator_env::OpRef,
    is_settings: bool,
) {
    let rx = crate::console::services::op::start_ref_validation(op_ref.clone());
    if is_settings {
        if let ManagerStage::Settings(settings) = &mut state.stage {
            settings.auth.pending_op_commit =
                Some(super::state::PendingOpCommit::new(op_ref, rx));
        }
    } else if let ManagerStage::Editor(editor) = &mut state.stage {
        editor.pending_op_commit = Some(super::state::PendingOpCommit::new(op_ref, rx));
    }
}

pub(crate) fn execute_workspace_save_effect(
    editor: &mut EditorState<'_>,
    paths: &crate::paths::JackinPaths,
    effect: WorkspaceSaveEffect,
) {
    match effect {
        WorkspaceSaveEffect::StartDriftCheck {
            original_name,
            prospective_mounts,
            plan,
            exit_on_success,
        } => {
            let rx = crate::console::services::workspace_save::start_drift_check(
                paths.clone(),
                original_name.clone(),
                prospective_mounts,
            );
            editor.pending_drift_check = Some(PendingDriftCheck::new(
                rx,
                original_name,
                plan,
                exit_on_success,
            ));
            editor.modal = Some(Modal::StatusPopup {
                state: jackin_tui::components::StatusPopupState::new(
                    "Saving",
                    "Checking isolation records...",
                ),
            });
        }
        WorkspaceSaveEffect::StartIsolationCleanup {
            records,
            plan,
            exit_on_success,
        } => {
            let rx = crate::console::services::workspace_save::start_isolation_cleanup(
                paths.clone(),
                records,
            );
            editor.pending_isolation_cleanup =
                Some(PendingIsolationCleanup::new(rx, plan, exit_on_success));
            editor.modal = Some(Modal::StatusPopup {
                state: jackin_tui::components::StatusPopupState::new(
                    "Saving",
                    "Deleting isolated state...",
                ),
            });
        }
    }
}

pub(crate) fn execute_workspace_save_write(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    cwd: &std::path::Path,
    input: WorkspaceSaveWriteInput<'_>,
    exit_on_success: bool,
) {
    let mode = match input.mode {
        WorkspaceSaveWriteMode::Edit {
            original_name,
            pending_name,
            effective_removals,
        } => crate::console::services::config::WorkspaceSaveMode::Edit {
            original_name,
            pending_name,
            effective_removals,
        },
        WorkspaceSaveWriteMode::Create { name } => {
            crate::console::services::config::WorkspaceSaveMode::Create { name }
        }
    };
    let service_input = crate::console::services::config::WorkspaceSaveInput {
        mode,
        original: input.original,
        pending: input.pending,
    };
    match crate::console::services::config::save_workspace(paths, service_input) {
        Ok(saved) => {
            *config = saved.config;
            if let ManagerStage::Editor(editor) = &mut state.stage {
                if let Some(new_name) = saved.pending_rename {
                    editor.mode = EditorMode::Edit { name: new_name };
                }
                if let EditorMode::Edit { name } = &editor.mode
                    && let Some(ws) = config.workspaces.get(name)
                {
                    editor.original = ws.clone();
                    editor.pending = ws.clone();
                }
                editor.save_flow = super::state::EditorSaveFlow::Idle;
            }
            if exit_on_success
                || matches!(
                    state.stage,
                    ManagerStage::Editor(EditorState {
                        mode: EditorMode::Create,
                        ..
                    })
                )
            {
                let _ = update_manager(
                    state,
                    ManagerMessage::ReloadFromConfig {
                        config: Box::new(config.clone()),
                        cwd: cwd.to_path_buf(),
                    },
                );
                let saved_count = state.workspaces.len();
                if let Some(idx) = state
                    .workspaces
                    .iter()
                    .position(|w| w.name == saved.current_name)
                {
                    state.selected = ManagerListRow::SavedWorkspace(idx)
                        .to_screen_index(saved_count)
                        .unwrap_or(0);
                }
            }
        }
        Err(e) => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                super::input::save::open_save_error_popup(editor, &e.to_string());
            }
        }
    }
}

pub(crate) fn execute_role_source_persist(
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    key: &str,
    source: &crate::config::RoleSource,
) -> anyhow::Result<()> {
    crate::console::services::config::upsert_role_source(config, paths, key, source)
}

fn execute_settings_save(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    match crate::console::services::config::save_settings(
        paths,
        crate::console::services::config::SettingsSaveInput {
            mounts_original: &settings.mounts.original,
            mounts_pending: &settings.mounts.pending,
            env_original: &settings.env.original,
            env_pending: &settings.env.pending,
            auth_pending: &settings.auth.pending,
            original_github_env: &settings.auth.original_github_env,
            github_env: &settings.auth.github_env,
            trust_pending: &settings.trust.pending,
            git_coauthor_trailer: settings.general.pending_coauthor_trailer,
            git_dco: settings.general.pending_dco,
        },
    ) {
        Ok(saved) => {
            *config = saved;
            settings.mark_saved();
            settings.mounts.exit_requested = true;
        }
        Err(err) => settings.mounts.error = Some(err.to_string()),
    }
}

pub(crate) type ManagerBackgroundEvent = jackin_console::tui::message::BackgroundEvent<
    ManagerMessage,
    super::state::PendingRoleLoad,
    PendingDriftCheck,
    crate::config::DriftDetection,
    PendingIsolationCleanup,
>;

pub(crate) fn poll_background_messages(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
) -> Vec<ManagerBackgroundEvent> {
    let mut messages = vec![
        ManagerBackgroundEvent::Message(ManagerMessage::PollFileBrowserGitUrls),
        ManagerBackgroundEvent::Message(ManagerMessage::PollPickerLoads),
    ];
    if let ManagerStage::Editor(editor) = &mut state.stage {
        if let Some((load, result)) = super::input::editor::poll_role_load_completion(editor) {
            messages.push(ManagerBackgroundEvent::RoleLoadFinished { load, result });
        }
    }
    execute_manager_effect(
        state,
        config,
        paths,
        ConsoleEffect::RequestActiveMountInfoRefresh.into(),
    );
    if let Some(result) = state.poll_mount_info_refresh() {
        messages.push(ManagerBackgroundEvent::Message(
            ManagerMessage::MountInfoRefreshed(result),
        ));
    }
    if let Some(result) = state.poll_instance_refresh() {
        messages.push(ManagerBackgroundEvent::Message(
            ManagerMessage::InstancesRefreshed(result),
        ));
    }
    execute_manager_effect(state, config, paths, ConsoleEffect::RequestInstanceRefresh.into());
    if let Some((op_ref, result, is_settings)) = state.poll_pending_op_commit() {
        messages.push(ManagerBackgroundEvent::Message(
            ManagerMessage::OpCommitResolved {
                op_ref,
                result,
                is_settings,
            },
        ));
    }
    if let Some((check, detection)) = state.poll_pending_drift_check() {
        messages.push(ManagerBackgroundEvent::DriftCheckFinished { check, detection });
    }
    if let Some((cleanup, result)) = state.poll_pending_isolation_cleanup() {
        messages.push(ManagerBackgroundEvent::IsolationCleanupFinished { cleanup, result });
    }
    messages
}

/// Drained from the outer event loop every tick so picker results land without
/// keystroke pumping. This executor starts non-TUI load services for pending
/// picker requests, then routes completed subscriptions back into picker state.
pub(crate) fn poll_picker_loads(state: &mut ManagerState<'_>) -> bool {
    let mut dirty = false;
    if let Some(Modal::OpPicker { state }) = state.list_modal.as_mut() {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Editor(editor) = &mut state.stage
        && let Some(Modal::OpPicker { state }) = editor.modal.as_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && let Some(super::state::SettingsEnvModal::OpPicker { state }) =
            settings.env.modal.as_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && let Some(super::state::SettingsAuthModal::OpPicker { state }) =
            settings.auth.modal.as_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    dirty
}

fn poll_op_picker_load(state: &mut OpPickerState) -> bool {
    let mut dirty = execute_op_picker_pending_load(state);
    dirty |= state.poll_load();
    dirty |= execute_op_picker_pending_load(state);
    dirty
}

fn execute_op_picker_pending_load(state: &mut OpPickerState) -> bool {
    let Some(pending) = state.take_pending_load() else {
        return false;
    };
    let rx = crate::console::services::op_picker::start_load(
        pending.cached,
        pending.request,
        pending.runner,
    );
    state.attach_load_receiver(rx);
    true
}

pub(crate) fn poll_file_browser_git_urls(state: &mut ManagerState<'_>) -> bool {
    let mut dirty = false;
    if let Some(modal) = state.list_modal.as_mut() {
        dirty |= poll_modal_file_browser_git_url(modal);
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = editor.modal.as_mut() {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
            for modal in &mut editor.modal_parents {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = prelude.modal.as_mut() {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(modal) = settings.mounts.modal.as_mut() {
                dirty |= poll_global_mount_file_browser_git_url(modal);
            }
            for modal in &mut settings.mounts.modal_parents {
                dirty |= poll_global_mount_file_browser_git_url(modal);
            }
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    dirty
}

fn poll_modal_file_browser_git_url(modal: &mut Modal<'_>) -> bool {
    match modal {
        Modal::FileBrowser { state, .. } => state.poll_git_url_resolution(),
        _ => false,
    }
}

fn poll_global_mount_file_browser_git_url(modal: &mut GlobalMountModal<'_>) -> bool {
    match modal {
        GlobalMountModal::FileBrowser { state } => state.poll_git_url_resolution(),
        _ => false,
    }
}
