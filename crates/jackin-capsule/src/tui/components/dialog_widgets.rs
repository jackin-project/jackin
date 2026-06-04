//! Ratatui rendering for capsule dialog overlays.
//!
//! Every `Dialog` variant is rendered as a Ratatui widget using shared
//! `jackin-tui` components (Panel, FilterInput, ConfirmDialog, etc.) so
//! the capsule and the host share one component vocabulary.
//!
//! Rendering happens inside `compose_ratatui_frame()` via
//! `render_dialog_ratatui()`. The dialog state is snapshotted into
//! `DialogRatatuiSnapshot` before the draw closure borrows the Ratatui
//! terminal so there are no borrow conflicts.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use jackin_tui::components::confirm_dialog::{ConfirmState, render_confirm_dialog};
use jackin_tui::components::filter_input::render_filter_input;
use jackin_tui::theme::{PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};
use ratatui::style::Color;

use crate::pull_request::PullRequestInfo;
use crate::tui::components::dialog::Dialog;

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
    },
    /// Type-to-filter list picker (CommandPalette, AgentPicker, SplitPicker,
    /// ClosePicker, ProviderPicker).
    FilterPicker {
        title: String,
        filter: String,
        items: Vec<PickerItem>,
        /// Index into `items` (includes Section rows) for the focused row.
        selected: usize,
    },
    /// Single-line text input (RenameTab).
    TextInputDialog {
        dialog_title: String,
        label: String,
        value: String,
        cursor: usize,
    },
    /// Read-only label/value info panel (GitHubContext).
    InfoRows {
        dialog_title: String,
        rows: Vec<(String, String)>,
        /// Which row index carries the copy shortcut (OSC 52 target).
        copy_row: Option<usize>,
        /// Whether the copy was just triggered (shows "✓ Copied!").
        copied: bool,
    },
    /// The "Debug info" dialog, rendered through the shared jackin-tui
    /// `ContainerInfoState` so its rows, copy affordances, link styling, and
    /// hover behaviour are identical to the host console and launch cockpit.
    DebugInfo(jackin_tui::components::ContainerInfoState),
}

impl Dialog {
    /// Build a fully-owned snapshot for Ratatui rendering. Called before
    /// the `ratatui_terminal.draw()` closure so there are no borrow conflicts.
    pub(crate) fn to_ratatui_snapshot(
        &self,
        pr_branch: Option<&str>,
        pr_info: Option<&PullRequestInfo>,
        pr_loading: bool,
    ) -> DialogRatatuiSnapshot {
        match self {
            Dialog::ConfirmAction { kind, selected_yes } => DialogRatatuiSnapshot::ConfirmAction {
                title: kind.title().to_string(),
                body: kind.message().to_string(),
                selected_yes: *selected_yes,
            },

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
                            Some(PickerItem::Item(label.to_string()))
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
                    PickerIntent::NewTab => "New tab".to_string(),
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
                    // Label only — render_separator in dialog.rs adds the ── dashes.
                    items.push(PickerItem::Section("agents".into()));
                    for (_, label) in &agent_matches {
                        items.push(PickerItem::Item((*label).to_string()));
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
                    .map(|dir| PickerItem::Item(dir.label().to_string()))
                    .collect();
                DialogRatatuiSnapshot::FilterPicker {
                    title: "Split direction".into(),
                    filter: filter.clone(),
                    items,
                    selected: *selected,
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
                    .map(|(_, label)| PickerItem::Item((*label).to_string()))
                    .collect();
                DialogRatatuiSnapshot::FilterPicker {
                    title: "Close".into(),
                    filter: filter.clone(),
                    items,
                    selected: *selected,
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
                    .map(|n| format!("Provider: {n}"))
                    .unwrap_or_else(|| "Provider".into());
                let items: Vec<PickerItem> = providers
                    .iter()
                    .map(|p| PickerItem::Item(p.label.clone()))
                    .collect();
                DialogRatatuiSnapshot::FilterPicker {
                    title,
                    filter: String::new(),
                    items,
                    selected: *selected,
                }
            }

            Dialog::RenameTab { input, .. } => DialogRatatuiSnapshot::TextInputDialog {
                dialog_title: "Rename tab".into(),
                label: "Name".into(),
                value: input.value().to_string(),
                cursor: input.cursor(),
            },

            Dialog::ContainerInfo { .. } => DialogRatatuiSnapshot::DebugInfo(
                self.container_info_state()
                    .expect("container_info_state is Some for ContainerInfo"),
            ),

            Dialog::GitHubContext { copied } => {
                let branch = pr_branch
                    .map(String::from)
                    .unwrap_or_else(|| "(unknown)".into());
                let loading_placeholder = if pr_loading { "resolving…" } else { "(none)" };
                let pr_number = pr_info
                    .map(|p| p.number_label())
                    .unwrap_or_else(|| loading_placeholder.to_string());
                let pr_title = pr_info
                    .map(|p| p.title.clone())
                    .unwrap_or_else(|| loading_placeholder.to_string());
                let pr_url = pr_info
                    .map(|p| p.url.clone())
                    .unwrap_or_else(|| loading_placeholder.to_string());
                let ci = pr_info
                    .and_then(|p| p.checks.as_ref())
                    .map(|c| c.summary())
                    .unwrap_or_else(|| {
                        if pr_loading {
                            "resolving…"
                        } else {
                            "(unknown)"
                        }
                        .to_string()
                    });
                DialogRatatuiSnapshot::InfoRows {
                    dialog_title: "GitHub context".into(),
                    rows: vec![
                        ("Branch".into(), branch),
                        ("Pull Request".into(), pr_number),
                        ("PR Title".into(), pr_title),
                        ("GitHub URL".into(), pr_url),
                        ("CI Status".into(), ci),
                    ],
                    copy_row: Some(3),
                    copied: *copied,
                }
            }
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
        } => {
            render_confirm_action(frame, area, title, body, *selected_yes);
        }
        DialogRatatuiSnapshot::FilterPicker {
            title,
            filter,
            items,
            selected,
        } => {
            render_filter_picker(frame, area, title, filter, items, *selected);
        }
        DialogRatatuiSnapshot::TextInputDialog {
            dialog_title,
            label,
            value,
            cursor,
        } => {
            render_text_input_dialog(frame, area, dialog_title, label, value, *cursor);
        }
        DialogRatatuiSnapshot::InfoRows {
            dialog_title,
            rows,
            copy_row,
            copied,
        } => {
            render_info_rows_dialog(frame, area, dialog_title, rows, *copy_row, *copied);
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
) {
    let mut state = ConfirmState::new(format!("{title}\n\n{body}"));
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

    if inner.height < 2 {
        return;
    }

    // Filter input on row 0 (shared component).
    let filter_area = Rect { height: 1, ..inner };
    render_filter_input(frame, filter_area, filter);

    if inner.height < 3 {
        return;
    }

    // Items from row 2 onward (row 1 = separator gap). Section rows are dim;
    // item rows are white and let the shared render_picker_list paint the
    // selected-row highlight (green background, ▸ cursor) + scroll thumb.
    let list_area = Rect {
        y: inner.y + 2,
        height: inner.height.saturating_sub(2),
        ..inner
    };

    let list_items: Vec<ratatui::widgets::ListItem<'_>> = items
        .iter()
        .map(|item| match item {
            PickerItem::Section(label) => ratatui::widgets::ListItem::new(Line::from(
                Span::styled(format!(" {label}"), jackin_tui::theme::DIM),
            )),
            PickerItem::Item(label) => ratatui::widgets::ListItem::new(Line::from(Span::styled(
                label.clone(),
                Style::default().fg(WHITE),
            ))),
        })
        .collect();

    jackin_tui::components::render_picker_list(
        list_area,
        frame.buffer_mut(),
        list_items,
        Some(selected),
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
    // Shared modal panel: PHOSPHOR_GREEN focused border, matching the menu /
    // pickers and the rest of jackin's dialogs.
    let title_str = format!(" {dialog_title} ");
    let block = jackin_tui::components::Panel::new()
        .title(&title_str)
        .focus(jackin_tui::components::PanelFocus::Focused)
        .block();
    let inner = block.inner(area);
    Clear.render(area, frame.buffer_mut());
    block.render(area, frame.buffer_mut());

    if inner.height < 3 {
        return;
    }

    let label_area = Rect { height: 1, ..inner };
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{label}: "),
            jackin_tui::theme::BOLD_WHITE,
        )),
        label_area,
    );

    let input_area = Rect {
        y: inner.y + 2,
        height: 1,
        ..inner
    };

    // Show text before cursor in green, cursor cell as inverse block,
    // text after cursor dim — mirrors the shared TextField rendering.
    let cursor_byte = cursor.min(value.len());
    let before = &value[..cursor_byte];
    let tail = &value[cursor_byte..];
    let (at_cursor, after): (String, &str) = if let Some(c) = tail.chars().next() {
        let byte_len = c.len_utf8();
        (c.to_string(), &tail[byte_len..])
    } else {
        (" ".to_string(), "")
    };

    let spans = vec![
        Span::styled(before, jackin_tui::theme::GREEN),
        Span::styled(
            at_cursor,
            Style::default()
                .fg(Color::Black)
                .bg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(after, jackin_tui::theme::DIM),
    ];
    frame.render_widget(Paragraph::new(Line::from(spans)), input_area);
}

fn render_info_rows_dialog(
    frame: &mut Frame<'_>,
    area: Rect,
    dialog_title: &str,
    rows: &[(String, String)],
    copy_row: Option<usize>,
    copied: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            format!(" {dialog_title} "),
            jackin_tui::theme::BOLD_WHITE,
        ));
    let inner = block.inner(area);
    Clear.render(area, frame.buffer_mut());
    block.render(area, frame.buffer_mut());

    let label_width = rows
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(0);

    for (i, (label, value)) in rows.iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }
        let row_area = Rect {
            y: inner.y + i as u16,
            height: 1,
            ..inner
        };

        let is_copy_row = copy_row == Some(i);
        let suffix = if is_copy_row && copied {
            " ✓ Copied!"
        } else {
            ""
        };

        let padded_label = format!("{label:<width$}", width = label_width);
        let line = if is_copy_row {
            Line::from(vec![
                Span::styled(format!("{padded_label}: "), jackin_tui::theme::DIM),
                Span::styled(format!("{value}{suffix}"), jackin_tui::theme::BOLD_WHITE),
            ])
        } else {
            Line::from(vec![
                Span::styled(format!("{padded_label}: "), jackin_tui::theme::DIM),
                Span::styled(value.as_str(), Style::default().fg(WHITE)),
            ])
        };
        frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), row_area);
    }
}
