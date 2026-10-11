// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Dialog click dispatch against modal hit regions.

use super::github_context::GithubContextView;
use super::input::{
    PickerRow, close_target_filtered_indices, picker_filtered_rows,
    split_direction_filtered_indices,
};
use super::usage::UsageDialogTab;
use super::{
    CLOSE_TARGET_ITEMS, ConfirmKind, Dialog, DialogAction, GITHUB_OPEN_CI_ROW, GITHUB_OPEN_PR_ROW,
    SPLIT_DIRECTION_ITEMS, file_url_path,
};

use crate::tui::components::palette::{PALETTE_ITEMS, palette_filtered_indices};

impl Dialog {
    /// Dispatch a left-click at `(row, col)` against the dialog's
    /// hit regions. Shared modal lifecycle classification handles
    /// outside-dismiss; inside clicks on a row select that row and
    /// immediately confirm; clicks on the border or padding rows are
    /// consumed so they do not leak through to the focused pane underneath.
    #[expect(
        clippy::too_many_lines,
        reason = "Dialog click dispatcher: per-Dialog-variant handle-click arm with \
                  nested per-hit-test + border-click + confirm-cancel + dialog-pop. \
                  Modal nesting is the per-variant dispatch protocol."
    )]
    pub fn handle_click(
        &mut self,
        row: u16,
        col: u16,
        term_rows: u16,
        term_cols: u16,
        github: Option<&GithubContextView<'_>>,
    ) -> DialogAction {
        let (box_row, box_col, height, width) = self.box_rect(term_rows, term_cols);
        let area = ratatui::layout::Rect {
            x: box_col,
            y: box_row,
            width,
            height,
        };
        // Outside the box dismisses; an inside hit falls through to the
        // per-dialog click handling below.
        if !area.contains(ratatui::layout::Position { x: col, y: row }) {
            return DialogAction::Dismiss;
        }
        // Text-input dialog has no clickable rows — clicks inside the
        // box are just swallowed so they don't dismiss or reach the
        // pane underneath.
        if matches!(self, Self::RenameTab { .. } | Self::ExportFile { .. }) {
            return DialogAction::Consume;
        }
        if matches!(self, Self::SpawnFailure(_)) {
            return DialogAction::Consume;
        }
        // ContainerInfo: any copyable row (Container ID or Invocation ID)
        // log) copies via the shared hit-test. The clicked row's value goes to
        // the clipboard and that row shows the copied check affordance.
        if matches!(self, Self::ContainerInfo { .. }) {
            let hit = self.container_info_state().and_then(|state| {
                crate::tui::components::container_info_surface::container_info_copy_payload_at(
                    area, &state, col, row,
                )
            });
            if let Some((hit_row, payload)) = hit {
                if let Self::ContainerInfo { copied_row, .. } = self {
                    *copied_row = Some(hit_row);
                }
                return DialogAction::CopyToClipboard(payload);
            }
            let reveal_hit = self.container_info_state().and_then(|state| {
                crate::tui::components::container_info_surface::container_info_hyperlink_payload_at(
                    area, &state, col, row,
                )
            });
            return match reveal_hit.and_then(|(_, href)| file_url_path(&href).map(str::to_owned)) {
                Some(path) => DialogAction::RevealHostPath(path),
                None => DialogAction::Consume,
            };
        }
        if matches!(self, Self::GitHubContext { .. }) {
            let area = ratatui::layout::Rect {
                x: box_col,
                y: box_row,
                width,
                height,
            };
            let hit = self.github_context_state(github).and_then(|state| {
                crate::tui::components::container_info_surface::container_info_copy_payload_at(
                    area, &state, col, row,
                )
            });
            if let Some((_hit_row, payload)) = hit {
                if let Self::GitHubContext { copied, .. } = self {
                    *copied = true;
                }
                return DialogAction::CopyToClipboard(payload);
            }
            let open_hit = self.github_context_state(github).and_then(|state| {
                crate::tui::components::container_info_surface::container_info_hyperlink_payload_at(
                    area, &state, col, row,
                )
            });
            return match open_hit {
                Some((GITHUB_OPEN_PR_ROW | GITHUB_OPEN_CI_ROW, payload)) => {
                    DialogAction::OpenHostUrl(payload)
                }
                Some(_) | None => DialogAction::Consume,
            };
        }
        if let Self::Usage { view, selected, .. } = self {
            let tab = Self::usage_tab_index_at(view, *selected, area, row, col);
            return match tab {
                Some(0) => {
                    *selected = UsageDialogTab::Overview;
                    DialogAction::Redraw
                }
                Some(idx) => view.tabs.get(idx.saturating_sub(1)).map_or_else(
                    || DialogAction::Consume,
                    |tab| DialogAction::SwitchUsageProvider {
                        provider_label: tab.label.clone(),
                        account_id: tab.id.clone(),
                    },
                ),
                None => DialogAction::Consume,
            };
        }
        // ConfirmAction: only the visible Yes/No button cells confirm or
        // dismiss. The shared confirm widget owns button geometry, including
        // the taller data-loss exit variant.
        if let Self::ConfirmAction { kind, selected_yes } = self {
            let body = if matches!(kind, ConfirmKind::Exit) {
                "Exit jackin❯?\n\n! Exiting force-stops the container immediately.\n! Work not saved outside the container will be lost.".to_owned()
            } else {
                format!("{}\n\n{}", kind.title(), kind.message())
            };
            let area = ratatui::layout::Rect {
                x: box_col,
                y: box_row,
                width,
                height,
            };
            let actions = [
                termrock::widgets::Action {
                    id: true,
                    label: "Yes",
                    enabled: true,
                    style: None,
                },
                termrock::widgets::Action {
                    id: false,
                    label: "No",
                    enabled: true,
                    style: None,
                },
            ];
            let mut state = termrock::widgets::ChoiceDialogState::new(Some(*selected_yes));
            let theme = termrock::style::DesignSystem::default();
            let mut buffer = ratatui::buffer::Buffer::empty(area);
            let dialog =
                termrock::widgets::Dialog::new("Confirm", ratatui::text::Text::from(body), &theme)
                    .style(ratatui::style::Style::default())
                    .emphasis(termrock::widgets::PanelChrome::Focused);
            ratatui::widgets::StatefulWidget::render(
                &termrock::widgets::ChoiceDialog::new(dialog, &actions).gap(" "),
                area,
                &mut buffer,
                &mut state,
            );
            return match state.click(ratatui::layout::Position::new(col, row)) {
                termrock::interaction::Outcome::Activated(true) => {
                    DialogAction::ConfirmedAction(*kind)
                }
                termrock::interaction::Outcome::Activated(false) => DialogAction::Dismiss,
                _ => DialogAction::Consume,
            };
        }
        // Row layout inside the box for filterable dialogs:
        //   box_row + 0:  top border (decorative)
        //   box_row + 1:  blank pad row
        //   box_row + 2:  filter input ("/ <text>▏")
        //   box_row + 3:  blank pad row separating filter from items
        //   box_row + 3:  first item row
        //
        // Clicks on the filter row are no-op consumes (no in-place
        // edit yet); clicks on item rows select + confirm against
        // the current filtered list so a future refactor that
        // shortens / lengthens the visible item count via filter
        // input still routes the click to the right action.
        let first_item_row = box_row + 3;
        let visible_count: u16 = match self {
            Self::CommandPalette {
                filter,
                close_label,
                ..
            } => u16::try_from(palette_filtered_indices(filter, *close_label).len())
                .unwrap_or(u16::MAX),
            Self::SplitDirectionPicker { filter, .. } => {
                u16::try_from(split_direction_filtered_indices(filter).len()).unwrap_or(u16::MAX)
            }
            Self::CloseTargetPicker { filter, .. } => {
                u16::try_from(close_target_filtered_indices(filter).len()).unwrap_or(u16::MAX)
            }
            Self::AgentPicker { agents, filter, .. } => {
                u16::try_from(picker_filtered_rows(agents, filter).len()).unwrap_or(u16::MAX)
            }
            Self::RenameTab { .. }
            | Self::ExportFile { .. }
            | Self::ContainerInfo { .. }
            | Self::GitHubContext { .. }
            | Self::Usage { .. }
            | Self::SpawnFailure(_)
            | Self::ConfirmAction { .. }
            | Self::ExecPicker(_)
            | Self::ExitDirty { .. }
            | Self::ExitInspect { .. } => 0,
        };
        if row < first_item_row || row >= first_item_row + visible_count {
            return DialogAction::Consume;
        }
        let visible_idx = (row - first_item_row) as usize;
        match self {
            Self::CommandPalette {
                selected,
                filter,
                close_label,
            } => {
                let visible = palette_filtered_indices(filter, *close_label);
                let Some(&source_idx) = visible.get(visible_idx) else {
                    return DialogAction::Consume;
                };
                *selected = visible_idx;
                DialogAction::Command(PALETTE_ITEMS[source_idx].0.clone())
            }
            Self::SplitDirectionPicker { selected, filter } => {
                let visible = split_direction_filtered_indices(filter);
                let Some(&source_idx) = visible.get(visible_idx) else {
                    return DialogAction::Consume;
                };
                *selected = visible_idx;
                DialogAction::SplitDirection(SPLIT_DIRECTION_ITEMS[source_idx])
            }
            Self::CloseTargetPicker { selected, filter } => {
                let visible = close_target_filtered_indices(filter);
                let Some(&source_idx) = visible.get(visible_idx) else {
                    return DialogAction::Consume;
                };
                *selected = visible_idx;
                DialogAction::PickedCloseTarget(CLOSE_TARGET_ITEMS[source_idx].0)
            }
            Self::AgentPicker {
                agents,
                selected,
                intent,
                filter,
            } => {
                let visible = picker_filtered_rows(agents, filter);
                let Some(&picker_row) = visible.get(visible_idx) else {
                    return DialogAction::Consume;
                };
                match picker_row {
                    PickerRow::Section(_) => DialogAction::Consume,
                    PickerRow::Agent(idx) => {
                        *selected = visible_idx;
                        DialogAction::SpawnAgent {
                            agent: Some(agents[idx].clone()),
                            intent: *intent,
                        }
                    }
                    PickerRow::Shell => {
                        *selected = visible_idx;
                        DialogAction::SpawnAgent {
                            agent: None,
                            intent: *intent,
                        }
                    }
                }
            }
            // Text-input, ContainerInfo, and ConfirmAction
            // clicks were already handled by early returns above.
            Self::RenameTab { .. }
            | Self::ExportFile { .. }
            | Self::ContainerInfo { .. }
            | Self::GitHubContext { .. }
            | Self::Usage { .. }
            | Self::ConfirmAction { .. }
            | Self::ExecPicker(_)
            | Self::ExitDirty { .. }
            | Self::ExitInspect { .. }
            | Self::SpawnFailure(_) => DialogAction::Consume,
        }
    }
}
