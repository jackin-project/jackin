//! Workspace-manager effect executors and background polling.

use crate::console::services::instances::load_instance_refresh_snapshot;
use crate::console::tui::{
    ManagerEffect, WorkspaceSaveEffect, WorkspaceSaveWriteInput, WorkspaceSaveWriteMode,
};
use jackin_config::AppConfig;
use jackin_console::tui::effect::ConsoleEffect;
use jackin_console::tui::screens::workspaces::update::saved_workspace_selected_index;
use jackin_tui::runtime::spawn_blocking_subscription;

use crate::console::tui::state::{
    EditorMode, EditorSaveFlow, EditorState, ManagerStage, ManagerState, Modal, PendingDriftCheck,
    PendingIsolationCleanup, PendingMountInfoRefresh, PendingRoleLoad,
};
use jackin_console::tui::components::error_popup;
use jackin_console::tui::components::status_popup;
use jackin_console::tui::state::update::{ManagerBackgroundEvent, ManagerMessage, update_manager};

pub(crate) fn op_cli_available() -> bool {
    jackin_console::tui::op_picker::cli_available()
}

pub(crate) fn execute_manager_effect(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
    effect: ManagerEffect,
) -> bool {
    match effect {
        ManagerEffect::Console(ConsoleEffect::RequestActiveMountInfoRefresh) => {
            if state.mount_info_refresh_in_flight() {
                return false;
            }
            let Some((target, sources)) = state.active_mount_info_sources(config) else {
                return false;
            };
            let rx = spawn_blocking_subscription(move || {
                let entries = jackin_console::services::mount_info::inspect_entries(sources);
                PendingMountInfoRefresh { target, entries }
            });
            state.begin_mount_info_refresh(rx);
            false
        }
        ManagerEffect::Console(ConsoleEffect::RequestInstanceRefresh) => {
            let Some(generation) = state.next_instance_refresh_generation_if_due() else {
                return false;
            };
            let paths = paths.clone();
            let rx = spawn_blocking_subscription(move || {
                let result = load_instance_refresh_snapshot(&paths);
                (generation, result)
            });
            state.begin_instance_refresh(rx);
            false
        }
        ManagerEffect::Console(ConsoleEffect::SaveSettings) => {
            execute_settings_save(state, config, paths);
            true
        }
        ManagerEffect::StartRoleRegistration {
            raw,
            key,
            selector,
            source,
        } => {
            execute_role_registration_start(state, paths, raw, &key, selector, source);
            true
        }
        ManagerEffect::PersistTrustedRoleSource { key, source } => {
            execute_trusted_role_source_persist(state, config, paths, &key, source);
            true
        }
        ManagerEffect::OpenCreatePreludeFileBrowser => {
            jackin_console::tui::file_browser::start_create_prelude_file_browser_open(state)
        }
        ManagerEffect::OpenCreatePreludeFileBrowserAtLastCwd => {
            jackin_console::tui::file_browser::start_create_prelude_file_browser_reopen(state)
        }
        ManagerEffect::OpenEditorAuthSourceFolderBrowser => {
            jackin_console::tui::file_browser::start_editor_auth_source_folder_browser_open(state)
        }
        ManagerEffect::OpenEditorAddMountFileBrowser => {
            jackin_console::tui::file_browser::start_editor_add_mount_file_browser_open(state)
        }
        ManagerEffect::OpenGlobalMountFileBrowser => {
            jackin_console::tui::file_browser::start_global_mount_file_browser_open(state)
        }
        ManagerEffect::OpenSettingsAuthSourceFolderBrowser => {
            jackin_console::tui::file_browser::start_settings_auth_source_folder_browser_open(state)
        }
        ManagerEffect::ApplyFileBrowserOutcome { context, outcome } => {
            jackin_console::tui::file_browser::execute_file_browser_outcome_or_start_listing(
                state,
                context,
                outcome,
                crate::console::validate_auth_source_folder,
            )
        }
        ManagerEffect::ResolveFileBrowserGitUrl(path) => {
            jackin_console::tui::file_browser::execute_file_browser_git_url_resolution(state, &path)
        }
        ManagerEffect::PollFileBrowserGitUrls => {
            jackin_console::tui::file_browser::poll_file_browser_git_urls(state)
        }
        ManagerEffect::PollPickerLoads => poll_picker_loads(state),
        ManagerEffect::CopyContainerInfoValue { row, payload } => {
            execute_container_info_copy(state, row, &payload)
        }
        ManagerEffect::OpenUrl(url) => execute_open_url(state, &url),
        ManagerEffect::RemoveWorkspace { name, cwd } => {
            execute_remove_workspace(state, config, paths, &cwd, &name)
        }
        ManagerEffect::ValidateOpCommit {
            op_ref,
            is_settings,
        } => {
            execute_op_commit_validation(state, op_ref, is_settings);
            true
        }
    }
}

fn execute_container_info_copy(state: &mut ManagerState<'_>, row: usize, payload: &str) -> bool {
    let mut out = std::io::stdout();
    let copied = std::io::Write::write_all(
        &mut out,
        &jackin_tui::ansi::encode_osc52_clipboard_write(payload),
    )
    .and_then(|()| std::io::Write::flush(&mut out))
    .is_ok();
    if !copied {
        return false;
    }
    let Some(Modal::ContainerInfo { state: info }) = state.list_modal.as_mut() else {
        return false;
    };
    info.mark_copied(row);
    true
}

pub fn execute_pending_workspace_save_commit(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
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
        jackin_console::tui::input::save::commit_editor_save(state, config, plan, exit_on_success)?
    {
        execute_workspace_save_effect(state, config, paths, cwd, effect);
    }
    Ok(true)
}

pub(crate) fn execute_remove_workspace(
    state: &mut ManagerState<'_>,
    _config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
    cwd: &std::path::Path,
    name: &str,
) -> bool {
    let rx = crate::console::services::config::start_remove_workspace(
        paths.clone(),
        cwd.to_path_buf(),
        name.to_owned(),
    );
    state.begin_config_save(rx);
    true
}

fn apply_remove_workspace_result(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    cwd: std::path::PathBuf,
    result: anyhow::Result<AppConfig>,
) {
    match result {
        Ok(saved) => {
            *config = saved;
            let _unused = update_manager(
                state,
                ManagerMessage::ReloadFromConfig {
                    config: Box::new(config.clone()),
                    cwd,
                },
            );
        }
        Err(error) => {
            let _unused = update_manager(
                state,
                ManagerMessage::OpenListErrorPopup {
                    title: error_popup::delete_failed_error_title().into(),
                    message: format!("{error:#}"),
                },
            );
        }
    }
}

pub(crate) fn apply_role_load_completion(
    state: &mut ManagerState<'_>,
    _config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
    load: PendingRoleLoad,
    result: anyhow::Result<()>,
) {
    match result {
        Ok(()) => {
            let rx = crate::console::services::config::start_role_source_persist(
                paths.clone(),
                jackin_console::tui::subscriptions::RoleSourcePersistOrigin::RoleLoad {
                    raw: load.raw,
                    key: load.key,
                    source: load.source,
                },
            );
            state.begin_config_save(rx);
        }
        Err(e) => {
            let ManagerStage::Editor(editor) = &mut state.stage else {
                return;
            };
            crate::debug_log!(
                "role",
                "role loader failed for key={key:?} raw={raw:?}: {e:?}",
                key = load.key,
                raw = load.raw
            );
            let err_text = e.to_string();
            if let Some(panic_message) = err_text.strip_prefix("role loader panicked: ") {
                crate::console::tui::state::open_role_input_error(
                    editor,
                    &format!(
                        "Could not load role {:?}.\n\nThe role loader hit an internal \
                         error while registering the repository.\n\n{panic_message}",
                        load.raw
                    ),
                );
                return;
            }
            open_role_resolution_error(editor, &load.raw, Some(&load.source.git), &e);
        }
    }
}

fn apply_role_source_persist_result(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    result: anyhow::Result<AppConfig>,
    origin: jackin_console::tui::subscriptions::RoleSourcePersistOrigin<jackin_config::RoleSource>,
) {
    match origin {
        jackin_console::tui::subscriptions::RoleSourcePersistOrigin::RoleLoad {
            raw,
            key,
            source,
        } => match result {
            Ok(saved) => {
                *config = saved;
                let ManagerStage::Editor(editor) = &mut state.stage else {
                    return;
                };
                crate::debug_log!(
                    "role",
                    "role repo registration completed for key={key:?} git={git:?}",
                    git = source.git.as_str()
                );
                if source.trusted {
                    crate::debug_log!(
                        "role",
                        "role source is trusted; adding key={key:?} directly to the workspace"
                    );
                    crate::console::tui::state::add_role_to_workspace_editor(editor, config, &key);
                } else {
                    crate::debug_log!(
                        "role",
                        "role source registered untrusted; opening trust confirm for key={key:?} git={git:?}",
                        git = source.git.as_str()
                    );
                    crate::console::tui::state::open_role_trust_confirm(editor, key, source);
                }
            }
            Err(e) => {
                let ManagerStage::Editor(editor) = &mut state.stage else {
                    return;
                };
                crate::debug_log!(
                    "role",
                    "role loader failed for key={key:?} raw={raw:?}: {e:?}"
                );
                open_role_resolution_error(
                    editor,
                    &raw,
                    Some(&source.git),
                    &e.context("role repository loaded, but registration could not be persisted"),
                );
            }
        },
        jackin_console::tui::subscriptions::RoleSourcePersistOrigin::TrustConfirm {
            key,
            source: _,
        } => match result {
            Ok(saved) => {
                *config = saved;
                let ManagerStage::Editor(editor) = &mut state.stage else {
                    return;
                };
                crate::console::tui::state::add_role_to_workspace_editor(editor, config, &key);
            }
            Err(error) => {
                let ManagerStage::Editor(editor) = &mut state.stage else {
                    return;
                };
                crate::console::tui::state::open_editor_action_error(editor, &error);
            }
        },
    }
}

#[cfg(test)]
pub(crate) fn apply_role_load_completion_for_tests(
    editor: &mut EditorState<'_>,
    config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
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
                open_role_resolution_error(
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
                crate::console::tui::state::add_role_to_workspace_editor(editor, config, &load.key);
            } else {
                crate::debug_log!(
                    "role",
                    "role source registered untrusted; opening trust confirm for key={key:?} git={git:?}",
                    key = load.key,
                    git = load.source.git.as_str()
                );
                crate::console::tui::state::open_role_trust_confirm(editor, load.key, load.source);
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
                crate::console::tui::state::open_role_input_error(
                    editor,
                    &format!(
                        "Could not load role {:?}.\n\nThe role loader hit an internal \
                         error while registering the repository.\n\n{panic_message}",
                        load.raw
                    ),
                );
                return;
            }
            open_role_resolution_error(editor, &load.raw, Some(&load.source.git), &e);
        }
    }
}

#[cfg(test)]
pub(crate) async fn apply_role_input_with_runner_for_tests(
    editor: &mut EditorState<'_>,
    config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
    value: &str,
    runner: &mut impl jackin_docker::CommandRunner,
) {
    let raw = value.trim();
    crate::debug_log!("role", "resolving role loader input: raw={raw:?}");
    let resolved = match crate::console::resolve_role_input_source(config, raw) {
        Ok(resolved) => resolved,
        Err(error) => {
            if let Some(git) = error.source_url.as_ref() {
                open_role_resolution_error(editor, &error.raw, Some(git), &error.error);
            } else {
                open_role_resolution_error(editor, &error.raw, None, &error.error);
            }
            return;
        }
    };

    crate::debug_log!(
        "role",
        "registering role repo for key={key:?} git={git:?}",
        key = resolved.key,
        git = resolved.source.git.as_str()
    );
    let result = crate::console::services::role_load::register_with_runner(
        paths,
        &resolved.selector,
        &resolved.source.git,
        runner,
        crate::tui::is_debug_mode(),
    )
    .await;
    let (_tx, rx) = tokio::sync::oneshot::channel();
    apply_role_load_completion_for_tests(
        editor,
        config,
        paths,
        PendingRoleLoad {
            raw: resolved.raw,
            key: resolved.key,
            source: resolved.source,
            rx,
        },
        result,
    );
}

#[cfg(test)]
pub(crate) fn persist_trusted_role_source_for_tests(
    editor: &mut EditorState<'_>,
    config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
    key: &str,
    source: &jackin_config::RoleSource,
) {
    match execute_role_source_persist(config, paths, key, source) {
        Ok(()) => {
            crate::console::tui::state::add_role_to_workspace_editor(editor, config, key);
        }
        Err(error) => {
            crate::console::tui::state::open_editor_action_error(editor, &error);
        }
    }
}

fn execute_trusted_role_source_persist(
    state: &mut ManagerState<'_>,
    _config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
    key: &str,
    mut source: jackin_config::RoleSource,
) {
    let ManagerStage::Editor(_) = &mut state.stage else {
        return;
    };
    source.trusted = true;
    let rx = crate::console::services::config::start_role_source_persist(
        paths.clone(),
        jackin_console::tui::subscriptions::RoleSourcePersistOrigin::TrustConfirm {
            key: key.to_owned(),
            source,
        },
    );
    state.begin_config_save(rx);
}

pub(crate) fn execute_token_generate(
    paths: &jackin_core::JackinPaths,
    config: &AppConfig,
    req: &crate::console::tui::state::PendingTokenGenerate,
) -> anyhow::Result<jackin_core::EnvValue> {
    jackin_env::mint_token_value(paths, config, &req.scope, &req.args)
}

pub(crate) use jackin_console::tui::state::update::{
    apply_token_generate_result, execute_open_url,
};

fn execute_role_registration_start(
    state: &mut ManagerState<'_>,
    paths: &jackin_core::JackinPaths,
    raw: String,
    key: &str,
    selector: jackin_core::RoleSelector,
    source: jackin_config::RoleSource,
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
        editor.pending_role_load = Some(PendingRoleLoad {
            raw,
            key: key.to_owned(),
            source,
            rx,
        });
        editor.modal = Some(Modal::StatusPopup {
            state: status_popup::role_loading_status_popup_state(key),
        });
    }
}

use jackin_console::tui::state::update::execute_op_commit_validation;

pub(crate) fn execute_workspace_save_effect(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
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
                state: status_popup::workspace_save_drift_check_status_popup_state(),
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
                state: status_popup::workspace_save_isolation_cleanup_status_popup_state(),
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
    _config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
    _cwd: &std::path::Path,
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
    let rx = crate::console::services::config::start_workspace_save(
        paths.clone(),
        mode,
        input.original.clone(),
        input.pending.clone(),
        exit_on_success,
    );
    state.begin_config_save(rx);
}

fn apply_workspace_save_write_result(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    cwd: &std::path::Path,
    result: anyhow::Result<jackin_console::tui::subscriptions::WorkspaceSaveResult<AppConfig>>,
    exit_on_success: bool,
) {
    match result {
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
                editor.save_flow = EditorSaveFlow::Idle;
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
                let _unused = update_manager(
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
                    state.selected = saved_workspace_selected_index(saved_count, idx);
                }
            }
        }
        Err(e) => {
            if let ManagerStage::Editor(editor) = &mut state.stage {
                jackin_console::tui::input::save::open_save_error_popup(editor, &e.to_string());
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn execute_role_source_persist(
    config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
    key: &str,
    source: &jackin_config::RoleSource,
) -> anyhow::Result<()> {
    crate::console::services::config::upsert_role_source(config, paths, key, source)
}

fn execute_settings_save(
    state: &mut ManagerState<'_>,
    _config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let mounts_save = settings.mounts.save_refs();
    let env_save = settings.env.save_refs();
    let auth_save = settings.auth.save_refs();
    let trust_save = settings.trust.save_refs();
    let general_save = settings.general.save_refs();
    let rx = crate::console::services::config::start_settings_save(
        paths.clone(),
        crate::console::services::config::OwnedSettingsSaveInput {
            mounts_original: mounts_save.original.to_vec(),
            mounts_pending: mounts_save.pending.to_vec(),
            env_original: env_save.original.clone(),
            env_pending: env_save.pending.clone(),
            auth_pending: auth_save.pending.to_vec(),
            original_github_env: auth_save.original_github_env.clone(),
            github_env: auth_save.github_env.clone(),
            trust_pending: trust_save.pending.to_vec(),
            git_coauthor_trailer: general_save.git_coauthor_trailer,
            git_dco: general_save.git_dco,
        },
    );
    state.begin_config_save(rx);
}

fn apply_settings_save_result(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    result: anyhow::Result<AppConfig>,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    match result {
        Ok(saved) => {
            *config = saved;
            settings.mark_saved();
            settings.mounts.exit_requested = true;
        }
        Err(err) => settings.mounts.error = Some(err.to_string()),
    }
}

pub fn poll_background_messages(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
) -> Vec<ManagerBackgroundEvent> {
    let mut messages = vec![
        ManagerBackgroundEvent::Message(ManagerMessage::PollFileBrowserGitUrls),
        ManagerBackgroundEvent::Message(ManagerMessage::PollPickerLoads),
    ];
    if let Some((load, result)) = state.poll_pending_role_load() {
        messages.push(ManagerBackgroundEvent::RoleLoadFinished { load, result });
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
    if let Some(result) = state.poll_file_browser_listing() {
        messages.push(ManagerBackgroundEvent::Message(
            ManagerMessage::FileBrowserListingLoaded(result),
        ));
    }
    if let Some(result) = state.poll_file_browser_commit() {
        messages.push(ManagerBackgroundEvent::Message(
            ManagerMessage::FileBrowserCommitValidated(result),
        ));
    }
    if let Some(result) = state.poll_instance_refresh() {
        messages.push(ManagerBackgroundEvent::Message(
            ManagerMessage::InstancesRefreshed(result),
        ));
    }
    execute_manager_effect(
        state,
        config,
        paths,
        ConsoleEffect::RequestInstanceRefresh.into(),
    );
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
    if let Some(result) = state.poll_config_save() {
        messages.push(ManagerBackgroundEvent::ConfigSaveFinished(result));
    }
    messages
}

pub fn apply_background_event(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &jackin_core::JackinPaths,
    cwd: &std::path::Path,
    event: ManagerBackgroundEvent,
) -> bool {
    match event {
        ManagerBackgroundEvent::Message(message) => {
            let mut dirty = update_manager(state, message).is_dirty();
            for effect in state.drain_effects() {
                dirty |= execute_manager_effect(state, config, paths, effect);
            }
            dirty
        }
        ManagerBackgroundEvent::RoleLoadFinished { load, result } => {
            apply_role_load_completion(state, config, paths, load, result);
            true
        }
        ManagerBackgroundEvent::DriftCheckFinished { check, detection } => {
            if let Ok(Some(effect)) =
                jackin_console::tui::input::save::continue_save_after_drift_check(
                    state, config, check, detection,
                )
            {
                execute_workspace_save_effect(state, config, paths, cwd, effect);
            }
            true
        }
        ManagerBackgroundEvent::IsolationCleanupFinished { cleanup, result } => {
            if let Ok(Some(effect)) =
                jackin_console::tui::input::save::continue_save_after_isolation_cleanup(
                    state, config, cleanup, result,
                )
            {
                execute_workspace_save_effect(state, config, paths, cwd, effect);
            }
            true
        }
        ManagerBackgroundEvent::ConfigSaveFinished(result) => {
            match result {
                jackin_console::tui::subscriptions::ConfigSaveResult::Workspace {
                    result,
                    exit_on_success,
                } => {
                    apply_workspace_save_write_result(state, config, cwd, result, exit_on_success);
                }
                jackin_console::tui::subscriptions::ConfigSaveResult::Settings(result) => {
                    apply_settings_save_result(state, config, result);
                }
                jackin_console::tui::subscriptions::ConfigSaveResult::RemoveWorkspace {
                    result,
                    cwd,
                } => {
                    apply_remove_workspace_result(state, config, cwd, result);
                }
                jackin_console::tui::subscriptions::ConfigSaveResult::RoleSourcePersist {
                    result,
                    origin,
                } => {
                    apply_role_source_persist_result(state, config, result, origin);
                }
            }
            true
        }
    }
}

/// Drained from the outer event loop every tick so picker results land without
pub(crate) use jackin_console::tui::op_picker::poll_picker_loads;

// ── Role-resolution error helpers ───────────────────────────────────────────
//
// These depend on `crate::runtime::RepoError` and `jackin_manifest::repo` types that
// are not accessible from `jackin-console`. All callers live in this file.

pub(crate) fn open_role_resolution_error(
    editor: &mut EditorState<'_>,
    raw: &str,
    source_url: Option<&String>,
    err: &anyhow::Error,
) {
    use jackin_console::tui::components::error_popup::{
        configured_role_load_error_message, repository_role_load_error_message,
    };
    crate::debug_log!(
        "role",
        "showing role-load error popup for raw={raw:?}: {err:?}"
    );
    let message = source_url.map_or_else(
        || configured_role_load_error_message(raw),
        |source_url| {
            repository_role_load_error_message(raw, source_url, friendly_role_resolution_error(err))
        },
    );
    editor.open_error_popup(error_popup::role_load_error_popup_state(message));
}

fn friendly_role_resolution_error(err: &anyhow::Error) -> String {
    use jackin_console::tui::components::error_popup::{
        generic_role_repository_error_message, invalid_role_repository_message,
        role_repository_remote_mismatch_message, role_repository_unavailable_message,
    };

    if let Some(repo_err) = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<crate::runtime::RepoError>())
    {
        return match repo_err {
            crate::runtime::RepoError::CloneFailed(_) => {
                role_repository_unavailable_message().into()
            }
            crate::runtime::RepoError::RemoteMismatch => {
                role_repository_remote_mismatch_message().into()
            }
            crate::runtime::RepoError::InvalidRoleRepo(detail) => {
                invalid_role_repository_message(humanize_invalid_role_repo(detail))
            }
        };
    }
    generic_role_repository_error_message().into()
}

fn humanize_invalid_role_repo(err: &jackin_manifest::repo::RoleRepoValidationError) -> String {
    use jackin_manifest::repo::RoleRepoValidationError as V;
    match err {
        V::Missing(path) => {
            let file = path
                .file_name()
                .and_then(|name| name.to_str())
                .map_or_else(|| path.display().to_string(), str::to_owned);
            error_popup::missing_role_repository_file_message(file)
        }
        _ => err.to_string().trim_end_matches('.').to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use jackin_config::AppConfig;
    use jackin_config::WorkspaceConfig;
    use jackin_console::tui::auth::{AuthKind, AuthMode};
    use jackin_console::tui::components::file_browser::{
        FileBrowserOutcome, FileBrowserState, FolderListing,
    };
    use jackin_console::tui::effect::FileBrowserEffectContext;
    use jackin_console::tui::effect::{ConsoleEffect, WorkspaceSaveEffect};
    use jackin_console::tui::state::update::{ManagerBackgroundEvent, ManagerMessage};

    use crate::console::tui::state::{
        AuthForm, AuthFormFocus, AuthFormTarget, CreatePreludeState, EditorState,
        FileBrowserTarget, ManagerEffect, ManagerStage, ManagerState, Modal, PendingRoleLoad,
        SettingsAuthModal, SettingsState,
    };
    use crate::console::tui::{WorkspaceSaveWriteInput, WorkspaceSaveWriteMode};

    use super::{
        apply_role_load_completion, execute_manager_effect, execute_workspace_save_effect,
        execute_workspace_save_write, poll_background_messages,
    };

    #[tokio::test]
    async fn poll_background_messages_routes_file_browser_poll_through_message() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);

        let events = poll_background_messages(&mut state, &mut config, &paths);

        assert!(events.iter().any(|event| matches!(
            event,
            ManagerBackgroundEvent::Message(ManagerMessage::PollFileBrowserGitUrls)
        )));
    }

    #[tokio::test]
    async fn execute_manager_effect_requests_instance_refresh() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);

        execute_manager_effect(
            &mut state,
            &mut config,
            &paths,
            ConsoleEffect::RequestInstanceRefresh.into(),
        );

        assert!(
            state.instance_refresh_in_flight(),
            "instance refresh effect should spawn a worker"
        );
    }

    #[tokio::test]
    async fn workspace_save_drift_check_starts_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let editor = EditorState::new_edit("workspace".into(), WorkspaceConfig::default());
        let mut state = ManagerState::from_config(&config, cwd);
        state.stage = ManagerStage::Editor(editor);

        execute_workspace_save_effect(
            &mut state,
            &mut config,
            &paths,
            cwd,
            WorkspaceSaveEffect::StartDriftCheck {
                original_name: "workspace".into(),
                prospective_mounts: Vec::new(),
                plan: jackin_console::tui::state::PendingSaveCommit {
                    effective_removals: Vec::new(),
                    final_mounts: None,
                    delete_isolated_acknowledged: false,
                    isolated_cleanup_complete: false,
                },
                exit_on_success: false,
            },
        );

        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("expected editor stage");
        };
        assert!(
            editor.pending_drift_check.is_some(),
            "workspace save drift detection should run on a worker"
        );
        assert!(matches!(editor.modal, Some(Modal::StatusPopup { .. })));
    }

    #[tokio::test]
    async fn workspace_save_write_starts_config_save_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let original = WorkspaceConfig::default();
        let pending = WorkspaceConfig::default();
        let editor = EditorState::new_edit("workspace".into(), original.clone());
        let mut state = ManagerState::from_config(&config, cwd);
        state.stage = ManagerStage::Editor(editor);

        execute_workspace_save_write(
            &mut state,
            &mut config,
            &paths,
            cwd,
            WorkspaceSaveWriteInput {
                mode: WorkspaceSaveWriteMode::Edit {
                    original_name: "workspace".into(),
                    pending_name: None,
                    effective_removals: Vec::new(),
                },
                original: &original,
                pending: &pending,
            },
            false,
        );

        assert!(
            state.config_save_in_flight(),
            "workspace config save should run on a worker"
        );
    }

    #[tokio::test]
    async fn settings_save_starts_config_save_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let settings = SettingsState::from_config(&config);
        let mut state = ManagerState::from_config(&config, cwd);
        state.stage = ManagerStage::Settings(settings);

        execute_manager_effect(
            &mut state,
            &mut config,
            &paths,
            ConsoleEffect::SaveSettings.into(),
        );

        assert!(
            state.config_save_in_flight(),
            "settings config save should run on a worker"
        );
    }

    #[tokio::test]
    async fn remove_workspace_starts_config_save_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);

        super::execute_remove_workspace(&mut state, &mut config, &paths, cwd, "workspace");

        assert!(
            state.config_save_in_flight(),
            "workspace delete should run on a worker"
        );
    }

    #[tokio::test]
    async fn trusted_role_source_persist_starts_config_save_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let editor = EditorState::new_edit("workspace".into(), WorkspaceConfig::default());
        let mut state = ManagerState::from_config(&config, cwd);
        state.stage = ManagerStage::Editor(editor);

        execute_manager_effect(
            &mut state,
            &mut config,
            &paths,
            ManagerEffect::PersistTrustedRoleSource {
                key: "agent-smith".into(),
                source: jackin_config::RoleSource::default(),
            },
        );

        assert!(
            state.config_save_in_flight(),
            "trusted role source persistence should run on a worker"
        );
    }

    #[tokio::test]
    async fn role_load_completion_starts_role_source_persist_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let editor = EditorState::new_edit("workspace".into(), WorkspaceConfig::default());
        let mut state = ManagerState::from_config(&config, cwd);
        state.stage = ManagerStage::Editor(editor);
        let (_tx, rx) = tokio::sync::oneshot::channel();

        apply_role_load_completion(
            &mut state,
            &mut config,
            &paths,
            PendingRoleLoad {
                raw: "agent-smith".into(),
                key: "agent-smith".into(),
                source: jackin_config::RoleSource::default(),
                rx,
            },
            Ok(()),
        );

        assert!(
            state.config_save_in_flight(),
            "loaded role source persistence should run on a worker"
        );
    }

    #[tokio::test]
    async fn create_prelude_file_browser_open_starts_listing_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);

        execute_manager_effect(
            &mut state,
            &mut config,
            &paths,
            ManagerEffect::OpenCreatePreludeFileBrowser,
        );

        assert!(
            state.file_browser_listing_in_flight(),
            "file browser open should scan directories on a worker"
        );
        assert!(
            matches!(state.stage, ManagerStage::List),
            "the modal should open only after the worker returns"
        );
    }

    #[tokio::test]
    async fn file_browser_navigation_starts_listing_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        let listing = FolderListing {
            root: cwd.to_path_buf(),
            cwd: cwd.to_path_buf(),
            entries: Vec::new(),
        };
        let mut prelude = CreatePreludeState::new();
        prelude.modal = Some(Modal::FileBrowser {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: FileBrowserState::from_listing(listing),
        });
        state.stage = ManagerStage::CreatePrelude(prelude);

        execute_manager_effect(
            &mut state,
            &mut config,
            &paths,
            ManagerEffect::ApplyFileBrowserOutcome {
                context: FileBrowserEffectContext::Prelude { browser_cwd: None },
                outcome: FileBrowserOutcome::NavigateTo(cwd.join("child")),
            },
        );

        assert!(
            state.file_browser_listing_in_flight(),
            "file browser navigation should scan directories on a worker"
        );
    }

    #[tokio::test]
    async fn file_browser_commit_starts_validation_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let child = cwd.join("child");
        std::fs::create_dir(&child).unwrap();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        let listing = FolderListing {
            root: cwd.to_path_buf(),
            cwd: cwd.to_path_buf(),
            entries: Vec::new(),
        };
        let mut prelude = CreatePreludeState::new();
        prelude.modal = Some(Modal::FileBrowser {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: FileBrowserState::from_listing(listing),
        });
        state.stage = ManagerStage::CreatePrelude(prelude);

        execute_manager_effect(
            &mut state,
            &mut config,
            &paths,
            ManagerEffect::ApplyFileBrowserOutcome {
                context: FileBrowserEffectContext::Prelude { browser_cwd: None },
                outcome: FileBrowserOutcome::RequestCommit(child),
            },
        );

        assert!(
            state.file_browser_commit_in_flight(),
            "file browser commit validation should run on a worker"
        );
    }

    #[tokio::test]
    async fn editor_auth_source_folder_open_starts_listing_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut editor = EditorState::new_edit("workspace".into(), WorkspaceConfig::default());
        let mut form = AuthForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::Sync);
        form.set_source_folder(cwd.to_path_buf());
        editor.modal = Some(Modal::AuthForm {
            target: AuthFormTarget::Workspace {
                kind: AuthKind::Claude,
            },
            state: Box::new(form),
            focus: AuthFormFocus::SourceFolder,
            literal_buffer: String::new(),
        });
        state.stage = ManagerStage::Editor(editor);

        execute_manager_effect(
            &mut state,
            &mut config,
            &paths,
            ManagerEffect::OpenEditorAuthSourceFolderBrowser,
        );

        assert!(state.file_browser_listing_in_flight());
        let ManagerStage::Editor(editor) = &state.stage else {
            panic!("expected editor stage");
        };
        assert!(matches!(editor.modal, Some(Modal::AuthForm { .. })));
    }

    #[tokio::test]
    async fn settings_auth_source_folder_open_starts_listing_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = jackin_core::JackinPaths::for_tests(tmp.path());
        let cwd = tmp.path();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, cwd);
        let mut settings = SettingsState::from_config(&config);
        let mut form = AuthForm::new(AuthKind::Claude);
        form.set_mode(AuthMode::Sync);
        form.set_source_folder(cwd.to_path_buf());
        settings.auth.modal = Some(SettingsAuthModal::AuthForm {
            target: AuthFormTarget::Workspace {
                kind: AuthKind::Claude,
            },
            state: Box::new(form),
            focus: AuthFormFocus::SourceFolder,
            literal_buffer: String::new(),
        });
        state.stage = ManagerStage::Settings(settings);

        execute_manager_effect(
            &mut state,
            &mut config,
            &paths,
            ManagerEffect::OpenSettingsAuthSourceFolderBrowser,
        );

        assert!(state.file_browser_listing_in_flight());
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(matches!(
            settings.auth.modal,
            Some(SettingsAuthModal::AuthForm { .. })
        ));
    }
}
