//! Ratatui rendering for capsule dialog overlays.
//!
//! Every `Dialog` variant is rendered as a Ratatui widget using shared
//! `jackin-tui` components (Panel, `FilterInput`, `ConfirmDialog`, etc.) so
//! the capsule and the host share one component vocabulary.
//!
//! Rendering happens inside `compose_ratatui_frame()` via
//! `render_dialog_ratatui()`. The dialog state is snapshotted into
//! `DialogRatatuiSnapshot` before the draw closure borrows the Ratatui
//! terminal so there are no borrow conflicts.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Widget};

use jackin_tui::components::confirm_dialog::{ConfirmState, render_confirm_dialog};
use jackin_tui::components::filter_input::render_filter_input;
use jackin_tui::theme::PHOSPHOR_GREEN;

use crate::tui::components::dialog::{Dialog, GithubContextView};

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
    /// List picker (`CommandPalette`, `AgentPicker`, `SplitPicker`, `ClosePicker`,
    /// `ProviderPicker`). `show_filter` draws the type-to-filter input + gap
    /// above the items; `ProviderPicker` is a flat list and clears it so its
    /// `box_rect` (border + items + border) is not under-allocated by the
    /// two reserved filter rows, which clipped the list.
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
    /// The "Debug info" dialog, rendered through the shared jackin-tui
    /// `ContainerInfoState` so its rows, copy affordances, focused shell,
    /// spacing, link styling, and hover behaviour are identical to the host
    /// console and launch cockpit. GitHub context uses the same variant with
    /// GitHub-specific rows.
    DebugInfo(jackin_tui::components::ContainerInfoState),
}

impl Dialog {
    /// Build a fully-owned snapshot for Ratatui rendering. Called before
    /// the `ratatui_terminal.draw()` closure so there are no borrow conflicts.
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
                        let label =
                            jackin_tui::agent_display_name(slug.as_str()).unwrap_or(slug.as_str());
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

            Dialog::ProviderPicker {
                agent,
                providers,
                selected,
                ..
            } => {
                let title = agent
                    .as_deref()
                    .and_then(jackin_tui::agent_display_name)
                    .map_or_else(|| "Provider".into(), |n| format!("Provider: {n}"));
                let items: Vec<PickerItem> = providers
                    .iter()
                    .map(|p| PickerItem::Item(p.label.clone()))
                    .collect();
                DialogRatatuiSnapshot::FilterPicker {
                    title,
                    filter: String::new(),
                    items,
                    selected: *selected,
                    show_filter: false,
                }
            }

            Dialog::RenameTab { input, .. } => DialogRatatuiSnapshot::TextInputDialog {
                dialog_title: "Rename tab".into(),
                label: "Name".into(),
                value: input.value().to_owned(),
                cursor: input.cursor(),
            },

            Dialog::ContainerInfo { .. } => DialogRatatuiSnapshot::DebugInfo(
                self.container_info_state()
                    .expect("container_info_state is Some for ContainerInfo"),
            ),

            Dialog::GitHubContext { .. } => DialogRatatuiSnapshot::DebugInfo(
                self.github_context_state(github)
                    .expect("github_context_state is Some for GitHubContext"),
            ),

            Dialog::ExitDirty { summary, selected } => {
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
        }
    }
}

impl DialogRatatuiSnapshot {
    /// Per-axis scroll availability for this snapshot's body within `block_area`
    /// (the dialog's outer rect). `ScrollAxes::none()` for dialogs that do not
    /// scroll. Measured the same way `render_scrollable_dialog_body` measures,
    /// so a hint built from this advertises exactly the axes whose scrollbar is
    /// drawn — the hint and the scrollbar never disagree.
    pub(crate) fn scroll_axes(&self, block_area: Rect) -> jackin_tui::components::ScrollAxes {
        match self {
            Self::DebugInfo(state) => jackin_tui::components::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                block_area,
            ),
            _ => jackin_tui::components::ScrollAxes::none(),
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering — called from compose_ratatui_frame() inside the draw closure
// ---------------------------------------------------------------------------

/// Render a dialog overlay using Ratatui shared components.
///
/// `rect` is the `(row, col, height, width)` tuple from `Dialog::box_rect()`,
/// already computed before the draw closure. `frame` is the Ratatui frame
/// for the current draw pass.
pub(crate) fn render_dialog_ratatui(
    frame: &mut Frame<'_>,
    rect: (u16, u16, u16, u16),
    snapshot: &DialogRatatuiSnapshot,
) {
    let (row, col, height, width) = rect;
    let area = Rect {
        x: col,
        y: row,
        width,
        height,
    };
    // Skip if the dialog rect would overflow the terminal.
    if area.right() > frame.area().width || area.bottom() > frame.area().height {
        return;
    }
    match snapshot {
        DialogRatatuiSnapshot::ConfirmAction {
            title,
            body,
            selected_yes,
            data_loss,
        } => {
            render_confirm_action(frame, area, title, body, *selected_yes, *data_loss);
        }
        DialogRatatuiSnapshot::FilterPicker {
            title,
            filter,
            items,
            selected,
            show_filter,
        } => {
            render_filter_picker(frame, area, title, filter, items, *selected, *show_filter);
        }
        DialogRatatuiSnapshot::TextInputDialog {
            dialog_title,
            label,
            value,
            cursor,
        } => {
            jackin_tui::components::render_labeled_text_input_dialog(
                frame,
                area,
                dialog_title,
                label,
                value,
                *cursor,
            );
        }
        DialogRatatuiSnapshot::DebugInfo(state) => {
            jackin_tui::components::render_container_info(frame, area, state);
        }
    }
}

// ---------------------------------------------------------------------------
// Per-variant render helpers
// ---------------------------------------------------------------------------

fn render_confirm_action(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    body: &str,
    selected_yes: bool,
    data_loss: bool,
) {
    // Exit uses the shared data-loss variant (prompt + warning notes); every
    // other confirm keeps the plain title+body prompt. Same widget either way.
    let mut state = if data_loss {
        jackin_tui::components::exit_confirm_state_with_data_loss()
    } else {
        ConfirmState::new(format!("{title}\n\n{body}"))
    };
    if selected_yes {
        state = state.with_focus_yes();
    }
    render_confirm_dialog(frame, area, &state);
}

fn render_filter_picker(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    filter: &str,
    items: &[PickerItem],
    selected: usize,
    show_filter: bool,
) {
    // Reuse the shared modal panel so the menu/pickers match every other
    // jackin' dialog: PHOSPHOR_GREEN focused border + bold-white title.
    let title_str = format!(" {title} ");
    let block = jackin_tui::components::Panel::new()
        .title(&title_str)
        .focus(jackin_tui::components::PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    Clear.render(area, frame.buffer_mut());
    block.render(area, frame.buffer_mut());

    if inner.height < 1 {
        return;
    }

    // A flat list (ProviderPicker) fills the whole inner area from row 0; a
    // filterable picker reserves row 0 for the input and row 1 as a gap, so
    // its items start at row 2. box_rect mirrors this: +2 rows flat, +4 with
    // the filter — keep the two in lockstep or the list clips.
    let list_area = if show_filter {
        let filter_area = Rect { height: 1, ..inner };
        render_filter_input(frame, filter_area, filter);
        if inner.height < 3 {
            return;
        }
        // Items from row 2 onward (row 1 = separator gap). Section rows are
        // dim; item rows are white and let the shared render_picker_list paint
        // the selected-row highlight (green background, ▸ cursor) + scroll
        // thumb.
        Rect {
            y: inner.y + 2,
            height: inner.height.saturating_sub(2),
            ..inner
        }
    } else {
        inner
    };

    // Section rows are full-width centered dividers drawn by render_picker_list;
    // item rows are white and let the shared highlight paint the selected row.
    let rows: Vec<jackin_tui::components::PickerRow<'_>> = items
        .iter()
        .map(|item| match item {
            PickerItem::Section(label) => {
                jackin_tui::components::PickerRow::Separator(label.clone())
            }
            PickerItem::Item(label) => jackin_tui::components::PickerRow::Item(
                ratatui::widgets::ListItem::new(Line::from(Span::styled(
                    label.clone(),
                    Style::default().fg(PHOSPHOR_GREEN),
                ))),
            ),
        })
        .collect();

    jackin_tui::components::render_picker_list(list_area, frame.buffer_mut(), rows, Some(selected));
}
