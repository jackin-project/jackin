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

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Widget};

use jackin_core::tui_theme::accent_fg;
use termrock::Theme;
use termrock::widgets::{
    Action, ChoiceDialog, ChoiceDialogState, DetailTableState, Dialog as MessageShell, List,
    ListRow, ListState, MessageDialog, Panel, PanelEmphasis, RowRole, Tab, Tabs, TabsState,
    TextInput, TextInputState, Validation,
};

use crate::tui::components::dialog::{Dialog, GithubContextView};

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

            Dialog::ProviderPicker {
                agent,
                providers,
                selected,
                ..
            } => {
                let title = agent
                    .as_deref()
                    .and_then(crate::tui::components::agent_display_name)
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
            render_text_input_dialog(frame, area, dialog_title, label, value, *cursor);
        }
        DialogRatatuiSnapshot::ErrorPopup(state) => {
            let theme = Theme::default();
            let dialog = MessageShell::new(
                &state.title,
                ratatui::text::Text::from(state.message.as_str()),
                &theme,
            )
            .style(Style::default())
            .emphasis(PanelEmphasis::Focused);
            frame.render_stateful_widget(
                &MessageDialog::new(dialog, &[], &theme).wrap(true),
                area,
                &mut DetailTableState::<usize>::default(),
            );
        }
        DialogRatatuiSnapshot::DebugInfo(state) => {
            crate::tui::components::container_info_surface::render_container_info(
                frame, area, state,
            );
        }
        DialogRatatuiSnapshot::UsageInfo {
            state,
            tabs,
            tab_bar_focused,
            hovered_tab,
        } => {
            render_usage_info(frame, area, state, tabs, *tab_bar_focused, *hovered_tab);
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
    let theme = Theme::default();
    // Exit uses the shared data-loss variant (prompt + warning notes); every
    // other confirm keeps the plain title+body prompt. Same widget either way.
    let body = if data_loss {
        "Exit jackin❯?\n\n! Exiting force-stops the container immediately.\n! Work not saved outside the container will be lost.".to_owned()
    } else {
        format!("{title}\n\n{body}")
    };
    let actions = [
        Action {
            id: true,
            label: "Yes",
            enabled: true,
            style: None,
        },
        Action {
            id: false,
            label: "No",
            enabled: true,
            style: None,
        },
    ];
    let dialog = MessageShell::new("Confirm", ratatui::text::Text::from(body), &theme)
        .style(Style::default())
        .emphasis(PanelEmphasis::Focused);
    frame.render_stateful_widget(
        &ChoiceDialog::new(dialog, &actions).gap(" "),
        area,
        &mut ChoiceDialogState::new(Some(selected_yes)),
    );
}

fn render_usage_info(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
    tabs: &[(String, bool)],
    tab_bar_focused: bool,
    hovered_tab: Option<usize>,
) {
    let title = usage_panel_title(state, area.width);
    let theme = Theme::default();
    let inner = termrock::layout::render_dialog_shell(
        frame,
        area,
        Some(title.as_str()),
        PanelEmphasis::Focused,
        &theme,
    );
    if inner.height == 0 {
        return;
    }
    let tab_area = usage_tab_strip_area(inner, tabs);
    let canonical_tabs = tabs
        .iter()
        .enumerate()
        .map(|(id, (label, active))| Tab {
            id,
            label,
            glyph: None,
            active: *active,
            enabled: true,
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        &Tabs::new(&canonical_tabs, &Theme::default()).gap(termrock::widgets::TAB_GAP),
        tab_area,
        &mut TabsState {
            selected: canonical_tabs
                .iter()
                .find(|tab| tab.active)
                .map(|tab| tab.id),
            hovered: hovered_tab,
            focused: tab_bar_focused,
            regions: Vec::new(),
        },
    );
    // Body geometry comes from the shared `usage_body_rect`, the same source the
    // scroll-bound path uses, so the rendered viewport and the scroll clamp can
    // never disagree (Bug 2). (`usage_tab_strip_area` above gives the strip its
    // centered x; its height matches `usage_body_rect`'s fixed 2-row reservation.)
    let body = usage_body_rect(area);
    let lines = usage_info_lines_for_width(state, body.width);
    let mut scroll = state.scroll.clone();
    termrock::layout::render_scrollable_dialog_body(frame, area, body, &lines, &mut scroll, &theme);
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
    // jackin❯ dialog: accent_fg() focused border + bold-white title.
    let theme = Theme::default();
    let block = Panel::new(&theme)
        .title(title)
        .emphasis(PanelEmphasis::Focused)
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
        let mut filter_state = TextInputState::new(filter).with_allow_empty(true);
        frame.render_stateful_widget(
            &TextInput::new("Filter", &theme)
                .placeholder("Filter")
                .validation(Validation::Valid),
            filter_area,
            &mut filter_state,
        );
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
    let rows = items
        .iter()
        .enumerate()
        .map(|(id, item)| match item {
            PickerItem::Section(label) => ListRow {
                id,
                label: Line::from(label.clone()),
                trailing: None,
                role: RowRole::Separator,
                enabled: false,
            },
            PickerItem::Item(label) => ListRow {
                id,
                label: Line::from(Span::styled(
                    label.clone(),
                    Style::default().fg(accent_fg()),
                )),
                trailing: None,
                role: RowRole::Item,
                enabled: true,
            },
        })
        .collect::<Vec<_>>();
    frame.render_stateful_widget(
        &List::new(&rows, &theme),
        list_area,
        &mut ListState::new(Some(selected)),
    );
}

fn render_text_input_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    dialog_title: &str,
    label: &str,
    value: &str,
    cursor: usize,
) {
    let theme = Theme::default();
    let panel = Panel::new(&theme)
        .title(dialog_title)
        .emphasis(PanelEmphasis::Focused);
    let inner = panel.inner(area);
    frame.render_widget(&panel, area);
    if inner.height < 2 {
        return;
    }
    frame.render_widget(ratatui::widgets::Paragraph::new(format!("{label}:")), inner);
    let mut state = TextInputState::new(value).with_allow_empty(true);
    assert!(
        state.set_cursor_byte(cursor),
        "text-input snapshot cursor must remain on a grapheme boundary"
    );
    frame.render_stateful_widget(
        &TextInput::new(label, &theme)
            .placeholder("")
            .validation(Validation::Valid),
        Rect {
            y: inner.y.saturating_add(1),
            height: 1,
            ..inner
        },
        &mut state,
    );
}
