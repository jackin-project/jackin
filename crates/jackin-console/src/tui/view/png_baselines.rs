// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! PNG baselines over the complete console screen inventory: 16 stage and
//! overlay views, five account-management cases, four create-prelude wizard
//! steps, and all 18 `ConsoleModal` variants — 43 full frames total.
//! The inventory guard requires that exact count. Brand-header crops derive
//! their separate non-modal inventory from the same cases.
//!
//! Compare mode (default) is zero-tolerance on decoded pixels and NEVER
//! writes; bless mode (`JACKIN_BLESS_PNGS=1`) rewrites every baseline from an
//! actual render and is the only write path. Plans 006–013 run compare only;
//! re-bless is sanctioned in plan 005 (initial) and plan 014 (reviewed).

#![cfg(test)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

use crate::tui::{
    state::{EditorState, EditorTab, ManagerStage, ManagerState, Modal, SettingsState},
    view::{prepare_for_render, render},
};
use jackin_config::{AppConfig, WorkspaceConfig};
use termrock::style::RolePalette;

/// One baselined screen: stable kebab-case id plus a headless constructor for
/// its canonical state.
pub(super) struct BaselineCase {
    pub(super) id: &'static str,
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) build: fn() -> (ManagerState<'static>, AppConfig, PathBuf),
}

fn test_cwd() -> PathBuf {
    PathBuf::from("/workspace")
}

fn render_case(case: &BaselineCase) -> Vec<u8> {
    let (mut state, config, cwd) = (case.build)();
    let buffer = render_manager_buffer(&mut state, &config, &cwd, case.width, case.height);
    termrock_raster::render_png(&buffer, &RolePalette::default())
        .expect("baselined screen must rasterize")
}

pub(super) fn render_manager_buffer(
    state: &mut ManagerState<'_>,
    config: &AppConfig,
    cwd: &Path,
    width: u16,
    height: u16,
) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    prepare_for_render(state, config, cwd, area);
    terminal
        .draw(|frame| render(frame, area, state, config, cwd))
        .unwrap();
    terminal.backend().buffer().clone()
}

// ── Stage-view constructors ────────────────────────────────────────────────

fn plain() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let state = ManagerState::from_config(&config, &cwd);
    (state, config, cwd)
}

fn populated_config() -> AppConfig {
    toml::from_str(
        r#"
[roles."chainargos/agent-smith"]
git = "https://example.invalid/agent-smith.git"

[docker.mounts]
cache = { src = "/cache", dst = "/cache", readonly = false }

[workspaces.alpha]
workdir = "/workspace"
allowed_roles = ["chainargos/agent-smith"]

[[workspaces.alpha.mounts]]
src = "/workspace"
dst = "/workspace"
readonly = false

[workspaces.beta]
workdir = "/beta"
"#,
    )
    .expect("valid populated-list config")
}

fn workspaces_list_empty() -> (ManagerState<'static>, AppConfig, PathBuf) {
    plain()
}

fn workspaces_list_populated() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let state = ManagerState::from_config(&config, &cwd);
    (state, config, cwd)
}

fn editor_with_tab(tab: EditorTab) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_tab = tab;
    state.stage = ManagerStage::Editor(editor);
    (state, config, cwd)
}

fn editor_general() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::General)
}

fn editor_mounts() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::Mounts)
}

fn editor_roles() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::Roles)
}

fn editor_secrets() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::Secrets)
}

fn editor_auth() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::Auth)
}

fn settings_with_tab(
    tab: crate::tui::state::SettingsTab,
) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = tab;
    state.stage = ManagerStage::Settings(settings);
    (state, config, cwd)
}

fn settings_general() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::General)
}

fn settings_mounts() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::Mounts)
}

fn settings_environments() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::Environments)
}

fn settings_auth() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::Auth)
}

fn settings_trust() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::Trust)
}

fn create_prelude() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = plain();
    state.stage = ManagerStage::CreatePrelude(crate::tui::state::CreatePreludeState::default());
    (state, config, cwd)
}

fn confirm_delete() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = populated_then();
    state.stage = ManagerStage::ConfirmDelete {
        name: "alpha".to_owned(),
        state: crate::tui::components::ConfirmState::new("Delete workspace?"),
    };
    (state, config, cwd)
}

fn populated_then() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let state = ManagerState::from_config(&config, &cwd);
    (state, config, cwd)
}

fn confirm_instance_purge() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = populated_then();
    state.stage = ManagerStage::ConfirmInstancePurge {
        container: "jackin-alpha".to_owned(),
        label: "alpha".to_owned(),
        state: crate::tui::components::ConfirmState::new(
            "Purge instance?\nThis removes the container and its state.",
        ),
    };
    (state, config, cwd)
}

fn keyboard_help() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = populated_then();
    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    (state, config, cwd)
}

// ── Modal constructors (all 18 `ConsoleModal` variants) ────────────────────

fn with_list_modal(modal: Modal<'static>) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.list_modal = Some(modal);
    (state, config, cwd)
}

fn with_editor_modal(modal: Modal<'static>) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.modal = Some(modal);
    state.stage = ManagerStage::Editor(editor);
    (state, config, cwd)
}

fn role_picker_state() -> crate::tui::state::RolePickerState {
    crate::tui::state::RolePickerState::new(vec![
        jackin_core::RoleSelector::parse("chainargos/agent-smith").expect("valid role selector"),
    ])
}

fn modal_text_input() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::TextInput {
        target: crate::tui::state::TextInputTarget::Name,
        state: crate::tui::components::TextInputState::new("Name", "alpha"),
    })
}

fn modal_file_browser() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let cwd = test_cwd();
    with_list_modal(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
        state: crate::tui::components::file_browser::FileBrowserState::from_listing(
            crate::services::file_browser::listing_at(cwd.clone(), cwd),
        ),
    })
}

fn modal_mount_dst_choice() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::MountDstChoice {
        target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
        state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new("/workspace"),
    })
}

fn modal_workdir_pick() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::WorkdirPick {
        state: crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(&[
            jackin_config::MountConfig {
                src: "/workspace".into(),
                dst: "/workspace".into(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            },
        ]),
    })
}

fn modal_confirm() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::Confirm {
        target: crate::tui::state::ConfirmTarget::DeleteEnvVar {
            scope: crate::tui::state::SecretsScopeTag::Workspace,
            key: "TOKEN".into(),
        },
        state: crate::tui::components::ConfirmState::new("Delete TOKEN?"),
    })
}

fn modal_save_discard_cancel() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::SaveDiscardCancel {
        state: crate::tui::components::SaveDiscardState::new("Save changes?"),
    })
}

fn modal_github_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(vec![
            crate::github_mounts::GithubChoice {
                src: "/workspace".into(),
                branch: "main".into(),
                url: "https://github.com/example/repo".into(),
            },
        ]),
    })
}

fn modal_confirm_save() -> (ManagerState<'static>, AppConfig, PathBuf) {
    use ratatui::text::Line;
    with_list_modal(
        Modal::ConfirmSave {
            state: crate::tui::components::confirm_save::ConfirmSaveState::<
                jackin_config::MountConfig,
            >::new(vec![
                Line::from("Create workspace: alpha"),
                Line::from(""),
                Line::from("Working directory: /workspace"),
            ]),
        },
    )
}

fn modal_error_popup() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::ErrorPopup {
        state: crate::tui::components::ErrorPopupState::new("Token mint failed", "op item missing"),
    })
}

fn modal_container_info() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::ContainerInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState::new(
            "Container",
            vec![
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Run ID", "abc",
                ),
            ],
        ),
    })
}

fn modal_status_popup() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::StatusPopup {
        state: crate::tui::components::StatusPopupState::new("Loading", "Resolving role"),
    })
}

fn modal_op_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::OpPicker {
        secrets_target: None,
        state: Box::new(crate::tui::op_picker::OpPickerState::new()),
    })
}

fn modal_role_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::RolePicker {
        state: role_picker_state(),
    })
}

fn modal_role_override_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::RoleOverridePicker {
        state: role_picker_state(),
    })
}

fn modal_source_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::SourcePicker {
        state: crate::tui::components::source_picker::SourcePickerState::new("TOKEN".into(), true),
        env_key: None,
    })
}

fn modal_auth_source_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::AuthSourcePicker {
        state: crate::tui::components::source_picker::SourcePickerState::new(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            true,
        ),
    })
}

fn modal_scope_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::ScopePicker {
        state: crate::tui::components::scope_picker::ScopePickerState::new(),
    })
}

fn modal_auth_form() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let kind = crate::tui::auth::AuthKind::Claude;
    with_editor_modal(Modal::AuthForm {
        target: crate::tui::state::AuthFormTarget::Workspace { kind },
        state: Box::new(crate::tui::state::AuthForm::new(kind)),
        focus: crate::tui::state::AuthFormFocus::Mode,
        literal_buffer: String::new(),
    })
}

// ── Create-prelude wizard modal steps ──────────────────────────────────────

fn prelude_with_modal(modal: Modal<'static>) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = plain();
    let prelude = crate::tui::state::CreatePreludeState {
        modal: Some(modal),
        ..Default::default()
    };
    state.stage = ManagerStage::CreatePrelude(prelude);
    (state, config, cwd)
}

fn create_prelude_workdir_pick() -> (ManagerState<'static>, AppConfig, PathBuf) {
    prelude_with_modal(Modal::WorkdirPick {
        state: crate::tui::components::workdir_pick::WorkdirPickState::from_mounts::<
            jackin_config::MountConfig,
        >(&[]),
    })
}

fn create_prelude_file_browser() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let cwd = test_cwd();
    prelude_with_modal(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
        state: crate::tui::components::file_browser::FileBrowserState::from_listing(
            crate::services::file_browser::listing_at(cwd.clone(), cwd),
        ),
    })
}

fn create_prelude_mount_dst_choice() -> (ManagerState<'static>, AppConfig, PathBuf) {
    prelude_with_modal(Modal::MountDstChoice {
        target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
        state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new("/workspace"),
    })
}

fn create_prelude_name_input() -> (ManagerState<'static>, AppConfig, PathBuf) {
    prelude_with_modal(Modal::TextInput {
        target: crate::tui::state::TextInputTarget::Name,
        state: crate::tui::components::TextInputState::new("Workspace name", "alpha"),
    })
}

fn account_config() -> AppConfig {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};
    use jackin_core::{Agent, EnvValue};
    let mut config = populated_config();
    for (id, name, credential) in [
        (
            "anthropic-personal",
            "Personal Claude",
            AccountCredential::Profile {
                agent: Agent::Claude,
                directory: "/profiles/claude-personal".into(),
            },
        ),
        (
            "anthropic-work",
            "Work Claude",
            AccountCredential::Profile {
                agent: Agent::Claude,
                directory: "/profiles/claude-work".into(),
            },
        ),
        (
            "anthropic-api",
            "Team API",
            AccountCredential::ApiKey {
                value: EnvValue::Plain("synthetic-review-key".into()),
                base_url: Some("https://api.example.invalid".into()),
                model: Some("team-model".into()),
            },
        ),
    ] {
        config.accounts.insert(
            id.into(),
            AccountConfig {
                enabled: true,
                name: name.into(),
                provider: AiProvider::Anthropic,
                credential,
            },
        );
    }
    config
        .accounts
        .get_mut("anthropic-personal")
        .unwrap()
        .enabled = false;
    config
        .account_bindings
        .insert(Agent::Claude, "anthropic-work".into());
    let workspace = config.workspaces.get_mut("alpha").unwrap();
    workspace.accounts = vec!["anthropic-work".into(), "anthropic-api".into()];
    workspace
        .account_bindings
        .insert(Agent::Claude, "anthropic-work".into());
    config
}

fn settings_accounts_populated() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = account_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = crate::tui::state::SettingsTab::Auth;
    state.stage = ManagerStage::Settings(settings);
    (state, config, cwd)
}

fn workspace_accounts_assigned() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = account_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("alpha".into(), config.workspaces["alpha"].clone());
    editor.active_tab = EditorTab::Auth;
    state.stage = ManagerStage::Editor(editor);
    (state, config, cwd)
}

fn settings_account_form(api: bool) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = settings_accounts_populated();
    let ManagerStage::Settings(settings) = &mut state.stage else {
        unreachable!()
    };
    let kind = crate::tui::auth::AuthKind::Claude;
    let form = if api {
        crate::tui::state::AuthForm::from_existing(
            kind,
            crate::tui::auth::AuthMode::ApiKey,
            Some(jackin_core::EnvValue::Plain("synthetic-review-key".into())),
        )
    } else {
        crate::tui::state::AuthForm::from_existing(kind, crate::tui::auth::AuthMode::Sync, None)
            .with_source_folder(Some("/profiles/claude-work".into()), None)
    };
    settings
        .auth
        .set_modal(crate::tui::state::SettingsModal::AuthForm {
            target: crate::tui::state::AuthFormTarget::Workspace { kind },
            state: Box::new(form),
            focus: crate::tui::state::AuthFormFocus::Save,
            literal_buffer: String::new(),
        });
    (state, config, cwd)
}

fn settings_account_api_form() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_account_form(true)
}
fn settings_account_profile_form() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_account_form(false)
}

fn account_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = populated_then();
    state.inline_account_picker = Some(crate::tui::state::AccountPickerState::new(
        "jackin-alpha".into(),
        jackin_core::Agent::Claude,
        vec![crate::services::launch::AccountChoice {
            id: "anthropic-work".into(),
            name: "Work".into(),
            provider: jackin_config::AiProvider::Anthropic,
            agents: vec![jackin_core::Agent::Claude],
        }],
    ));
    (state, config, cwd)
}

// ── Inventory ──────────────────────────────────────────────────────────────

const LIST: (u16, u16) = (80, 24);
const SCREEN: (u16, u16) = (90, 20);
const MODAL: (u16, u16) = (90, 24);

pub(super) fn inventory() -> Vec<BaselineCase> {
    let mut cases = Vec::new();
    let mut push = |id: &'static str,
                    size: (u16, u16),
                    build: fn() -> (ManagerState<'static>, AppConfig, PathBuf)| {
        cases.push(BaselineCase {
            id,
            width: size.0,
            height: size.1,
            build,
        });
    };

    // Stage views.
    push("workspaces-list-empty", LIST, workspaces_list_empty);
    push("workspaces-list-populated", LIST, workspaces_list_populated);
    push("editor-general", SCREEN, editor_general);
    push("editor-mounts", SCREEN, editor_mounts);
    push("editor-roles", SCREEN, editor_roles);
    push("editor-secrets", SCREEN, editor_secrets);
    push("editor-auth", SCREEN, editor_auth);
    push("settings-general", SCREEN, settings_general);
    push("settings-mounts", SCREEN, settings_mounts);
    push("settings-environments", SCREEN, settings_environments);
    push("settings-auth", SCREEN, settings_auth);
    push("settings-trust", SCREEN, settings_trust);
    push("create-prelude", MODAL, create_prelude);
    push("confirm-delete", MODAL, confirm_delete);
    push("confirm-instance-purge", MODAL, confirm_instance_purge);
    push("keyboard-help", LIST, keyboard_help);
    push("account-picker", MODAL, account_picker);
    push(
        "settings-accounts-populated",
        MODAL,
        settings_accounts_populated,
    );
    push(
        "workspace-accounts-assigned",
        MODAL,
        workspace_accounts_assigned,
    );
    push(
        "settings-account-api-form",
        MODAL,
        settings_account_api_form,
    );
    push(
        "settings-account-profile-form",
        MODAL,
        settings_account_profile_form,
    );

    // Create-prelude wizard modal steps.
    push(
        "create-prelude-workdir-pick",
        MODAL,
        create_prelude_workdir_pick,
    );
    push(
        "create-prelude-file-browser",
        MODAL,
        create_prelude_file_browser,
    );
    push(
        "create-prelude-mount-dst-choice",
        MODAL,
        create_prelude_mount_dst_choice,
    );
    push(
        "create-prelude-name-input",
        MODAL,
        create_prelude_name_input,
    );

    // All 18 ConsoleModal variants.
    push("modal-text-input", MODAL, modal_text_input);
    push("modal-file-browser", MODAL, modal_file_browser);
    push("modal-mount-dst-choice", MODAL, modal_mount_dst_choice);
    push("modal-workdir-pick", MODAL, modal_workdir_pick);
    push("modal-confirm", MODAL, modal_confirm);
    push(
        "modal-save-discard-cancel",
        MODAL,
        modal_save_discard_cancel,
    );
    push("modal-github-picker", MODAL, modal_github_picker);
    push("modal-confirm-save", MODAL, modal_confirm_save);
    push("modal-error-popup", MODAL, modal_error_popup);
    push("modal-container-info", MODAL, modal_container_info);
    push("modal-status-popup", MODAL, modal_status_popup);
    push("modal-op-picker", MODAL, modal_op_picker);
    push("modal-role-picker", MODAL, modal_role_picker);
    push(
        "modal-role-override-picker",
        MODAL,
        modal_role_override_picker,
    );
    push("modal-source-picker", MODAL, modal_source_picker);
    push("modal-auth-source-picker", MODAL, modal_auth_source_picker);
    push("modal-scope-picker", MODAL, modal_scope_picker);
    push("modal-auth-form", MODAL, modal_auth_form);

    cases
}

/// Complete account-era corpus: 16 stage/overlay views, five account cases,
/// four create-prelude wizard steps, and 18 `ConsoleModal` variants.
const EXPECTED_INVENTORY: usize = 43;

pub(super) fn baselines_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/view/baselines/png")
}

fn baseline_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.png"))
}

/// Compare-or-bless one case against `dir`. Returns `Err` with a message
/// naming the screen on missing baseline or pixel drift. Bless mode writes
/// the rendered PNG and returns `Ok`.
fn check_case(case: &BaselineCase, dir: &Path, bless: bool, rendered: &[u8]) -> Result<(), String> {
    let path = baseline_path(dir, case.id);
    if bless {
        fs::write(&path, rendered).map_err(|e| format!("{}: write failed: {e}", case.id))?;
        return Ok(());
    }
    match fs::read(&path) {
        Err(_) => Err(format!(
            "{}: no baseline at {} — bless via `JACKIN_BLESS_PNGS=1` (plan 005/014 only)",
            case.id,
            path.display()
        )),
        Ok(committed) => termrock_raster::compare_png_pixels(rendered, &committed)
            .map_err(|diff| format!("{}: {diff}", case.id)),
    }
}

#[cfg(test)]
mod tests;
