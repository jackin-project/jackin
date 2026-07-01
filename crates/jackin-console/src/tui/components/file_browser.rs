//! Host folder picker — custom directory browser scoped to $HOME.
//!
//! Behavior:
//! - Starts at $HOME.
//! - Refuses navigation above $HOME (clamps cwd back to root).
//! - Excludes noisy top-level directories at the $HOME level.
//! - Rejects $HOME itself and ~/.jackin/* as workspace sources.
//! - Tags git-repo rows with a trailing ` (git)` suffix in a distinct
//!   colour so the operator can scan for repos at a glance. Enter on a
//!   repo row opens a prompt (mount / pick-subdir / cancel) before
//!   committing or navigating in.
//!
//! Filesystem scanning, sandbox checks, Git URL resolution, and browser
//! launching live in `services::file_browser`; this module owns only terminal
//! state, input mapping, geometry, and rendering.
//!
//! The browser was originally built on `ratatui-explorer`, but that
//! crate's `Theme` exposes a single `dir_style` shared by every row —
//! meaning "colour git repos differently" is impossible. Rewriting in-
//! house costs ~400 lines and unlocks per-entry styling plus a simpler
//! keymap (`h/l` / arrows / `s` / `Esc` handled directly instead of
//! round-tripping through the explorer's event handler).

pub(super) use jackin_tui::theme::{PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};

pub(super) mod git_prompt;
pub(super) mod input;
pub mod listing;
pub(super) mod render;
pub(super) mod state;

pub use git_prompt::{GitPromptFocus, git_prompt_rect, git_prompt_url_row_rect};
pub use input::FileBrowserOutcome;
pub use listing::{FolderEntry, FolderListing};
pub use render::{listing_rect, render};
pub use state::FileBrowserState;

#[must_use]
pub fn page_rows_for_modal(term_size: ratatui::layout::Rect, state: &FileBrowserState) -> u16 {
    let modal_area = crate::tui::components::modal_rects::modal_rect_for_mode(
        term_size,
        crate::tui::components::modal_rects::ModalRectMode::FileBrowser,
    );
    let listing_area = listing_rect(modal_area, state.rejected_reason.is_some());
    u16::try_from(jackin_tui::components::viewport_height(listing_area)).unwrap_or(u16::MAX)
}
