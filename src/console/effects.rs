//! Workspace-manager effect executors and background polling.

use crate::config::AppConfig;
use crate::console::tui::op_picker::OpPickerState;
use crate::console::tui::effect::{ManagerEffect, WorkspaceSaveEffect, WorkspaceSaveWriteInput, WorkspaceSaveWriteMode};
use crate::console::services::instances::load_instance_refresh_snapshot;
use jackin_console::tui::effect::ConsoleEffect;
use jackin_tui::runtime::spawn_blocking_subscription;

use crate::console::tui::message::{ManagerMessage, update_manager};
use crate::console::tui::state::{
    CreatePreludeState, EditorMode, EditorSaveFlow, EditorState, FileBrowserTarget, GlobalMountModal,
    ManagerListRow, ManagerStage, ManagerState, Modal, PendingDriftCheck, PendingIsolationCleanup,
    PendingMountInfoRefresh, PendingRoleLoad,
};

pub(crate) fn request_file_browser_git_url_resolution(
    state: &mut jackin_console::tui::components::file_browser::FileBrowserState,
    path: std::path::PathBuf,
) {
    crate::console::services::file_browser::request_git_url_resolution(state, path);
}

pub(crate) fn apply_file_browser_outcome(
    state: &mut jackin_console::tui::components::file_browser::FileBrowserState,
    outcome: jackin_console::tui::components::file_browser::FileBrowserOutcome<std::path::PathBuf>,
) -> jackin_console::tui::components::file_browser::FileBrowserOutcome<std::path::PathBuf> {
    crate::console::services::file_browser::apply_outcome(state, outcome)
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
        ManagerEffect::PersistTrustedRoleSource { key, source } => {
            execute_trusted_role_source_persist(state, config, paths, &key, source);
        }
        ManagerEffect::OpenCreatePreludeFileBrowser => {
            execute_create_prelude_file_browser_open(state);
        }
        ManagerEffect::OpenCreatePreludeFileBrowserAtLastCwd => {
            execute_create_prelude_file_browser_reopen(state);
        }
        ManagerEffect::OpenEditorAddMountFileBrowser => {
            execute_editor_add_mount_file_browser_open(state);
        }
        ManagerEffect::OpenGlobalMountFileBrowser => {
            execute_global_mount_file_browser_open(state);
        }
        ManagerEffect::ValidateOpCommit {
            op_ref,
            is_settings,
        } => execute_op_commit_validation(state, op_ref, is_settings),
    }
}

pub(crate) fn execute_pending_workspace_save_commit(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    cwd: &std::path::Path,
) -> anyhow::Result<bool> {
    let pending = if let ManagerStage::Editor(editor) = &mut state.stage {
        match std::mem::replace(&mut editor.save_flow, EditorSaveFlow::Idle) {
            EditorSaveFlow::PendingCommit {
                plan,
                exit_on_success,
            } => Some((plan, exit_on_success)),
            other => {
                editor.save_flow = other;
                None
            }
        }
    } else {
        None
    };
    let Some((plan, exit_on_success)) = pending else {
        return Ok(false);
    };

    if let Some(effect) =
        crate::console::tui::input::save::commit_editor_save(state, config, plan, exit_on_success)?
    {
        execute_workspace_save_effect(state, config, paths, cwd, effect);
    }
    Ok(true)
}

fn execute_global_mount_file_browser_open(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    match crate::console::services::file_browser::from_home() {
        Ok(file_browser) => {
            settings
                .mounts
                .open_sub_modal(GlobalMountModal::FileBrowser {
                    state: Box::new(file_browser),
                });
        }
        Err(error) => {
            settings.mounts.add_draft = None;
            settings.mounts.error = Some(error.to_string());
        }
    }
}

fn execute_editor_add_mount_file_browser_open(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    match crate::console::services::file_browser::from_home() {
        Ok(file_browser) => {
            editor.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::EditAddMountSrc,
                state: file_browser,
            });
        }
        Err(error) => {
            crate::console::tui::input::editor::open_editor_action_error(editor, &error);
        }
    }
}

fn execute_create_prelude_file_browser_open(state: &mut ManagerState<'_>) {
    match crate::console::services::file_browser::from_home() {
        Ok(file_browser) => {
            let mut prelude = CreatePreludeState::new();
            prelude.modal = Some(Modal::FileBrowser {
                target: FileBrowserTarget::CreateFirstMountSrc,
                state: file_browser,
            });
            let _ = update_manager(state, ManagerMessage::EnterCreatePrelude(prelude));
        }
        Err(error) => {
            let _ = update_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: "File browser failed".into(),
                    message: format!("{error:#}"),
                },
            );
        }
    }
}

fn execute_create_prelude_file_browser_reopen(state: &mut ManagerState<'_>) {
    let ManagerStage::CreatePrelude(prelude) = &mut state.stage else {
        return;
    };
    let Ok(mut file_browser) = crate::console::services::file_browser::from_home() else {
        prelude.modal = None;
        return;
    };
    if let Some(cwd) = prelude.last_browser_cwd.as_ref() {
        crate::console::services::file_browser::clamp_to_cwd(&mut file_browser, cwd);
    }
    prelude.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: file_browser,
    });
}

pub(crate) fn detect_op_available() -> bool {
    crate::console::services::op::cli_available()
}

pub(crate) async fn resolve_supported_agents_for_console(
    paths: &crate::paths::JackinPaths,
    config: &AppConfig,
    role: &crate::selector::RoleSelector,
    runner: &mut impl crate::docker::CommandRunner,
) -> anyhow::Result<Vec<crate::agent::Agent>> {
    crate::console::services::agents::resolve_supported_for_console(paths, config, role, runner).await
}

pub(crate) fn execute_open_url(state: &mut ManagerState<'_>, url: &str) -> bool {
    match crate::console::services::browser::open_url(url) {
        Ok(()) => false,
        Err(error) => {
            report_open_url_error(state, error);
            true
        }
    }
}

pub(crate) fn execute_remove_workspace(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    cwd: &std::path::Path,
    name: &str,
) -> bool {
    match crate::console::services::config::remove_workspace(config, paths, name) {
        Ok(()) => {
            let _ = update_manager(
                state,
                ManagerMessage::ReloadFromConfig {
                    config: Box::new(config.clone()),
                    cwd: cwd.to_path_buf(),
                },
            );
        }
        Err(error) => {
            let _ = update_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: "Delete failed".into(),
                    message: format!("{error:#}"),
                },
            );
        }
    }
    true
}

pub(crate) fn resolve_pending_provider_launch(
    state: &mut crate::console::ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    selector: &crate::selector::RoleSelector,
) -> anyhow::Result<Option<crate::workspace::ResolvedWorkspace>> {
    let Some(input) = state.pending_launch.take() else {
        return Ok(None);
    };
    let Some(choice) = crate::console::domain::build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    crate::console::preview::resolve_selected_workspace(config, cwd, &choice, selector).map(Some)
}

pub(crate) enum LaunchDispatchResolution {
    NoEligibleRoles {
        name: String,
    },
    SingleRole {
        role: crate::selector::RoleSelector,
        workspace: crate::workspace::ResolvedWorkspace,
    },
    RolePicker {
        input: crate::workspace::LoadWorkspaceInput,
        roles: Vec<crate::selector::RoleSelector>,
        selected: Option<usize>,
    },
}

pub(crate) fn resolve_launch_dispatch(
    config: &AppConfig,
    cwd: &std::path::Path,
    input: crate::workspace::LoadWorkspaceInput,
) -> anyhow::Result<Option<LaunchDispatchResolution>> {
    let Some(choice) = crate::console::domain::build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let roles = choice.allowed_roles.clone();

    if roles.is_empty() {
        return Ok(Some(LaunchDispatchResolution::NoEligibleRoles {
            name: choice.name,
        }));
    }

    if roles.len() == 1 {
        let role = roles.into_iter().next().unwrap();
        let workspace =
            crate::console::preview::resolve_selected_workspace(config, cwd, &choice, &role)?;
        return Ok(Some(LaunchDispatchResolution::SingleRole { role, workspace }));
    }

    let selected = crate::app::context::preferred_agent_index(
        &roles,
        choice.last_role.as_deref(),
        choice.default_role.as_deref(),
    );
    Ok(Some(LaunchDispatchResolution::RolePicker {
        input,
        roles,
        selected,
    }))
}

pub(crate) fn global_mounts_require_sensitive_confirmation(
    mounts: &[crate::config::GlobalMountRow],
) -> bool {
    crate::console::domain::global_rows_have_sensitive_mount(mounts)
}

pub(crate) struct CommittedRoleLaunch {
    pub(crate) input: crate::workspace::LoadWorkspaceInput,
    pub(crate) workspace: crate::workspace::ResolvedWorkspace,
}

pub(crate) fn resolve_committed_role_launch(
    state: &mut crate::console::ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    role: &crate::selector::RoleSelector,
) -> anyhow::Result<Option<CommittedRoleLaunch>> {
    let Some(input) = state.pending_launch.take() else {
        return Ok(None);
    };
    let Some(choice) = crate::console::domain::build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = crate::console::preview::resolve_selected_workspace(config, cwd, &choice, role)?;
    Ok(Some(CommittedRoleLaunch { input, workspace }))
}

pub(crate) struct CommittedAgentLaunch {
    pub(crate) input: crate::workspace::LoadWorkspaceInput,
    pub(crate) role: crate::selector::RoleSelector,
    pub(crate) workspace: crate::workspace::ResolvedWorkspace,
    pub(crate) providers: Vec<jackin_protocol::Provider>,
}

pub(crate) fn resolve_committed_agent_launch(
    state: &mut crate::console::ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    agent: crate::agent::Agent,
) -> anyhow::Result<Option<CommittedAgentLaunch>> {
    let (Some(input), Some(role)) = (
        state.pending_launch.take(),
        state.pending_launch_role.take(),
    ) else {
        return Ok(None);
    };
    let Some(choice) = crate::console::domain::build_workspace_choice(config, cwd, &input)? else {
        return Ok(None);
    };
    let workspace = crate::console::preview::resolve_selected_workspace(config, cwd, &choice, &role)?;
    let providers = crate::console::domain::providers_for_launch(config, &choice.name, &role.key(), agent);
    Ok(Some(CommittedAgentLaunch {
        input,
        role,
        workspace,
        providers,
    }))
}

pub(crate) fn apply_role_load_completion(
    editor: &mut EditorState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    load: PendingRoleLoad,
    result: anyhow::Result<()>,
) {
    match result {
        Ok(()) => {
            if let Err(e) = execute_role_source_persist(config, paths, &load.key, &load.source) {
                crate::debug_log!(
                    "role",
                    "role loader failed for key={key:?} raw={raw:?}: {e:?}",
                    key = load.key,
                    raw = load.raw
                );
                crate::console::tui::input::editor::open_role_resolution_error(
                    editor,
                    &load.raw,
                    Some(&load.source.git),
                    &e.context("role repository loaded, but registration could not be persisted"),
                );
                return;
            }
            crate::debug_log!(
                "role",
                "role repo registration completed for key={key:?} git={git:?}",
                key = load.key,
                git = load.source.git.as_str()
            );
            if load.source.trusted {
                crate::debug_log!(
                    "role",
                    "role source is trusted; adding key={key:?} directly to the workspace",
                    key = load.key
                );
                crate::console::tui::input::editor::add_role_to_workspace_editor(
                    editor, config, &load.key,
                );
            } else {
                crate::debug_log!(
                    "role",
                    "role source registered untrusted; opening trust confirm for key={key:?} git={git:?}",
                    key = load.key,
                    git = load.source.git.as_str()
                );
                crate::console::tui::input::editor::open_role_trust_confirm(
                    editor,
                    load.key,
                    load.source,
                );
            }
        }
        Err(e) => {
            crate::debug_log!(
                "role",
                "role loader failed for key={key:?} raw={raw:?}: {e:?}",
                key = load.key,
                raw = load.raw
            );
            let err_text = e.to_string();
            if let Some(panic_message) = err_text.strip_prefix("role loader panicked: ") {
                crate::console::tui::input::editor::open_role_input_error(
                    editor,
                    &format!(
                        "Could not load role {:?}.\n\nThe role loader hit an internal \
                         error while registering the repository.\n\n{panic_message}",
                        load.raw
                    ),
                );
                return;
            }
            crate::console::tui::input::editor::open_role_resolution_error(
                editor,
                &load.raw,
                Some(&load.source.git),
                &e,
            );
        }
    }
}

fn execute_trusted_role_source_persist(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    key: &str,
    mut source: crate::config::RoleSource,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    source.trusted = true;
    match execute_role_source_persist(config, paths, key, &source) {
        Ok(()) => {
            crate::console::tui::input::editor::add_role_to_workspace_editor(editor, config, key);
        }
        Err(error) => {
            crate::console::tui::input::editor::open_editor_action_error(editor, &error);
        }
    }
}

pub(crate) fn token_generate_label(req: &crate::console::tui::state::PendingTokenGenerate) -> String {
    use crate::workspace::token_setup::TokenSetupScope;

    match &req.scope {
        TokenSetupScope::Workspace(name) => format!("workspace {name:?}"),
        TokenSetupScope::WorkspaceRole { workspace, role } => {
            format!("workspace {workspace:?} role {role:?}")
        }
        TokenSetupScope::Global => "global config".to_string(),
    }
}

pub(crate) fn take_pending_token_generate(
    state: &mut ManagerState<'_>,
) -> Option<crate::console::tui::state::PendingTokenGenerate> {
    match &mut state.stage {
        ManagerStage::Editor(editor) => editor.pending_token_generate.take(),
        ManagerStage::Settings(settings) => settings.pending_token_generate.take(),
        _ => None,
    }
}

pub(crate) fn execute_token_generate(
    paths: &crate::paths::JackinPaths,
    config: &AppConfig,
    req: &crate::console::tui::state::PendingTokenGenerate,
) -> anyhow::Result<crate::operator_env::EnvValue> {
    crate::console::services::token_setup::mint_token_value(
        paths,
        config,
        &req.scope,
        &req.args,
    )
}

pub(crate) fn apply_token_generate_result(
    state: &mut ManagerState<'_>,
    result: anyhow::Result<crate::operator_env::EnvValue>,
) {
    match result {
        Ok(env_value) => apply_generated_token(state, env_value),
        Err(error) => report_token_generate_error(state, error),
    }
}

fn apply_generated_token(
    state: &mut ManagerState<'_>,
    env_value: crate::operator_env::EnvValue,
) {
    if let crate::operator_env::EnvValue::OpRef(op_ref) = &env_value {
        crate::console::services::op_picker::invalidate_cache_for_ref(&state.op_cache, op_ref);
    }

    match &mut state.stage {
        ManagerStage::Editor(editor) => match env_value {
            crate::operator_env::EnvValue::OpRef(op_ref) => {
                crate::console::tui::input::auth::apply_op_picker_to_auth_form_committed(
                    editor,
                    op_ref,
                );
            }
            crate::operator_env::EnvValue::Plain(value) => {
                crate::console::tui::input::auth::apply_plain_text_to_auth_form(editor, &value);
            }
        },
        ManagerStage::Settings(settings) => match env_value {
            crate::operator_env::EnvValue::OpRef(op_ref) => {
                crate::console::tui::input::apply_op_picker_to_settings_auth_form_committed(
                    &mut settings.auth,
                    op_ref,
                );
            }
            crate::operator_env::EnvValue::Plain(value) => {
                crate::console::tui::input::apply_plain_text_to_settings_auth_form(
                    &mut settings.auth,
                    &value,
                );
            }
        },
        _ => {}
    }
}

fn report_token_generate_error(state: &mut ManagerState<'_>, error: anyhow::Error) {
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            editor.modal = Some(Modal::ErrorPopup {
                state: jackin_tui::components::ErrorPopupState::new(
                    "Token generation failed",
                    error.to_string(),
                ),
            });
        }
        ManagerStage::Settings(_) => {
            let _ = update_manager(
                state,
                ManagerMessage::OpenSettingsErrorPopup {
                    title: "Token generation failed".into(),
                    message: error.to_string(),
                },
            );
        }
        _ => {}
    }
}

fn report_open_url_error(state: &mut ManagerState<'_>, error: anyhow::Error) {
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            editor.modal = Some(Modal::ErrorPopup {
                state: jackin_tui::components::ErrorPopupState::new(
                    "Failed to open URL",
                    error.to_string(),
                ),
            });
        }
        ManagerStage::Settings(_) => {
            let _ = update_manager(
                state,
                ManagerMessage::OpenSettingsErrorPopup {
                    title: "Failed to open URL".into(),
                    message: error.to_string(),
                },
            );
        }
        _ => {
            let _ = update_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: "Failed to open URL".into(),
                    message: error.to_string(),
                },
            );
        }
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
        editor.pending_role_load = Some(crate::console::tui::state::PendingRoleLoad {
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
                Some(crate::console::tui::state::PendingOpCommit::new(op_ref, rx));
        }
    } else if let ManagerStage::Editor(editor) = &mut state.stage {
        editor.pending_op_commit = Some(crate::console::tui::state::PendingOpCommit::new(op_ref, rx));
    }
}

pub(crate) fn execute_workspace_save_effect(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    cwd: &std::path::Path,
    effect: WorkspaceSaveEffect,
) {
    match effect {
        WorkspaceSaveEffect::StartDriftCheck {
            original_name,
            prospective_mounts,
            plan,
            exit_on_success,
        } => {
            let has_records =
                crate::isolation::state::list_records_for_workspace(&paths.data_dir, &original_name)
                    .is_ok_and(|records| !records.is_empty());
            if !has_records {
                let (_tx, rx) = tokio::sync::oneshot::channel();
                let check = PendingDriftCheck::new(rx, original_name, plan, exit_on_success);
                if let Ok(Some(effect)) =
                    crate::console::tui::input::save::continue_save_after_drift_check(
                        state,
                        config,
                        check,
                        Ok(crate::config::DriftDetection::default()),
                    )
                {
                    execute_workspace_save_effect(state, config, paths, cwd, effect);
                }
                return;
            }
            let ManagerStage::Editor(editor) = &mut state.stage else {
                return;
            };
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
            let ManagerStage::Editor(editor) = &mut state.stage else {
                return;
            };
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
        WorkspaceSaveEffect::WriteWorkspace {
            mode,
            original,
            pending,
            exit_on_success,
        } => {
            execute_workspace_save_write(
                state,
                config,
                paths,
                cwd,
                WorkspaceSaveWriteInput {
                    mode,
                    original: &original,
                    pending: &pending,
                },
                exit_on_success,
            );
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
                editor.save_flow = crate::console::tui::state::EditorSaveFlow::Idle;
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
                crate::console::tui::input::save::open_save_error_popup(editor, &e.to_string());
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
    crate::console::tui::state::PendingRoleLoad,
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
        if let Some((load, result)) = crate::console::tui::input::editor::poll_role_load_completion(editor) {
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

pub(crate) fn apply_background_event(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &crate::paths::JackinPaths,
    cwd: &std::path::Path,
    event: ManagerBackgroundEvent,
) -> bool {
    match event {
        ManagerBackgroundEvent::Message(message) => update_manager(state, message).is_dirty(),
        ManagerBackgroundEvent::RoleLoadFinished { load, result } => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                apply_role_load_completion(editor, config, paths, load, result);
            }
            true
        }
        ManagerBackgroundEvent::DriftCheckFinished { check, detection } => {
            if let Ok(Some(effect)) =
                crate::console::tui::input::save::continue_save_after_drift_check(
                    state, config, check, detection,
                )
            {
                execute_workspace_save_effect(state, config, paths, cwd, effect);
            }
            true
        }
        ManagerBackgroundEvent::IsolationCleanupFinished { cleanup, result } => {
            if let Ok(Some(effect)) =
                crate::console::tui::input::save::continue_save_after_isolation_cleanup(
                    state, config, cleanup, result,
                )
            {
                execute_workspace_save_effect(state, config, paths, cwd, effect);
            }
            true
        }
    }
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
        && let Some(crate::console::tui::state::SettingsEnvModal::OpPicker { state }) =
            settings.env.modal.as_mut()
    {
        dirty |= poll_op_picker_load(state);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && let Some(crate::console::tui::state::SettingsAuthModal::OpPicker { state }) =
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
        crate::operator_env::default_op_struct_runner(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, RoleSource};
    use crate::workspace::{LoadWorkspaceInput, WorkspaceConfig};
    use std::collections::BTreeMap;

    fn role_source() -> RoleSource {
        RoleSource {
            git: "https://example.invalid/org/role.git".to_string(),
            trusted: true,
            env: BTreeMap::new(),
        }
    }

    fn workspace(workdir: &std::path::Path, allowed_roles: Vec<&str>) -> WorkspaceConfig {
        WorkspaceConfig {
            version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
            workdir: workdir.display().to_string(),
            mounts: vec![crate::workspace::MountConfig {
                src: workdir.display().to_string(),
                dst: workdir.display().to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_roles: allowed_roles.into_iter().map(str::to_string).collect(),
            default_role: None,
            default_agent: None,
            last_role: None,
            env: BTreeMap::new(),
            roles: BTreeMap::new(),
            keep_awake: crate::workspace::KeepAwakeConfig::default(),
            claude: None,
            codex: None,
            amp: None,
            kimi: None,
            opencode: None,
            github: None,
            git_pull_on_entry: false,
        }
    }

    #[test]
    fn resolve_launch_dispatch_returns_none_for_deleted_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();

        let resolution = resolve_launch_dispatch(
            &config,
            temp.path(),
            LoadWorkspaceInput::Saved("missing".to_string()),
        )
        .unwrap();

        assert!(resolution.is_none());
    }

    #[test]
    fn resolve_launch_dispatch_reports_no_eligible_roles() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config
            .workspaces
            .insert("empty".to_string(), workspace(temp.path(), Vec::new()));

        let resolution = resolve_launch_dispatch(
            &config,
            temp.path(),
            LoadWorkspaceInput::Saved("empty".to_string()),
        )
        .unwrap()
        .expect("workspace exists");

        assert!(matches!(
            resolution,
            LaunchDispatchResolution::NoEligibleRoles { name } if name == "empty"
        ));
    }

    #[test]
    fn resolve_launch_dispatch_resolves_single_role_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.roles.insert("smith".to_string(), role_source());
        config
            .workspaces
            .insert("solo".to_string(), workspace(temp.path(), vec!["smith"]));

        let resolution = resolve_launch_dispatch(
            &config,
            temp.path(),
            LoadWorkspaceInput::Saved("solo".to_string()),
        )
        .unwrap()
        .expect("workspace exists");

        let LaunchDispatchResolution::SingleRole { role, workspace } = resolution else {
            panic!("expected single-role launch dispatch");
        };
        assert_eq!(role.key(), "smith");
        assert_eq!(workspace.label, "solo");
    }

    #[test]
    fn resolve_launch_dispatch_preselects_role_picker() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.roles.insert("alpha".to_string(), role_source());
        config.roles.insert("beta".to_string(), role_source());
        let mut saved = workspace(temp.path(), vec!["alpha", "beta"]);
        saved.last_role = Some("beta".to_string());
        config.workspaces.insert("multi".to_string(), saved);

        let resolution = resolve_launch_dispatch(
            &config,
            temp.path(),
            LoadWorkspaceInput::Saved("multi".to_string()),
        )
        .unwrap()
        .expect("workspace exists");

        let LaunchDispatchResolution::RolePicker {
            roles, selected, ..
        } = resolution
        else {
            panic!("expected role picker dispatch");
        };
        assert_eq!(roles.iter().map(|role| role.key()).collect::<Vec<_>>(), vec!["alpha", "beta"]);
        assert_eq!(selected, Some(1));
    }
}
