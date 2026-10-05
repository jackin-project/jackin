// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Geometry and scroll helpers for read-only dialogs extracted from the
//! coordinator. `box_rect` stays in coordinator per plan.

use super::hint::{
    confirm_hint, export_file_hint, flat_picker_hint, info_dialog_hint, palette_hint, picker_hint,
    read_only_hint, rename_hint, usage_empty_hint, usage_hint,
};
use super::{Dialog, GithubContextView};
use termrock::widgets::HintSpan;

impl Dialog {
    /// Mutable body-scroll state for the read-only info dialogs whose content
    /// can overflow (`ContainerInfo`, `GitHubContext`). `None` for dialogs that do
    /// not scroll. Lets the daemon route mouse-wheel events to the dialog body.
    pub(crate) fn body_scroll_mut(&mut self) -> Option<&mut termrock::scroll::DialogScroll> {
        match self {
            Self::ContainerInfo { scroll, .. }
            | Self::GitHubContext { scroll, .. }
            | Self::Usage { scroll, .. } => Some(scroll),
            _ => None,
        }
    }

    pub(crate) fn clamp_body_scroll(
        &mut self,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let rect = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        if matches!(self, Self::Usage { .. }) {
            let Some(state) = self.usage_state() else {
                return;
            };
            let (content_width, content_height, clamp_rect) =
                crate::tui::components::dialog_widgets::usage_scroll_inputs(rect, &state);
            if let Self::Usage { scroll, .. } = self {
                crate::tui::components::container_info_surface::clamp_container_info_scroll(
                    scroll,
                    content_width,
                    content_height,
                    clamp_rect,
                );
            }
            return;
        }
        if matches!(self, Self::ContainerInfo { .. }) {
            let Some(state) = self.container_info_state() else {
                return;
            };
            if let Self::ContainerInfo { scroll, .. } = self {
                crate::tui::components::container_info_surface::clamp_container_info_scroll(
                    scroll,
                    state.content_width(),
                    state.content_height(),
                    rect,
                );
            }
        } else if let Self::GitHubContext { .. } = self {
            let Some(state) = self.github_context_state(github) else {
                return;
            };
            if let Self::GitHubContext { scroll, .. } = self {
                crate::tui::components::container_info_surface::clamp_container_info_scroll(
                    scroll,
                    state.content_width(),
                    state.content_height(),
                    rect,
                );
            }
        }
    }

    pub(crate) fn body_scroll_axes(
        &self,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) -> termrock::scroll::ScrollAxes {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let rect = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        if matches!(self, Self::Usage { .. }) {
            let Some(state) = self.usage_state() else {
                return termrock::scroll::ScrollAxes::none();
            };
            let (content_width, content_height, scroll_rect) =
                crate::tui::components::dialog_widgets::usage_scroll_inputs(rect, &state);
            return termrock::scroll::dialog_scroll_axes(
                content_width,
                content_height,
                scroll_rect,
            );
        }
        if matches!(self, Self::ContainerInfo { .. }) {
            let Some(state) = self.container_info_state() else {
                return termrock::scroll::ScrollAxes::none();
            };
            return termrock::scroll::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                rect,
            );
        } else if matches!(self, Self::GitHubContext { .. }) {
            let Some(state) = self.github_context_state(github) else {
                return termrock::scroll::ScrollAxes::none();
            };
            return termrock::scroll::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                rect,
            );
        }
        termrock::scroll::ScrollAxes::none()
    }

    /// Footer hint spans for this dialog. Rendered by the multiplexer
    /// compositor near the bottom chrome so every dialog follows the same
    /// hint contract without competing with the branch/container status row.
    ///
    /// `axes` reflects the dialog body's *actual* per-axis overflow (computed
    /// by the caller from the rendered snapshot + rect), so the scrollable info
    /// dialogs advertise only the scroll direction(s) the operator can move —
    /// never both axes when the body fits one.
    pub(crate) fn footer_hint_spans(
        &self,
        github: Option<&GithubContextView<'_>>,
        axes: termrock::scroll::ScrollAxes,
    ) -> Vec<HintSpan<'static>> {
        match self {
            Self::CommandPalette { .. } => palette_hint(),
            Self::SplitDirectionPicker { .. }
            | Self::AgentPicker { .. }
            | Self::CloseTargetPicker { .. } => picker_hint(),
            Self::ExecPicker(_) => vec![
                HintSpan::Key("↑/↓"),
                HintSpan::Text("credential"),
                HintSpan::Key("Space"),
                HintSpan::Text("toggle"),
                HintSpan::Key("PgUp/PgDn"),
                HintSpan::Text("argv"),
                HintSpan::Key("↵"),
                HintSpan::Text("approve"),
                HintSpan::Key("Esc"),
                HintSpan::Text("deny"),
            ],
            Self::RenameTab { .. } => rename_hint(),
            Self::ExportFile { .. } => export_file_hint(),
            Self::ContainerInfo { .. } => info_dialog_hint("copy value", axes),
            Self::SpawnFailure(_) => vec![HintSpan::Key("↵/Esc"), HintSpan::Text("dismiss")],
            Self::GitHubContext { .. } => {
                if github.and_then(|view| view.status.loaded()).is_some() {
                    let mut spans = info_dialog_hint("copy GitHub URL", axes);
                    let insert_at = spans
                        .iter()
                        .rposition(|span| matches!(span, HintSpan::Key("Esc")))
                        .unwrap_or(spans.len());
                    spans.splice(
                        insert_at..insert_at,
                        [
                            HintSpan::Key("O"),
                            HintSpan::Text("open PR"),
                            HintSpan::GroupSep,
                            HintSpan::Key("C"),
                            HintSpan::Text("open CI"),
                            HintSpan::GroupSep,
                        ],
                    );
                    spans
                } else {
                    read_only_hint()
                }
            }
            Self::Usage { projection, .. }
                if projection.as_deref().is_none_or(|projection| {
                    projection
                        .providers
                        .iter()
                        .all(|provider| provider.accounts.is_empty())
                }) =>
            {
                usage_empty_hint(axes)
            }
            Self::Usage { .. } => usage_hint(axes),
            Self::ConfirmAction { .. } => confirm_hint(),
            // No filter input on either: the modal is a fixed choice list and
            // Inspect is a read-only scroll list. Reuse the shared no-filter
            // "select" hint and read-only hint rather than the picker's
            // "type filter" / "launch" wording.
            Self::ExitDirty { .. } => flat_picker_hint(),
            Self::ExitInspect { .. } => read_only_hint(),
        }
    }
}
