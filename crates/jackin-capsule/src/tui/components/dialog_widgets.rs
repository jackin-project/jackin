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

use jackin_tui::components::TabStrip;
use jackin_tui::components::confirm_dialog::{ConfirmState, render_confirm_dialog};
use jackin_tui::components::filter_input::render_filter_input;
use jackin_tui::theme::{DIM, PHOSPHOR_GREEN, WHITE};

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
    /// Usage overlay, rendered from the same scrollable row model as `DebugInfo`
    /// but laid out as CodexBar-style sections instead of generic key/value
    /// diagnostics.
    UsageInfo {
        state: jackin_tui::components::ContainerInfoState,
        tabs: Vec<(String, bool)>,
        tab_bar_focused: bool,
        hovered_tab: Option<usize>,
    },
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
    pub(crate) fn scroll_axes(&self, block_area: Rect) -> jackin_tui::components::ScrollAxes {
        match self {
            Self::DebugInfo(state) => jackin_tui::components::dialog_scroll_axes(
                state.content_width(),
                state.content_height(),
                block_area,
            ),
            Self::UsageInfo { state, tabs, .. } => {
                let (width, height) = usage_info_content_size(state);
                let tab_width = usage_tab_strip_width(tabs);
                let width = width.max(tab_width);
                let height = height.saturating_add(2);
                jackin_tui::components::dialog_scroll_axes(width, height, block_area)
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

fn render_usage_info(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &jackin_tui::components::ContainerInfoState,
    tabs: &[(String, bool)],
    tab_bar_focused: bool,
    hovered_tab: Option<usize>,
) {
    let title = usage_panel_title(state, area.width);
    let inner = jackin_tui::components::render_dialog_shell(frame, area, Some(title.as_str()));
    if inner.height == 0 {
        return;
    }
    let tab_area = usage_tab_strip_area(inner, tabs);
    let tab_refs = tabs
        .iter()
        .map(|(label, active)| (label.as_str(), *active))
        .collect::<Vec<_>>();
    TabStrip::new(&tab_refs)
        .focused(tab_bar_focused)
        .hovered(hovered_tab)
        .render(frame, tab_area);
    let body_y = inner.y.saturating_add(tab_area.height);
    let body = Rect {
        x: inner.x,
        y: body_y,
        width: inner.width,
        height: inner.height.saturating_sub(tab_area.height),
    };
    let lines = usage_info_lines_for_width(state, body.width);
    let mut scroll = state.scroll.clone();
    jackin_tui::components::render_scrollable_dialog_body(frame, area, body, &lines, &mut scroll);
}

pub(crate) fn usage_dialog_inner_area(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

pub(crate) fn usage_tab_strip_area(inner: Rect, tabs: &[(String, bool)]) -> Rect {
    let strip_width = usage_tab_strip_width(tabs)
        .saturating_sub(usize::from(jackin_tui::TAB_GAP))
        .min(usize::from(inner.width));
    let strip_offset = usize::from(inner.width).saturating_sub(strip_width) / 2;
    Rect {
        x: inner
            .x
            .saturating_add(u16::try_from(strip_offset).unwrap_or(u16::MAX)),
        y: inner.y,
        width: u16::try_from(strip_width)
            .unwrap_or(inner.width)
            .max(1)
            .min(inner.width),
        height: inner.height.min(2),
    }
}

pub(crate) fn usage_tab_strip_index_at(
    tabs: &[(String, bool)],
    tab_area: Rect,
    col: u16,
) -> Option<usize> {
    let tab_refs = tabs
        .iter()
        .map(|(label, active)| (label.as_str(), *active))
        .collect::<Vec<_>>();
    TabStrip::new(&tab_refs).hit_index_at(tab_area, col, tab_area.y)
}

pub(crate) fn usage_tab_strip_labels(
    view: &jackin_protocol::control::FocusedUsageView,
    selected: crate::tui::components::dialog::UsageDialogTab,
) -> Vec<(String, bool)> {
    let overview_active = selected == crate::tui::components::dialog::UsageDialogTab::Overview;
    let mut tabs = vec![("Overview".to_owned(), overview_active)];
    tabs.extend(view.tabs.iter().map(|tab| {
        (
            usage_provider_display_label(&tab.label).to_owned(),
            !overview_active && tab.active,
        )
    }));
    tabs
}

pub(crate) fn usage_provider_display_label(label: &str) -> &str {
    match label {
        "Codex" | "OpenAI / Codex" => "OpenAI",
        "Claude" | "Anthropic / Claude" => "Anthropic",
        "Grok Build" | "xAI / Grok" => "xAI",
        "GLM / Z.AI" => "Z.AI",
        other => other,
    }
}

fn usage_tab_strip_width(tabs: &[(String, bool)]) -> usize {
    let gap = usize::from(jackin_tui::TAB_GAP);
    tabs.iter()
        .map(|(label, _)| jackin_tui::display_cols(label) + 2 + gap)
        .sum()
}

/// Panel title. In the narrow list layout the provider-detail panel reads
/// `Usage: <provider>` (matching the narrow preview); the wide layout and the
/// Overview/Instance panels keep their own titles.
fn usage_panel_title(state: &jackin_tui::components::ContainerInfoState, width: u16) -> String {
    let base = state.title();
    if width >= 68 || base != "Usage" {
        return base.to_owned();
    }
    if let Some(header) = usage_row_value(state, "Header") {
        let short = header.rsplit(" / ").next().unwrap_or(header).trim();
        if !short.is_empty() {
            return format!("Usage: {short}");
        }
    }
    base.to_owned()
}

pub(crate) fn usage_info_required_height(
    state: &jackin_tui::components::ContainerInfoState,
) -> u16 {
    u16::try_from(usage_info_lines(state).len())
        .unwrap_or(u16::MAX)
        .saturating_add(5)
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
    // Width 0 disables right-alignment so content-size/height measurement
    // reflects the intrinsic line width, not a padded-to-panel width.
    usage_info_lines_impl(state, false, 0)
}

fn usage_info_lines_for_width(
    state: &jackin_tui::components::ContainerInfoState,
    width: u16,
) -> Vec<Line<'static>> {
    usage_info_lines_impl(state, width < 64, width)
}

fn usage_info_lines_impl(
    state: &jackin_tui::components::ContainerInfoState,
    list_layout: bool,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(state.rows().len().saturating_mul(2).saturating_add(1));
    let context = UsageLineContext {
        updated: usage_row_value(state, "Updated"),
        account: usage_row_value(state, "Account"),
        plan: usage_row_value(state, "Plan"),
        list_layout,
        width: width as usize,
    };
    if list_layout {
        lines.push(Line::from(""));
    } else {
        lines.push(usage_separator_line(context.width));
    }
    for row in state.rows() {
        usage_lines_for_row(row.label(), row.value(), context, &mut lines);
    }
    lines
}

#[derive(Clone, Copy)]
struct UsageLineContext<'a> {
    updated: Option<&'a str>,
    account: Option<&'a str>,
    plan: Option<&'a str>,
    list_layout: bool,
    /// Panel inner width for right-aligned header fields; 0 disables alignment.
    width: usize,
}

const USAGE_CONTENT_PAD_LEFT: usize = 2;
const USAGE_CONTENT_PAD_RIGHT: usize = 2;
const USAGE_METER_FILLED: char = '█';
const USAGE_METER_EMPTY: char = '░';

fn usage_content_width(width: usize) -> usize {
    if width == 0 {
        return 0;
    }
    width
        .saturating_sub(USAGE_CONTENT_PAD_LEFT + USAGE_CONTENT_PAD_RIGHT)
        .max(1)
}

fn usage_content_indent() -> Span<'static> {
    Span::raw(" ".repeat(USAGE_CONTENT_PAD_LEFT))
}

fn usage_meter_char(ch: char) -> bool {
    matches!(ch, USAGE_METER_FILLED | USAGE_METER_EMPTY | '·')
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
    context: UsageLineContext<'_>,
    lines: &mut Vec<Line<'static>>,
) {
    match label {
        "Header" => {
            usage_header_lines(
                value,
                context.updated,
                context.account,
                context.plan,
                context.width,
                lines,
            );
        }
        "Focused agent" | "Focused account" => {
            lines.push(Line::from(vec![
                usage_content_indent(),
                Span::styled(
                    value.to_owned(),
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        "Provider" | "Account" | "Plan" | "Status" | "Updated" | "Focused" | "Started"
        | "Today" | "Since start" => {}
        bucket if is_quota_bucket_row(bucket, value) => {
            if context.list_layout {
                usage_quota_bucket_compact_lines(bucket, value, context.width, lines);
            } else {
                usage_quota_bucket_lines(bucket, value, context.width, lines);
            }
        }
        _ if is_overview_provider_label(label) => {
            usage_overview_provider_lines(label, value, context.width, lines);
        }
        _ if is_overview_provider_row(value) => {
            usage_legacy_overview_provider_lines(label, value, lines);
        }
        _ => lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{label} "), DIM),
            Span::styled(value.to_owned(), Style::default().fg(WHITE)),
        ])),
    }
}

fn is_overview_provider_row(value: &str) -> bool {
    value.split(" || ").count() == 3
}

fn is_overview_provider_label(label: &str) -> bool {
    matches!(
        label,
        "OpenAI" | "Anthropic" | "Amp" | "xAI" | "Z.AI" | "Kimi" | "MiniMax"
    )
}

fn usage_legacy_overview_provider_lines(label: &str, value: &str, lines: &mut Vec<Line<'static>>) {
    let parts = value.split(" || ").collect::<Vec<_>>();
    if parts.len() != 3 {
        return;
    }
    let account = parts[0];
    let plan = parts[1];
    let status = parts[2];
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(
            label.to_owned(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(account.to_owned(), Style::default().fg(WHITE)),
        Span::raw("  "),
        Span::styled(plan.to_owned(), DIM),
    ]));
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(USAGE_CONTENT_PAD_LEFT + 2)),
        Span::styled(status.to_owned(), DIM),
    ]));
}

fn usage_overview_provider_lines(
    label: &str,
    value: &str,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let value = value.trim();
    let (summary, reset) = value.split_once(" · ").unwrap_or((value, ""));
    let left = if summary.ends_with("% left") {
        format!("{label:<11}{summary:>9}")
    } else {
        format!("{label:<11}{summary}")
    };
    let (reset, local_timestamp) = usage_overview_reset_columns(reset);
    let Some(local_timestamp) = local_timestamp else {
        lines.push(usage_header_two_column(
            &left,
            Style::default().fg(WHITE),
            reset,
            DIM,
            width,
        ));
        return;
    };
    let left_cols = jackin_tui::display_cols(&left);
    let reset_cols = jackin_tui::display_cols(reset);
    let local_cols = jackin_tui::display_cols(local_timestamp);
    let available = width.saturating_sub(USAGE_CONTENT_PAD_LEFT + USAGE_CONTENT_PAD_RIGHT);
    let left_gap = 3;
    let right_gap = available
        .checked_sub(left_cols + left_gap + reset_cols + local_cols)
        .filter(|gap| *gap >= 1)
        .unwrap_or(3);
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(left, Style::default().fg(WHITE)),
        Span::raw(" ".repeat(left_gap)),
        Span::styled(reset.to_owned(), DIM),
        Span::raw(" ".repeat(right_gap)),
        Span::styled(local_timestamp.to_owned(), DIM),
    ]));
}

fn usage_overview_reset_columns(reset: &str) -> (&str, Option<&str>) {
    let reset = reset.trim();
    if let Some((prefix, suffix)) = reset.rsplit_once(" (")
        && suffix.ends_with(')')
    {
        let timestamp = &reset[reset.len() - suffix.len() - 2..];
        return (prefix.trim(), Some(timestamp));
    }
    (reset, None)
}

fn usage_header_lines(
    value: &str,
    updated: Option<&str>,
    account: Option<&str>,
    plan: Option<&str>,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    // Line 1: provider flush-left, account flush-right (preview layout).
    let account = account.unwrap_or("account unavailable");
    lines.push(usage_header_two_column(
        value,
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        account,
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        width,
    ));

    // Line 2: "Updated …" flush-left, plan flush-right.
    let updated = updated.map(str::trim).filter(|value| !value.is_empty());
    let plan = plan.map(str::trim).filter(|value| !value.is_empty());
    if updated.is_some() || plan.is_some() {
        lines.push(usage_header_two_column(
            updated.unwrap_or(""),
            DIM,
            plan.unwrap_or(""),
            DIM,
            width,
        ));
    }
    lines.push(usage_separator_line(width));
}

/// Build a header line with `left` flush-left and `right` flush-right to
/// `width`. Falls back to a fixed three-space gap when `width` is 0 (the
/// measurement path) or too narrow to right-align without overlap.
fn usage_header_two_column(
    left: &str,
    left_style: Style,
    right: &str,
    right_style: Style,
    width: usize,
) -> Line<'static> {
    let left_cols = jackin_tui::display_cols(left);
    let right_cols = jackin_tui::display_cols(right);
    let gap = width
        .checked_sub(USAGE_CONTENT_PAD_LEFT + USAGE_CONTENT_PAD_RIGHT + left_cols + right_cols)
        .filter(|gap| *gap >= 1)
        .unwrap_or(3);
    let mut spans = vec![
        usage_content_indent(),
        Span::styled(left.to_owned(), left_style),
    ];
    if !right.is_empty() {
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(right.to_owned(), right_style));
    }
    Line::from(spans)
}

fn usage_quota_bucket_lines(
    label: &str,
    value: &str,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    if label == "Limit Reset Credits" {
        usage_limit_reset_credit_lines(value, width, lines);
        return;
    }

    let display_label = usage_bucket_display_label(label, value);
    if is_usage_separated_section(label) {
        push_usage_separator(lines, width);
    } else {
        push_usage_section_gap(lines);
    }
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(
            display_label,
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ]));

    let Some(first) = value.split(" · ").find(|part| !part.trim().is_empty()) else {
        return;
    };

    let (meter, remaining_label) = usage_meter_parts(first);
    if remaining_label.is_none() {
        lines.push(usage_header_two_column(
            first,
            Style::default().fg(WHITE),
            "",
            DIM,
            width,
        ));
        return;
    }

    let meter = usage_full_width_meter(meter, width);
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(meter, Style::default().fg(PHOSPHOR_GREEN)),
    ]));

    let details = usage_quota_bucket_detail_parts(label, value);
    let rows = if label == "Credits" {
        usage_credit_bucket_detail_rows(remaining_label.map(str::to_owned), &details)
    } else {
        usage_stacked_bucket_detail_rows(remaining_label.map(str::to_owned), &details)
    };
    for (left, right) in rows {
        lines.push(usage_header_two_column(
            &left,
            Style::default().fg(WHITE),
            &right,
            DIM,
            width,
        ));
    }
}

fn usage_credit_bucket_detail_rows(
    remaining_label: Option<String>,
    details: &[String],
) -> Vec<(String, String)> {
    let left = remaining_label.unwrap_or_default();
    let right = details
        .iter()
        .find(|detail| **detail != left)
        .cloned()
        .unwrap_or_default();
    vec![(left, right)]
        .into_iter()
        .filter(|(left, right)| !left.is_empty() || !right.is_empty())
        .collect()
}

fn usage_limit_reset_credit_lines(value: &str, width: usize, lines: &mut Vec<Line<'static>>) {
    push_usage_separator(lines, width);
    let parts = value
        .split(" · ")
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    let right = parts.first().copied().unwrap_or_default();
    lines.push(usage_header_two_column(
        "Limit Reset Credits",
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        right,
        DIM,
        width,
    ));
    for detail in parts.iter().skip(1) {
        lines.push(usage_header_two_column(
            detail,
            Style::default().fg(WHITE),
            "",
            DIM,
            width,
        ));
    }
}

fn usage_bucket_display_label(label: &str, value: &str) -> String {
    if label == "Individual credits" && value.starts_with("Individual credits: ") {
        "Credits".to_owned()
    } else {
        label.to_owned()
    }
}

fn is_usage_separated_section(label: &str) -> bool {
    matches!(
        label,
        "Credits" | "Individual credits" | "Limit Reset Credits"
    )
}

fn push_usage_section_gap(lines: &mut Vec<Line<'static>>) {
    if lines
        .last()
        .is_none_or(|line| !usage_line_is_blank(line) && !usage_line_is_separator(line))
    {
        lines.push(Line::from(""));
    }
}

fn push_usage_separator(lines: &mut Vec<Line<'static>>, width: usize) {
    if !lines.last().is_some_and(usage_line_is_separator) {
        lines.push(usage_separator_line(width));
    }
}

fn usage_separator_line(width: usize) -> Line<'static> {
    let target = width.max(1);
    Line::from(vec![Span::styled("─".repeat(target), DIM)])
}

fn usage_line_is_blank(line: &Line<'_>) -> bool {
    line.spans
        .iter()
        .all(|span| span.content.as_ref().trim().is_empty())
}

fn usage_line_is_separator(line: &Line<'_>) -> bool {
    let text = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '─')
}

fn usage_full_width_meter(meter: &str, width: usize) -> String {
    let target = usage_content_width(width).max(1);
    let filled = meter.chars().filter(|ch| *ch == USAGE_METER_FILLED).count();
    let total = meter
        .chars()
        .filter(|ch| usage_meter_char(*ch))
        .count()
        .max(1);
    let filled_cols = if filled >= total {
        target
    } else {
        filled.saturating_mul(target) / total
    };
    let filled_cols = filled_cols.min(target);
    format!(
        "{}{}",
        USAGE_METER_FILLED.to_string().repeat(filled_cols),
        USAGE_METER_EMPTY
            .to_string()
            .repeat(target.saturating_sub(filled_cols))
    )
}

fn usage_stacked_bucket_detail_rows(
    remaining_label: Option<String>,
    details: &[String],
) -> Vec<(String, String)> {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut lasts_until_reset = false;
    if let Some(label) = remaining_label {
        left.push(label);
    }
    for detail in details {
        if detail.starts_with("Resets") || detail.starts_with("Runs out") {
            right.push(detail.clone());
        } else if !left.iter().any(|existing| existing == detail) {
            if detail == "On pace" || detail.ends_with(" in reserve") {
                lasts_until_reset = true;
            }
            left.push(detail.clone());
        }
    }
    if lasts_until_reset
        && right.iter().any(|detail| detail.starts_with("Resets"))
        && !right.iter().any(|detail| detail.starts_with("Runs out"))
    {
        right.push("Lasts until reset".to_owned());
    } else if right.is_empty() && left.len() > 1 {
        right.push(String::new());
    }
    let len = left.len().max(right.len());
    (0..len)
        .map(|index| {
            (
                left.get(index).cloned().unwrap_or_default(),
                right.get(index).cloned().unwrap_or_default(),
            )
        })
        .filter(|(left, right)| !left.is_empty() || !right.is_empty())
        .collect()
}

fn usage_quota_bucket_compact_lines(
    label: &str,
    value: &str,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let details = usage_quota_bucket_detail_parts(label, value);
    let detail = if details.is_empty() {
        "status unavailable".to_owned()
    } else {
        // Narrow layout keeps only remaining + reset (e.g. "37% left · Resets
        // in 1h 21m"); pace and other tokens drop out to fit the width.
        let remaining = details.first().cloned();
        let reset = details.iter().find(|part| part.contains("Resets")).cloned();
        let kept = remaining.into_iter().chain(reset).collect::<Vec<_>>();
        if kept.is_empty() {
            details.join(" · ")
        } else {
            kept.join(" · ")
        }
    };
    let detail = compact_bucket_detail_for_width(label, &detail, width);
    lines.push(Line::from(vec![
        usage_content_indent(),
        Span::styled(
            label.to_owned(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", DIM),
        Span::styled(detail, Style::default().fg(WHITE)),
    ]));
}

fn compact_bucket_detail_for_width(label: &str, detail: &str, width: usize) -> String {
    if width == 0 {
        return detail.to_owned();
    }
    let prefix_cols = 2 + jackin_tui::display_cols(label) + 2;
    let Some(detail_cols) = width.checked_sub(prefix_cols) else {
        return String::new();
    };
    truncate_display_with_ellipsis(detail, detail_cols)
}

fn truncate_display_with_ellipsis(value: &str, width: usize) -> String {
    if jackin_tui::display_cols(value) <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_owned();
    }
    format!("{}…", jackin_tui::take_display_cols(value, width - 1))
}

fn usage_quota_bucket_detail_parts(label: &str, value: &str) -> Vec<String> {
    let parts = value
        .split(" · ")
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Vec::new();
    }

    let (_meter, remaining_label) = usage_meter_parts(parts[0]);
    let details = remaining_label
        .into_iter()
        .chain(parts.iter().skip(1).copied())
        .flat_map(|detail| detail.split(" · "))
        .filter(|detail| !detail.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if label == "Extra usage" {
        usage_extra_usage_details(details)
    } else {
        details
    }
}

fn usage_extra_usage_details(details: Vec<String>) -> Vec<String> {
    let mut used = Vec::new();
    let mut rest = Vec::new();
    for detail in details {
        if detail.ends_with("% used") {
            used.push(detail);
        } else {
            rest.push(detail);
        }
    }
    used.extend(rest);
    used
}

fn usage_meter_parts(value: &str) -> (&str, Option<&str>) {
    value
        .split_once(' ')
        .filter(|(meter, _)| meter.chars().all(usage_meter_char))
        .map_or((value, None), |(meter, label)| (meter, Some(label)))
}

fn is_quota_bucket_row(label: &str, value: &str) -> bool {
    is_known_quota_bucket(label) || quota_value_has_meter(value)
}

fn is_known_quota_bucket(label: &str) -> bool {
    matches!(
        label,
        "Session"
            | "Weekly"
            | "Credits"
            | "Sonnet"
            | "Opus"
            | "Daily Routines"
            | "Extra usage"
            | "Tokens"
            | "MCP"
            | "5-hour"
            | "Amp Free"
            | "Individual credits"
            | "Limit Reset Credits"
            | "Rate Limit"
    ) || label.starts_with("Codex Spark")
        || label.ends_with("rate limit")
        || label.ends_with("Coding plan")
}

fn quota_value_has_meter(value: &str) -> bool {
    value
        .split_once(' ')
        .is_some_and(|(meter, _)| meter.chars().all(usage_meter_char))
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

#[cfg(test)]
mod tests {
    use super::*;
    use jackin_tui::components::{ContainerInfoRow, ContainerInfoState};

    fn usage_state() -> ContainerInfoState {
        ContainerInfoState::new(
            "Usage",
            vec![
                ContainerInfoRow::new("Header", "OpenAI / Codex"),
                ContainerInfoRow::new("Account", "alexey@example.com"),
                ContainerInfoRow::new("Plan", "Pro 20x"),
                ContainerInfoRow::new("Updated", "Updated 2m ago"),
                ContainerInfoRow::new(
                    "Session",
                    "███████······· 50% left · 50% used · Resets at 15:00 UTC · On pace",
                ),
                ContainerInfoRow::new(
                    "Weekly",
                    "███████████··· 80% left · 20% used · Resets on Friday 10:00 UTC · 25% in reserve",
                ),
            ],
        )
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn usage_overlay_lines_fit_responsive_widths() {
        let state = usage_state();
        for width in [44, 64, 96] {
            let lines = usage_info_lines_for_width(&state, width);
            for line in &lines {
                let cols = usage_line_width(line);
                assert!(
                    cols <= usize::from(width),
                    "line exceeds width {width}: {cols} cols: {:?}",
                    line_text(line)
                );
            }
        }
    }

    #[test]
    fn usage_overlay_wide_layout_keeps_header_and_full_width_remaining_meters() {
        let state = usage_state();
        let width = 96;
        let lines = usage_info_lines_for_width(&state, width);
        let text = lines.iter().map(line_text).collect::<Vec<_>>();

        assert!(
            text.iter()
                .any(|line| line.contains("OpenAI / Codex") && line.contains("alexey@example.com")),
            "provider/account header missing: {text:?}"
        );
        assert!(
            text.iter()
                .any(|line| line.contains("Updated 2m ago") && line.contains("Pro 20x")),
            "freshness/plan header missing: {text:?}"
        );

        let meter = text
            .iter()
            .find(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && trimmed.chars().all(|ch| matches!(ch, '█' | '░'))
            })
            .expect("full-width quota meter");
        assert_eq!(
            jackin_tui::display_cols(meter.trim()),
            usage_content_width(usize::from(width)),
            "meter should span the padded dialog body width: {meter:?}"
        );
    }

    #[test]
    fn usage_overlay_narrow_layout_keeps_compact_bucket_details() {
        let state = usage_state();
        let lines = usage_info_lines_for_width(&state, 44);
        let text = lines.iter().map(line_text).collect::<Vec<_>>();

        assert!(
            text.iter()
                .any(|line| line.contains("Session") && line.contains("50% left")),
            "narrow layout should keep session remaining: {text:?}"
        );
        assert!(
            text.iter()
                .any(|line| line.contains("Session") && line.contains("Resets at 15:00 UTC")),
            "narrow layout should keep reset detail: {text:?}"
        );
        assert!(
            !text.iter().any(|line| line.contains("On pace")),
            "narrow layout should drop pace detail to fit: {text:?}"
        );
    }
}
