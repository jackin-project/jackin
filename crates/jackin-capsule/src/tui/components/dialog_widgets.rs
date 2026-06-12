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
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Widget};

use jackin_tui::components::confirm_dialog::{ConfirmState, render_confirm_dialog};
use jackin_tui::components::filter_input::render_filter_input;
use jackin_tui::theme::{BOLD_GREEN, DIM, PHOSPHOR_GREEN, WHITE};

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
    /// Usage overlay, rendered from the same scrollable row model as `DebugInfo`
    /// but laid out as CodexBar-style sections instead of generic key/value
    /// diagnostics.
    UsageInfo(jackin_tui::components::ContainerInfoState),
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
            Dialog::ConfirmAction { kind, selected_yes } => DialogRatatuiSnapshot::ConfirmAction {
                title: kind.title().to_owned(),
                body: kind.message().to_owned(),
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

            Dialog::Usage { .. } => DialogRatatuiSnapshot::UsageInfo(
                self.usage_state().expect("usage_state is Some for Usage"),
            ),
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
            Self::DebugInfo(state) | Self::UsageInfo(state) => {
                if matches!(self, Self::UsageInfo(_)) {
                    let (width, height) = usage_info_content_size(state);
                    return jackin_tui::components::dialog_scroll_axes(width, height, block_area);
                }
                jackin_tui::components::dialog_scroll_axes(
                    state.content_width(),
                    state.content_height(),
                    block_area,
                )
            }
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
        } => {
            render_confirm_action(frame, area, title, body, *selected_yes);
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
        DialogRatatuiSnapshot::UsageInfo(state) => {
            render_usage_info(frame, area, state);
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

fn render_usage_info(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &jackin_tui::components::ContainerInfoState,
) {
    let inner = jackin_tui::components::render_dialog_shell(frame, area, Some(state.title()));
    let lines = usage_info_lines(state);
    let mut scroll = state.scroll.clone();
    jackin_tui::components::render_scrollable_dialog_body(frame, area, inner, &lines, &mut scroll);
}

pub(crate) fn usage_info_required_height(
    state: &jackin_tui::components::ContainerInfoState,
) -> u16 {
    u16::try_from(usage_info_lines(state).len())
        .unwrap_or(u16::MAX)
        .saturating_add(3)
        .max(7)
}

pub(crate) fn usage_info_content_size(
    state: &jackin_tui::components::ContainerInfoState,
) -> (usize, usize) {
    let lines = usage_info_lines(state);
    let width = lines.iter().map(usage_line_width).max().unwrap_or(0);
    let height = lines.len();
    (width, height)
}

fn usage_info_lines(state: &jackin_tui::components::ContainerInfoState) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(state.rows().len().saturating_mul(2).saturating_add(1));
    let updated = usage_row_value(state, "Updated");
    let account = usage_row_value(state, "Account");
    let plan = usage_row_value(state, "Plan");
    let latest_tokens = usage_row_value(state, "Latest tokens");
    lines.push(Line::from(""));
    for row in state.rows() {
        usage_lines_for_row(
            row.label(),
            row.value(),
            updated,
            account,
            plan,
            latest_tokens,
            &mut lines,
        );
    }
    lines
}

fn usage_row_value<'a>(
    state: &'a jackin_tui::components::ContainerInfoState,
    label: &str,
) -> Option<&'a str> {
    state
        .rows()
        .iter()
        .find(|row| row.label() == label)
        .map(jackin_tui::components::ContainerInfoRow::value)
}

fn usage_line_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| jackin_tui::display_cols(span.content.as_ref()))
        .sum()
}

fn usage_lines_for_row(
    label: &str,
    value: &str,
    updated: Option<&str>,
    account: Option<&str>,
    plan: Option<&str>,
    latest_tokens: Option<&str>,
    lines: &mut Vec<Line<'static>>,
) {
    match label {
        "Tabs" => lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(value.to_owned(), BOLD_GREEN),
        ])),
        "Header" => usage_header_lines(value, updated, account, plan, lines),
        "Focused agent" | "Focused account" | "Instance" => {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    value.to_owned(),
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        "Account availability"
        | "Account cost and tokens"
        | "Instance spend"
        | "By agent codename"
        | "By provider/account" => {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(label.to_owned(), BOLD_GREEN),
            ]));
        }
        "Cost row" | "Token row" | "Spend row" | "Cost rows" => {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(value.to_owned(), Style::default().fg(WHITE)),
            ]));
        }
        "Tokens since start" => {
            let mut details = format!("Tokens since start {value}");
            if let Some(latest) = latest_tokens.filter(|value| !value.trim().is_empty()) {
                details.push_str("   Latest tokens ");
                details.push_str(latest);
            }
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(details, Style::default().fg(WHITE)),
            ]));
        }
        "History" => {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(value.to_owned(), Style::default().fg(PHOSPHOR_GREEN)),
            ]));
        }
        "Top model" => lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("Top model: ", DIM),
            Span::styled(value.to_owned(), Style::default().fg(WHITE)),
        ])),
        "Captured" | "Provenance" | "Source" | "Refresh" => {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{label}: "), DIM),
                Span::styled(value.to_owned(), Style::default().fg(WHITE)),
            ]));
        }
        "Provider status" => {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("Status: ", DIM),
                Span::styled(value.to_owned(), Style::default().fg(WHITE)),
            ]));
        }
        "Actions" => {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(value.to_owned(), DIM),
            ]));
        }
        "Cost" | "Subscription Utilization" | "Usage Dashboard" | "Status Page" => {
            usage_menu_row(label, value, lines);
        }
        "Provider" | "Account" | "Plan" | "Status" | "Updated" | "Focused" | "Started"
        | "Today" | "Since start" | "Today cost" | "30d cost" | "30d tokens" | "Latest tokens" => {}
        "Age" => lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("Started ", DIM),
            Span::styled(format!("{value} ago"), Style::default().fg(WHITE)),
        ])),
        "Active agent time" => lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("Active agent time ", DIM),
            Span::styled(value.to_owned(), Style::default().fg(WHITE)),
        ])),
        bucket if is_known_quota_bucket(bucket) => {
            usage_quota_bucket_lines(bucket, value, lines);
        }
        _ if is_instance_agent_row(value) => usage_instance_agent_lines(label, value, lines),
        _ if is_instance_provider_account_row(value) => {
            usage_instance_provider_account_lines(label, value, lines);
        }
        _ => lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{label} "), DIM),
            Span::styled(value.to_owned(), Style::default().fg(WHITE)),
        ])),
    }
}

fn usage_menu_row(label: &str, value: &str, lines: &mut Vec<Line<'static>>) {
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(label.to_owned(), Style::default().fg(WHITE)),
        Span::styled("  ", DIM),
        Span::styled(value.to_owned(), DIM),
        Span::styled("  >", DIM),
    ]));
}

fn is_instance_provider_account_row(value: &str) -> bool {
    value.contains(" since start") && value.contains(" tokens")
}

fn usage_instance_provider_account_lines(label: &str, value: &str, lines: &mut Vec<Line<'static>>) {
    let Some((account, summary)) = value.split_once(" · ") else {
        return;
    };
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            label.to_owned(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(account.to_owned(), Style::default().fg(WHITE)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(summary.to_owned(), Style::default().fg(WHITE)),
    ]));
}

fn is_instance_agent_row(value: &str) -> bool {
    let parts = value.split(" · ").count();
    parts >= 10 && value.contains(" · top ")
}

fn usage_instance_agent_lines(label: &str, value: &str, lines: &mut Vec<Line<'static>>) {
    let parts = value.split(" · ").collect::<Vec<_>>();
    if parts.len() < 10 {
        return;
    }
    let top_model = parts[parts.len() - 1];
    let lifecycle = parts[parts.len() - 2];
    let summary = parts[7..parts.len() - 2].join(" · ");
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            label.to_owned(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(parts[0].to_owned(), Style::default().fg(WHITE)),
        Span::raw("  "),
        Span::styled(parts[1].to_owned(), DIM),
        Span::raw("  "),
        Span::styled(parts[2].to_owned(), Style::default().fg(WHITE)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(parts[3].to_owned(), DIM),
        Span::raw("  "),
        Span::styled(parts[4].to_owned(), DIM),
        Span::raw("  "),
        Span::styled(parts[5].to_owned(), DIM),
        Span::raw("  "),
        Span::styled(parts[6].to_owned(), DIM),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(summary, Style::default().fg(WHITE)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(top_model.to_owned(), Style::default().fg(WHITE)),
        Span::raw("  "),
        Span::styled(lifecycle.to_owned(), DIM),
    ]));
}

fn usage_header_lines(
    value: &str,
    updated: Option<&str>,
    account: Option<&str>,
    plan: Option<&str>,
    lines: &mut Vec<Line<'static>>,
) {
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            value.to_owned(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled("   ", DIM),
        Span::styled(
            account.unwrap_or("account unavailable").to_owned(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ]));

    let mut details = Vec::new();
    if let Some(updated) = updated.filter(|value| !value.trim().is_empty()) {
        details.push(updated.to_owned());
    }
    if let Some(plan) = plan.filter(|value| !value.trim().is_empty()) {
        details.push(plan.to_owned());
    }
    if !details.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(details.join("   "), DIM),
        ]));
    }
}

fn usage_quota_bucket_lines(label: &str, value: &str, lines: &mut Vec<Line<'static>>) {
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            label.to_owned(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ]));

    let parts = value
        .split(" · ")
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return;
    }

    let (meter, remaining_label) = usage_meter_parts(parts[0]);
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(meter.to_owned(), Style::default().fg(PHOSPHOR_GREEN)),
    ]));
    let details = remaining_label
        .into_iter()
        .chain(parts.iter().skip(1).copied())
        .collect::<Vec<_>>();
    if !details.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(details.join("   "), Style::default().fg(WHITE)),
        ]));
    }
}

fn usage_meter_parts(value: &str) -> (&str, Option<&str>) {
    value
        .split_once(' ')
        .filter(|(meter, _)| meter.chars().all(|ch| matches!(ch, '█' | '·')))
        .map_or((value, None), |(meter, label)| (meter, Some(label)))
}

fn is_known_quota_bucket(label: &str) -> bool {
    matches!(
        label,
        "Session"
            | "Weekly"
            | "Credits"
            | "Sonnet"
            | "Daily Routines"
            | "Extra usage"
            | "Token window"
    ) || label.starts_with("Codex Spark")
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
