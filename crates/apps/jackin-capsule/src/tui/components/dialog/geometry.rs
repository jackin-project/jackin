// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::hint::{
    confirm_hint, export_file_hint, flat_picker_hint, info_dialog_hint, palette_hint, picker_hint,
    read_only_hint, rename_hint, usage_empty_hint, usage_hint,
};
use super::{Dialog, GithubContextView};
use termrock::widgets::HintSpan;

impl Dialog {
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
                let (content_width, content_height, clamp_rect) = if is_usage {
                    crate::tui::components::dialog_widgets::usage_scroll_inputs(rect, &state)
                } else {
                    (state.content_width(), state.content_height(), rect)
                };
                crate::tui::components::container_info_surface::clamp_container_info_scroll(
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
        github: Option<&GithubContextView<'_>>,
    ) -> termrock::scroll::ScrollAxes {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let rect = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        if matches!(self, Self::ContainerInfo { .. }) {
            let Some(state) = self.container_info_state() else {
                return termrock::scroll::ScrollAxes::none();
            };
            return termrock::scroll::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                rect,
            );
        } else if matches!(self, Self::GitHubContext { .. } | Self::Usage { .. }) {
            let is_usage = matches!(self, Self::Usage { .. });
            let state = if matches!(self, Self::GitHubContext { .. }) {
                let Some(state) = self.github_context_state(github) else {
                    return termrock::scroll::ScrollAxes::none();
                };
                state
            } else {
                let Some(state) = self.usage_state() else {
                    return termrock::scroll::ScrollAxes::none();
                };
                state
            };
            let (content_width, content_height, clamp_rect) = if is_usage {
                crate::tui::components::dialog_widgets::usage_scroll_inputs(rect, &state)
            } else {
                (state.content_width(), state.content_height(), rect)
            };
            return termrock::scroll::dialog_scroll_axes(content_width, content_height, clamp_rect);
        }
        termrock::scroll::ScrollAxes::none()
    }

    pub(crate) fn footer_hint_spans(
        &self,
        github: Option<&GithubContextView<'_>>,
        axes: termrock::scroll::ScrollAxes,
    ) -> Vec<HintSpan<'static>> {
        match self {
            Self::CommandPalette { .. } => palette_hint(),
            Self::SplitDirectionPicker { .. }
            | Self::AgentPicker { .. }
            | Self::CloseTargetPicker { .. }
            | Self::ExecPicker(_) => picker_hint(),
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
            Self::Usage { view, .. } if view.tabs.is_empty() => usage_empty_hint(axes),
            Self::Usage { .. } => usage_hint(axes),
            Self::ConfirmAction { .. } => confirm_hint(),

            Self::ExitDirty { .. } => flat_picker_hint(),
            Self::ExitInspect { .. } => read_only_hint(),
        }
    }
}
