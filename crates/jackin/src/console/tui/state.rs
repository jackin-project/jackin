//! Manager state machine for the jackin' console TUI.
//!
//! `ManagerState` and all concrete type aliases now live in `jackin-console`.
//! This module re-exports the full public surface and adds root-binary-only
//! helpers that depend on runtime types (`RepoError`, `RoleRepoValidationError`)
//! not available to `jackin-console`.

pub use jackin_console::tui::state::*;

// These re-imports are used by the child `tests` module via `use super::*`.
// Child modules have access to private items of their parent, so placing them
// here (even without `pub`) makes them available to tests without polluting
// the crate's public API.
#[cfg(test)]
use jackin_config::AppConfig;
#[cfg(test)]
use jackin_console::tui::auth::AuthKind;
#[cfg(test)]
use jackin_core::EnvValue;

// ── Root-binary-only helpers ────────────────────────────────────────────────
//
// These functions depend on `crate::runtime` and `crate::repo` types and
// therefore cannot live in `jackin-console`. They call into
// `jackin_console::tui::components::error_popup` for their message strings.

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
    editor.open_error_popup(
        jackin_console::tui::components::error_popup::role_load_error_popup_state(message),
    );
}

/// Translate a runtime role-resolution error into the operator-facing
/// blurb shown beneath the role-input dialog.
///
/// When adding a `RepoError` variant, add the corresponding match arm
/// here. Errors that were never wrapped as `RepoError` (e.g. fs/IO
/// errors raised before the clone) hit the fallback branch — generic
/// rather than mis-classified.
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

/// Render a `RoleRepoValidationError` for the role-input popup.
///
/// `Missing(path)` is shown as the basename only — the full repo path
/// is operator-noise here since the popup already says which role they
/// asked for. Other variants fall back to the typed `Display` impl with
/// any trailing period trimmed (the surrounding sentence adds its own).
fn humanize_invalid_role_repo(err: &crate::repo::RoleRepoValidationError) -> String {
    use crate::repo::RoleRepoValidationError as V;
    match err {
        V::Missing(path) => {
            let file = path
                .file_name()
                .and_then(|name| name.to_str())
                .map_or_else(|| path.display().to_string(), str::to_owned);
            jackin_console::tui::components::error_popup::missing_role_repository_file_message(file)
        }
        _ => err.to_string().trim_end_matches('.').to_owned(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
