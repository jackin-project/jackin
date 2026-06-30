//! Geometry and scroll helpers for read-only dialogs extracted from the
//! coordinator. `box_rect` stays in coordinator per plan.

use super::hint::{
    confirm_hint, export_file_hint, info_dialog_hint, palette_hint, picker_hint, provider_hint,
    read_only_hint, rename_hint, usage_hint,
};
use super::{Dialog, GithubContextView};
use jackin_tui::HintSpan;

impl Dialog {
    /// Mutable body-scroll state for the read-only info dialogs whose content
    /// can overflow (`ContainerInfo`, `GitHubContext`). `None` for dialogs that do
    /// not scroll. Lets the daemon route mouse-wheel events to the dialog body.
    pub(crate) fn body_scroll_mut(
        &mut self,
    ) -> Option<&mut jackin_tui::components::DialogBodyScroll> {
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
        if matches!(self, Self::ContainerInfo { .. }) {
            let Some(state) = self.container_info_state() else {
                return;
            };
            if let Self::ContainerInfo { scroll, .. } = self {
                jackin_tui::components::clamp_container_info_scroll(
                    scroll,
                    state.content_width(),
                    state.content_height(),
                    rect,
                );
            }
        } else if matches!(self, Self::GitHubContext { .. } | Self::Usage { .. }) {
            let is_usage = matches!(self, Self::Usage { .. });
            let state = if matches!(self, Self::GitHubContext { .. }) {
                let Some(state) = self.github_context_state(github) else {
                    return;
                };
                state
            } else {
                let Some(state) = self.usage_state() else {
                    return;
                };
                state
            };
            if let Self::GitHubContext { scroll, .. } | Self::Usage { scroll, .. } = self {
                // Usage clamps against the same body+lines the renderer uses, with
                // a rect whose viewport excludes the tab strip (Bug 2); other
                // dialogs clamp against the box rect directly.
                let (content_width, content_height, clamp_rect) = if is_usage {
                    crate::tui::components::dialog_widgets::usage_scroll_inputs(rect, &state)
                } else {
                    (state.content_width(), state.content_height(), rect)
                };
                jackin_tui::components::clamp_container_info_scroll(
                    scroll,
                    content_width,
                    content_height,
                    clamp_rect,
                );
            }
        }
    }

    pub(crate) fn body_scroll_axes(
        &self,
        term_rows: u16,
        term_cols: u16,
        _github: Option<&GithubContextView<'_>>,
    ) -> jackin_tui::components::ScrollAxes {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let rect = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        if matches!(self, Self::ContainerInfo { .. }) {
            let Some(state) = self.container_info_state() else {
                return jackin_tui::components::ScrollAxes::none();
            };
            return jackin_tui::components::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                rect,
            );
        }
        jackin_tui::components::ScrollAxes::none()
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
        axes: jackin_tui::components::ScrollAxes,
    ) -> Vec<HintSpan<'static>> {
        match self {
            Self::CommandPalette { .. } => palette_hint(),
            Self::SplitDirectionPicker { .. }
            | Self::AgentPicker { .. }
            | Self::CloseTargetPicker { .. }
            | Self::ExecPicker(_) => picker_hint(),
            Self::ProviderPicker { .. } => provider_hint(),
            Self::RenameTab { .. } => rename_hint(),
            Self::ExportFile { .. } => export_file_hint(),
            Self::ContainerInfo { .. } => info_dialog_hint("copy value", axes),
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
            Self::Usage { .. } => usage_hint(axes),
            Self::ConfirmAction { .. } => confirm_hint(),
            // No filter input on either: the modal is a fixed choice list and
            // Inspect is a read-only scroll list. Reuse the shared no-filter
            // "select" hint and read-only hint rather than the picker's
            // "type filter" / "launch" wording.
            Self::ExitDirty { .. } => provider_hint(),
            Self::ExitInspect { .. } => read_only_hint(),
        }
    }
}
