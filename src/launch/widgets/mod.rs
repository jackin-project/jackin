//! Reusable widgets for the workspace manager TUI.
//!
//! Two of the widgets wrap ratatui ecosystem crates (`ratatui-textarea`,
//! `tui-widget-list`). The rest are hand-rolled — `FileBrowser` was
//! originally built on `ratatui-explorer` but was rewritten in-house so
//! git-repo rows can carry a distinct trailing suffix (the library
//! exposes only a single shared `dir_style`). All are consumed by both
//! the manager (PR 2) and the Secrets tab (PR 3).

pub mod confirm;
pub mod file_browser;
pub mod github_picker;
pub mod mount_dst_choice;
pub mod panel_rain;
pub mod save_discard;
pub mod text_input;
pub mod workdir_pick;

/// Outcome of a modal's event-handling cycle. Passed back to the
/// manager state machine to decide whether to close the modal, commit
/// its value, or keep it open.
#[derive(Debug, Clone)]
pub enum ModalOutcome<T> {
    /// User is still interacting with the modal — keep rendering.
    Continue,
    /// User committed with this value (e.g. Enter in text input).
    Commit(T),
    /// User cancelled (Esc).
    Cancel,
}
