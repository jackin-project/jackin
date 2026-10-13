// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Baseline inventory and comparison.
#![cfg(test)]

use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::tui::state::ManagerState;
use jackin_config::AppConfig;

pub(crate) const LIST: (u16, u16) = (80, 24);
pub(crate) const SCREEN: (u16, u16) = (90, 20);
pub(crate) const MODAL: (u16, u16) = (90, 24);

pub(crate) fn case(
    id: &'static str,
    size: (u16, u16),
    build: fn() -> (ManagerState<'static>, AppConfig, PathBuf),
) -> BaselineCase {
    BaselineCase {
        id,
        width: size.0,
        height: size.1,
        build,
    }
}

pub(crate) fn stage_views() -> Vec<BaselineCase> {
    vec![
        case("workspaces-list-empty", LIST, workspaces_list_empty),
        case("workspaces-list-populated", LIST, workspaces_list_populated),
        case("editor-general", SCREEN, editor_general),
        case("editor-mounts", SCREEN, editor_mounts),
        case("editor-roles", SCREEN, editor_roles),
        case("editor-secrets", SCREEN, editor_secrets),
        case("editor-auth", SCREEN, editor_auth),
        case("settings-general", SCREEN, settings_general),
        case("settings-mounts", SCREEN, settings_mounts),
        case("settings-environments", SCREEN, settings_environments),
        case("settings-auth", SCREEN, settings_auth),
        case("settings-trust", SCREEN, settings_trust),
        case("create-prelude", MODAL, create_prelude),
        case("confirm-delete", MODAL, confirm_delete),
        case("confirm-instance-purge", MODAL, confirm_instance_purge),
        case("keyboard-help", LIST, keyboard_help),
    ]
}

// ── Account cases ──

pub(crate) fn account_cases() -> Vec<BaselineCase> {
    vec![
        case("account-picker", LIST, account_picker),
        case(
            "settings-accounts-populated",
            SCREEN,
            settings_accounts_populated,
        ),
        case(
            "workspace-accounts-assigned",
            SCREEN,
            workspace_accounts_assigned,
        ),
        case(
            "settings-account-api-form",
            MODAL,
            settings_account_api_form,
        ),
        case(
            "settings-account-profile-form",
            MODAL,
            settings_account_profile_form,
        ),
    ]
}

pub(crate) fn create_prelude_wizard_cases() -> Vec<BaselineCase> {
    vec![
        case(
            "create-prelude-workdir-pick",
            MODAL,
            create_prelude_workdir_pick,
        ),
        case(
            "create-prelude-file-browser",
            MODAL,
            create_prelude_file_browser,
        ),
        case(
            "create-prelude-mount-dst-choice",
            MODAL,
            create_prelude_mount_dst_choice,
        ),
        case(
            "create-prelude-name-input",
            MODAL,
            create_prelude_name_input,
        ),
    ]
}

pub(crate) fn modal_cases() -> Vec<BaselineCase> {
    vec![
        case("modal-text-input", MODAL, modal_text_input),
        case("modal-file-browser", MODAL, modal_file_browser),
        case("modal-mount-dst-choice", MODAL, modal_mount_dst_choice),
        case("modal-workdir-pick", MODAL, modal_workdir_pick),
        case("modal-confirm", MODAL, modal_confirm),
        case(
            "modal-save-discard-cancel",
            MODAL,
            modal_save_discard_cancel,
        ),
        case("modal-github-picker", MODAL, modal_github_picker),
        case("modal-confirm-save", MODAL, modal_confirm_save),
        case("modal-error-popup", MODAL, modal_error_popup),
        case("modal-container-info", MODAL, modal_container_info),
        case("modal-status-popup", MODAL, modal_status_popup),
        case("modal-op-picker", MODAL, modal_op_picker),
        case("modal-role-picker", MODAL, modal_role_picker),
        case(
            "modal-role-override-picker",
            MODAL,
            modal_role_override_picker,
        ),
        case("modal-source-picker", MODAL, modal_source_picker),
        case("modal-auth-source-picker", MODAL, modal_auth_source_picker),
        case("modal-scope-picker", MODAL, modal_scope_picker),
        case("modal-auth-form", MODAL, modal_auth_form),
    ]
}

pub(crate) fn inventory() -> Vec<BaselineCase> {
    let mut cases = Vec::with_capacity(EXPECTED_INVENTORY);
    cases.extend(stage_views());
    cases.extend(account_cases());
    cases.extend(create_prelude_wizard_cases());
    cases.extend(modal_cases());
    cases
}

/// Complete account-era corpus: 16 stage/overlay views, five account cases,
/// four create-prelude wizard steps, and 18 `ConsoleModal` variants.
pub(crate) const EXPECTED_INVENTORY: usize = 43;

pub(crate) fn baselines_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui/view/baselines/png")
}

pub(crate) fn baseline_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.png"))
}

/// Compare-or-bless one case against `dir`. Returns `Err` with a message
/// naming the screen on missing baseline or pixel drift. Bless mode writes
/// the rendered PNG and returns `Ok`.
pub(crate) fn check_case(
    case: &BaselineCase,
    dir: &Path,
    bless: bool,
    rendered: &[u8],
) -> Result<(), String> {
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
