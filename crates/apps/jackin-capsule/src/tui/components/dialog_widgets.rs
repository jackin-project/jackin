// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Ratatui rendering for capsule dialog overlays.
//!
//! Every `Dialog` variant is rendered by composing `TermRock` widgets so the
//! Capsule and host share one neutral component vocabulary.
//!
//! Rendering happens inside `compose_ratatui_frame()` via
//! `render_dialog_ratatui()`. The dialog state is snapshotted into
//! `DialogRatatuiSnapshot` before the draw closure borrows the Ratatui
//! terminal so there are no borrow conflicts.

use ratatui::layout::Rect;

// Usage-dialog rendering helpers extracted per R7 step 8. Re-exports preserve
// the original call sites (parent + tests.rs `use super::*` glob, plus
// `dialog::usage_info_required_height` + `dialog/usage.rs` callers).
pub(crate) mod usage;
#[expect(
    unused_imports,
    reason = "re-exports consumed by tests + sibling modules"
)]
pub(crate) use usage::{
    usage_body_rect, usage_content_width, usage_dialog_inner_area, usage_info_lines_for_width,
    usage_info_required_height, usage_line_width, usage_panel_title, usage_provider_display_label,
    usage_scroll_inputs, usage_tab_strip_area, usage_tab_strip_index_at, usage_tab_strip_labels,
    usage_tab_strip_width,
};

// ---------------------------------------------------------------------------
// Snapshot type — fully owned so it outlives the Multiplexer borrow
// ---------------------------------------------------------------------------

/// Renderable row inside a filter-picker dialog.
#[derive(Debug, Clone)]
pub(crate) enum PickerItem {
    /// Selectable item with a display label.
    Item(String),
    /// Non-selectable section separator ("── agents ──").
    Section(String),
}

/// Owned snapshot of a dialog's visible state for the Ratatui draw closure.
#[derive(Debug, Clone)]
pub(crate) enum DialogRatatuiSnapshot {
    /// Yes/No confirmation (maps to `render_confirm_dialog`).
    ConfirmAction {
        title: String,
        body: String,
        selected_yes: bool,
        /// Exit confirmation: render the shared data-loss variant (warns that
        /// quitting force-stops the container) instead of the plain prompt.
        data_loss: bool,
    },
    /// List picker. `show_filter` reserves rows for a type-to-filter input;
    /// flat lists use the entire inner area for their items.
    FilterPicker {
        title: String,
        filter: String,
        items: Vec<PickerItem>,
        /// Index into `items` (includes Section rows) for the focused row.
        selected: usize,
        show_filter: bool,
    },
    /// Single-line text input (`RenameTab`).
    TextInputDialog {
        dialog_title: String,
        label: String,
        value: String,
        cursor: usize,
    },
    /// Shared error popup used for capsule-owned modal errors.
    ErrorPopup(crate::tui::components::dialog::SpawnFailureState),
    /// The "Debug info" dialog, rendered from product-owned container facts
    /// through `TermRock` detail-table, focus, scroll, copy, and link primitives.
    /// GitHub context uses the same variant with GitHub-specific rows.
    DebugInfo(crate::tui::components::container_info_surface::ContainerInfoState),
    /// Usage overlay, rendered from the same scrollable row model as `DebugInfo`
    /// but laid out as CodexBar-style sections instead of generic key/value
    /// diagnostics.
    UsageInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState,
        tabs: Vec<(String, bool)>,
        tab_bar_focused: bool,
        hovered_tab: Option<usize>,
    },
}

impl DialogRatatuiSnapshot {
    /// Per-axis scroll availability for this snapshot's body within `block_area`
    /// (the dialog's outer rect). `ScrollAxes::none()` for dialogs that do not
    /// scroll. Measured the same way `render_scrollable_dialog_body` measures,
    /// so a hint built from this advertises exactly the axes whose scrollbar is
    /// drawn — the hint and the scrollbar never disagree.
    pub(crate) fn scroll_axes(&self, block_area: Rect) -> termrock::scroll::ScrollAxes {
        match self {
            Self::DebugInfo(state) => termrock::scroll::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                block_area,
            ),
            Self::UsageInfo { state, tabs, .. } => {
                // Same body+lines source the renderer uses (Bug 2): wrapped line
                // count + a `scroll_rect` whose viewport is the true body (box
                // minus border minus tab strip). The tab strip width still floors
                // the horizontal content so the strip itself can't overflow.
                let (content_width, content_height, scroll_rect) =
                    usage_scroll_inputs(block_area, state);
                let width = content_width.max(usage_tab_strip_width(tabs));
                termrock::scroll::dialog_scroll_axes(width, content_height, scroll_rect)
            }
            _ => termrock::scroll::ScrollAxes::none(),
        }
    }
}

mod render;
mod snapshot;
pub(crate) use render::render_dialog_ratatui;
