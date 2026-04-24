//! Reusable widgets for the workspace manager TUI.
//!
//! Three of the widgets wrap ratatui ecosystem crates
//! (`ratatui-textarea`, `ratatui-explorer`, `tui-widget-list`). Two are
//! hand-rolled (`Confirm`, `PanelRain`). All are consumed by both the
//! manager (PR 2) and the Secrets tab (PR 3).

pub mod confirm;
pub mod file_browser;
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
