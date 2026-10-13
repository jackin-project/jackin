// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Dialog-to-snapshot conversion: owned `DialogRatatuiSnapshot` builder.

use super::{DialogRatatuiSnapshot, PickerItem, usage_tab_strip_labels};
use crate::tui::components::dialog::{Dialog, GithubContextView};

impl Dialog {
    /// Build a fully-owned snapshot for Ratatui rendering. Called before
    /// the `ratatui_terminal.draw()` closure so there are no borrow conflicts.
    #[expect(
        clippy::too_many_lines,
        reason = "Dialog renderer snapshot builder carrying each dialog variant's \
                  per-row layout inline. Extracting per-variant bodies would \
                  require re-borrowing the dialog state across fn boundaries."
    )]
    pub(crate) fn to_ratatui_snapshot(
        &self,
        github: Option<&GithubContextView<'_>>,
    ) -> DialogRatatuiSnapshot {
        #[expect(
            clippy::expect_used,
            reason = "ContainerInfo match arm has already proven this dialog variant"
        )]
        match self {
            Dialog::ConfirmAction { kind, selected_yes } => {
                // Exit always renders the shared data-loss state (exit_confirm_state_with_data_loss);
                // title/message are unused for Exit so we pass empty strings to avoid dead formatting.
                let data_loss = matches!(kind, crate::tui::components::dialog::ConfirmKind::Exit);
                DialogRatatuiSnapshot::ConfirmAction {
                    title: if data_loss {
                        String::new()
                    } else {
                        kind.title().to_owned()
                    },
                    body: if data_loss {
                        String::new()
                    } else {
                        kind.message().to_owned()
                    },
                    selected_yes: *selected_yes,
                    data_loss,
                }
            }

            Dialog::CommandPalette {
                selected,
                filter,
                close_label,
            } => {
                use crate::tui::components::dialog::{PALETTE_ITEMS, PaletteCommand};
                let needle = filter.to_ascii_lowercase();
                let items: Vec<PickerItem> = PALETTE_ITEMS
                    .iter()
                    .filter_map(|(command, label)| {
                        let label = if matches!(command, PaletteCommand::Close) {
                            close_label.label()
                        } else {
                            label
                        };
                        if needle.is_empty() || label.to_ascii_lowercase().contains(&needle) {
                            Some(PickerItem::Item(label.to_owned()))
                        } else {
                            None
                        }
                    })
                    .collect();
                DialogRatatuiSnapshot::FilterPicker {
                    title: "Menu".into(),
                    filter: filter.clone(),
                    items,
                    selected: *selected,
                    show_filter: true,
                }
            }

            Dialog::AgentPicker {
                agents,
                selected,
                intent,
                filter,
            } => {
                use crate::tui::components::dialog::PickerIntent;
                let title = match intent {
                    PickerIntent::NewTab => "New tab".to_owned(),
                    PickerIntent::Split(dir) => format!("Split: {}", dir.label()),
                };
                let needle = filter.to_ascii_lowercase();
                let agent_matches: Vec<(usize, &str)> = agents
                    .iter()
                    .enumerate()
                    .filter_map(|(i, slug)| {
                        let label = crate::tui::components::agent_display_name(slug.as_str())
                            .unwrap_or(slug.as_str());
                        if needle.is_empty() || label.to_ascii_lowercase().contains(&needle) {
                            Some((i, label))
                        } else {
                            None
                        }
                    })
                    .collect();
                let shell_match = needle.is_empty() || "shell".contains(&needle);
                let mut items: Vec<PickerItem> = Vec::with_capacity(agent_matches.len() + 3);
                if !agent_matches.is_empty() {
                    // Label only — render_picker_list draws the ── dashes full-width.
                    items.push(PickerItem::Section("agents".into()));
                    for (_, label) in &agent_matches {
                        items.push(PickerItem::Item((*label).to_owned()));
                    }
                }
                if shell_match {
                    items.push(PickerItem::Section("shells".into()));
                    items.push(PickerItem::Item("Shell".into()));
                }
                DialogRatatuiSnapshot::FilterPicker {
                    title,
                    filter: filter.clone(),
                    items,
                    selected: *selected,
                    show_filter: true,
                }
            }

            Dialog::ExecPicker(state) => {
                // Multi-select credential list. The checkbox state is encoded in
                // each row label (`[x]` / `[ ]`) so the shared single-select
                // FilterPicker widget renders it without a bespoke widget; the
                // cursor is the highlighted row, Space toggles via handle_key.
                let items: Vec<PickerItem> = state
                    .items
                    .iter()
                    .map(|item| {
                        let mark = if item.selected { "[x]" } else { "[ ]" };
                        PickerItem::Item(format!("{mark} {}  {}", item.binding.name, item.display))
                    })
                    .collect();
                DialogRatatuiSnapshot::FilterPicker {
                    title: format!("Attach credentials · {}", state.command),
                    filter: String::new(),
                    items,
                    selected: state.cursor,
                    show_filter: false,
                }
            }

            Dialog::SplitDirectionPicker { selected, filter } => {
                use crate::tui::components::dialog::SPLIT_DIRECTION_ITEMS;
                let needle = filter.to_ascii_lowercase();
                let items: Vec<PickerItem> = SPLIT_DIRECTION_ITEMS
                    .iter()
                    .filter(|dir| {
                        needle.is_empty() || dir.label().to_ascii_lowercase().contains(&needle)
                    })
                    .map(|dir| PickerItem::Item(dir.label().to_owned()))
                    .collect();
                DialogRatatuiSnapshot::FilterPicker {
                    title: "Split direction".into(),
                    filter: filter.clone(),
                    items,
                    selected: *selected,
                    show_filter: true,
                }
            }

            Dialog::CloseTargetPicker { selected, filter } => {
                use crate::tui::components::dialog::CLOSE_TARGET_ITEMS;
                let needle = filter.to_ascii_lowercase();
                let items: Vec<PickerItem> = CLOSE_TARGET_ITEMS
                    .iter()
                    .filter(|(_, label)| {
                        needle.is_empty() || label.to_ascii_lowercase().contains(&needle)
                    })
                    .map(|(_, label)| PickerItem::Item((*label).to_owned()))
                    .collect();
                DialogRatatuiSnapshot::FilterPicker {
                    title: "Close".into(),
                    filter: filter.clone(),
                    items,
                    selected: *selected,
                    show_filter: true,
                }
            }

            Dialog::RenameTab { input, .. } => DialogRatatuiSnapshot::TextInputDialog {
                dialog_title: "Rename tab".into(),
                label: "Name".into(),
                value: input.value().to_owned(),
                cursor: input.cursor_byte(),
            },
            Dialog::ExportFile {
                input,
                reveal_after_export,
                open_after_export,
            } => DialogRatatuiSnapshot::TextInputDialog {
                dialog_title: if *open_after_export {
                    "Export file and open".into()
                } else if *reveal_after_export {
                    "Export file and reveal".into()
                } else {
                    "Export file".into()
                },
                label: "Path".into(),
                value: input.value().to_owned(),
                cursor: input.cursor_byte(),
            },
            Dialog::SpawnFailure(state) => DialogRatatuiSnapshot::ErrorPopup(state.clone()),

            Dialog::ContainerInfo { .. } => DialogRatatuiSnapshot::DebugInfo(
                self.container_info_state()
                    .expect("container_info_state is Some for ContainerInfo"),
            ),

            Dialog::GitHubContext { .. } => DialogRatatuiSnapshot::DebugInfo(
                self.github_context_state(github)
                    .expect("github_context_state is Some for GitHubContext"),
            ),

            Dialog::ExitDirty {
                summary, selected, ..
            } => {
                use crate::tui::components::dialog::EXIT_DIRTY_ROWS;
                // Per-repo summary lines render as non-selectable section rows
                // above the four choice rows.
                let mut items: Vec<PickerItem> = summary
                    .iter()
                    .map(|line| PickerItem::Section(line.clone()))
                    .collect();
                let first_choice = items.len();
                for (_, label) in EXIT_DIRTY_ROWS {
                    items.push(PickerItem::Item(label.to_owned()));
                }
                let last_choice = EXIT_DIRTY_ROWS.len().saturating_sub(1);
                DialogRatatuiSnapshot::FilterPicker {
                    title: "Unsaved work — exit?".into(),
                    filter: String::new(),
                    items,
                    selected: first_choice + (*selected).min(last_choice),
                    show_filter: false,
                }
            }

            Dialog::ExitInspect { lines, selected } => {
                use crate::tui::components::dialog::InspectRow;
                let items = lines
                    .iter()
                    .map(|row| match row {
                        InspectRow::Repo(label) => PickerItem::Section(label.clone()),
                        InspectRow::File(line) => PickerItem::Item(line.clone()),
                    })
                    .collect();
                DialogRatatuiSnapshot::FilterPicker {
                    title: "Inspect changes".into(),
                    filter: String::new(),
                    items,
                    selected: *selected,
                    show_filter: false,
                }
            }
            Dialog::Usage {
                view,
                selected,
                tab_bar_focused,
                hovered_tab,
                ..
            } => DialogRatatuiSnapshot::UsageInfo {
                state: self.usage_state().expect("usage_state is Some for Usage"),
                tabs: usage_tab_strip_labels(view, *selected),
                tab_bar_focused: *tab_bar_focused,
                hovered_tab: *hovered_tab,
            },
        }
    }
}
