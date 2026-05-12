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
//! The browser was originally built on `ratatui-explorer`, but that
//! crate's `Theme` exposes a single `dir_style` shared by every row —
//! meaning "colour git repos differently" is impossible. Rewriting in-
//! house costs ~400 lines and unlocks per-entry styling plus a simpler
//! keymap (`h/l` / arrows / `s` / `Esc` handled directly instead of
//! round-tripping through the explorer's event handler).

use ratatui::style::Color;

pub(super) use super::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};
/// Sandbox-rejection / error red.
pub(super) const DANGER_RED: Color = Color::Rgb(255, 94, 122);

/// Directories excluded from the listing when browsing $HOME.
pub(super) const EXCLUDED: &[&str] = &[
    "Library",
    "Applications",
    "Movies",
    "Music",
    "OrbStack",
    "Pictures",
];

pub(super) mod git_prompt;
pub(super) mod input;
pub(super) mod render;
pub(super) mod state;

pub use git_prompt::{GitPromptFocus, git_prompt_rect, git_prompt_url_row_rect};
pub use render::{listing_rect, render};
pub use state::{FileBrowserState, FolderEntry};
