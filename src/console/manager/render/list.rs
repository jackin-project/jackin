//! List-stage rendering: the left-column workspace list, right-pane
//! details (saved workspace / current-directory / "+ New workspace"
//! sentinel), and the transient toast overlay.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use super::super::state::{ManagerListRow, ManagerState, WorkspaceSummary};
use super::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE};
use crate::config::AppConfig;

pub(super) fn render_list_body(
    frame: &mut Frame,
    area: Rect,
    state: &ManagerState<'_>,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    // See ManagerListRow docs for row layout.
    let saved_count = state.workspaces.len();

    // Split driven by `state.list_split_pct` (default 30), adjustable via
    // mouse-drag on the seam column. Keeps the right pane visible on every
    // row. Row-specific right-pane renderers:
    //   CurrentDirectory  → current-dir details
    //   SavedWorkspace(i) → saved-workspace details
    //   NewWorkspace      → description-of-what-a-workspace-is pane
    let left_pct = state.list_split_pct;
    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    let list_area = columns[0];

    match state.selected_row() {
        ManagerListRow::CurrentDirectory => {
            render_current_dir_details_pane(frame, columns[1], cwd);
        }
        ManagerListRow::NewWorkspace => {
            render_sentinel_description_pane(frame, columns[1]);
        }
        ManagerListRow::SavedWorkspace(i) => {
            if let Some(ws) = state.workspaces.get(i) {
                render_details_pane(frame, columns[1], ws, config);
            }
        }
    }

    // Left: [Current directory] + saved workspaces + [+ New workspace].
    // The cwd path itself is shown on the right-pane `workdir` line; keep the
    // list row label short to avoid duplicate visual load.
    let mut items: Vec<ListItem> = Vec::with_capacity(saved_count + 2);
    items.push(ListItem::new(Line::from(Span::styled(
        "Current directory",
        Style::default().fg(WHITE),
    ))));
    items.extend(
        state
            .workspaces
            .iter()
            .map(|w| ListItem::new(Line::from(w.name.as_str()))),
    );
    items.push(ListItem::new(Line::from(Span::styled(
        "+ New workspace",
        Style::default().fg(WHITE),
    ))));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(PHOSPHOR_DARK)),
        )
        .style(Style::default().fg(PHOSPHOR_GREEN))
        .highlight_style(Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black))
        .highlight_symbol("▸ ");

    let mut ls = ListState::default();
    ls.select(Some(state.selected));
    frame.render_stateful_widget(list, list_area, &mut ls);

    // Toast overlay — rendered last so it appears on top.
    if let Some(toast) = &state.toast {
        render_toast(frame, area, toast);
    }
}

fn render_toast(frame: &mut Frame, area: Rect, toast: &super::super::state::Toast) {
    use super::super::state::ToastKind;
    let elapsed = toast.shown_at.elapsed();
    // Auto-expire after 3 seconds — caller should clear before that,
    // but defensively skip rendering if we're past.
    if elapsed > std::time::Duration::from_secs(3) {
        return;
    }

    let (prefix, color) = match toast.kind {
        ToastKind::Success => ("✓ ", PHOSPHOR_GREEN),
        ToastKind::Error => ("✗ ", Color::Rgb(255, 94, 122)),
    };
    let mut style = Style::default().fg(color).add_modifier(Modifier::BOLD);
    // Shimmer: first 400ms is bright-white flicker, then settles.
    if elapsed < std::time::Duration::from_millis(400) {
        style = style.fg(WHITE);
    }
    let line = Line::from(Span::styled(format!("{}{}", prefix, toast.message), style));
    let banner_area = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: 1,
    };
    frame.render_widget(ratatui::widgets::Clear, banner_area);
    frame.render_widget(Paragraph::new(line), banner_area);
}

/// Build aligned 3-column mount rows: (`path_display`, mode, `kind_label`).
pub(super) fn format_mount_rows(
    mounts: &[crate::workspace::MountConfig],
) -> Vec<(String, &'static str, String)> {
    mounts
        .iter()
        .map(|m| {
            let src = crate::tui::shorten_home(&m.src);
            let dst = crate::tui::shorten_home(&m.dst);
            let path = if m.src == m.dst {
                src
            } else {
                format!("{src} \u{2192} {dst}")
            };
            let mode: &'static str = if m.readonly { "ro" } else { "rw" };
            let kind = super::super::mount_info::inspect(&m.src).label();
            (path, mode, kind)
        })
        .collect()
}

/// Width of the `Mode` column. Pinned to the length of the header label
/// "Mode" (4 chars) so the data-row values "rw"/"ro" pad to the same width.
/// Without the pad, `rw`/`ro` (2 chars) would render 2 chars short of the
/// header's 4, shifting the `Type` column left by 2 on every data row.
/// Both the header and data rows emit a two-space gutter after this column
/// before the `Type` column so "Mode" and "Type" never run together.
pub(super) const MOUNT_MODE_COL_WIDTH: usize = 4;

/// Compute the width used for the `Path` column so that header and data rows
/// align. Derived from both the "Path" header label and the widest row path,
/// with a minimum floor so short-path tables still look tabular.
pub(super) fn mount_path_width(rows: &[(String, &str, String)]) -> usize {
    rows.iter()
        .map(|(p, _, _)| p.chars().count())
        .max()
        .unwrap_or(0)
        .max(10) // floor so a single-row mount still has a clear column
        .max("Path".len())
}

/// Header row for the mount table. `path_w` must come from
/// [`mount_path_width`] over the same `rows` used for the data lines so the
/// columns line up.
pub(super) fn render_mount_header(path_w: usize) -> Line<'static> {
    // Format: "  <path padded to path_w>  <mode padded>  Type"
    // Leading two-space gutter + two-space gap between Mode and Type both
    // match the data-row format — so "Mode" never runs into "Type".
    let mode_col = format!("{:<mw$}", "Mode", mw = MOUNT_MODE_COL_WIDTH);
    Line::from(Span::styled(
        format!("  {path:<path_w$}  {mode_col}  Type", path = "Path"),
        Style::default().fg(WHITE),
    ))
}

/// Render aligned mount rows as `Line`s (no selection prefix). `path_w` is
/// passed in so the header and data rows share the same column boundary.
pub(super) fn render_mount_lines(
    rows: &[(String, &str, String)],
    path_w: usize,
) -> Vec<Line<'static>> {
    rows.iter()
        .map(|(path, mode, kind)| {
            Line::from(vec![
                Span::raw(format!("  {path:<path_w$}  ")),
                Span::styled(
                    format!("{mode:<MOUNT_MODE_COL_WIDTH$}"),
                    Style::default().fg(PHOSPHOR_DIM),
                ),
                // Two-space gap before the type column — matches the header.
                Span::raw("  "),
                Span::styled(
                    kind.clone(),
                    Style::default()
                        .fg(PHOSPHOR_DIM)
                        .add_modifier(Modifier::ITALIC),
                ),
            ])
        })
        .collect()
}

fn render_details_pane(frame: &mut Frame, area: Rect, ws: &WorkspaceSummary, config: &AppConfig) {
    let ws_config = config.workspaces.get(&ws.name);
    let mounts = ws_config.map_or(&[][..], |w| w.mounts.as_slice());
    let agent_count = agents_block_agent_count(ws_config, config);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // General: workdir + 2 borders (Last used moved to Agents)
            Constraint::Length(mount_block_height(mounts)), // Mounts: header + N rows + 2 borders
            Constraint::Length(env_block_height(ws_config)), // Environments
            Constraint::Length(agents_block_height(agent_count)), // Agents: default + blank + names + 2 borders
        ])
        .split(area);

    render_general_subpanel(frame, rows[0], ws);
    render_mounts_subpanel(frame, rows[1], mounts);
    render_environments_subpanel(frame, rows[2], ws_config);
    render_agents_subpanel(frame, rows[3], ws_config, config);
}

/// Exact row count a Mounts sub-panel needs to render `mounts` without
/// leaving a phantom empty row inside the block. Layout: 2 borders + 1
/// header row + N data rows (minimum 1 for the "(none)" placeholder when
/// `mounts` is empty). Clamped to a reasonable maximum so a workspace with
/// many mounts can't eat the full right pane.
///
/// Shared by `render_details_pane` and `render_current_dir_details_pane`
/// so both produce identically-tight blocks.
fn mount_block_height(mounts: &[crate::workspace::MountConfig]) -> u16 {
    let data_rows = if mounts.is_empty() { 1 } else { mounts.len() };
    (data_rows + 2 + 1).min(12) as u16 // +1 header, +2 borders
}

/// Exact row count the Environments sub-panel needs given the workspace
/// config. Layout: 2 borders + workspace section (sub-header + N keys) +
/// per-agent sections (blank spacer + sub-header + M keys per agent
/// with overrides). When the workspace and all agent overrides are
/// empty, falls back to a single "(no environment variables)"
/// placeholder row. Clamped to a reasonable maximum so a workspace with
/// many env vars and override sections can't eat the full right pane.
fn env_block_height(ws_config: Option<&crate::workspace::WorkspaceConfig>) -> u16 {
    let Some(ws) = ws_config else {
        // No workspace config — placeholder branch (used by the cwd pane).
        return 3; // 1 placeholder + 2 borders
    };

    let workspace_keys = ws.env.len();
    let agents_with_overrides: Vec<usize> = ws
        .agents
        .values()
        .filter_map(|o| {
            if o.env.is_empty() {
                None
            } else {
                Some(o.env.len())
            }
        })
        .collect();

    if workspace_keys == 0 && agents_with_overrides.is_empty() {
        // Nothing to show — single placeholder line.
        return 3; // 1 placeholder + 2 borders
    }

    // 1 row for the `All agents:` sub-header + N workspace keys.
    let mut rows: usize = if workspace_keys == 0 {
        // Workspace has no env keys but at least one agent has overrides;
        // skip the `All agents:` section entirely and start with the
        // per-agent sections.
        0
    } else {
        1 + workspace_keys
    };
    // Each per-agent override section: blank spacer + sub-header + N keys.
    // The first agent section needs no leading blank if the workspace
    // section was skipped.
    for (i, n) in agents_with_overrides.iter().enumerate() {
        let leading_blank = usize::from(!(i == 0 && workspace_keys == 0));
        rows += leading_blank + 1 + n;
    }

    (rows + 2).min(20) as u16 // +2 borders, clamped
}

/// Number of agent rows the Agents block will render. Mirrors the
/// agent-listing rule in `render_agents_subpanel` so the block height
/// can be sized exactly.
fn agents_block_agent_count(
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
) -> usize {
    let all_allowed = ws_config.is_none_or(super::super::agent_allow::allows_all_agents);
    if all_allowed {
        config.agents.len()
    } else {
        ws_config.map_or(0, |w| w.allowed_agents.len())
    }
}

/// Exact row count the Agents sub-panel needs: 2 borders + 1 default
/// row + 1 blank spacer + N agent rows. Clamped to a reasonable max so
/// a globally-allowed workspace with many agents doesn't push the
/// Environments block off-screen.
fn agents_block_height(agent_count: usize) -> u16 {
    // Always reserve at least one row for the agent list area, even
    // when there are no agents — the block reads as broken otherwise.
    let agent_rows = agent_count.max(1);
    (2 + 1 + 1 + agent_rows).min(14) as u16
}

/// Right-pane details shown when the cursor is on the synthetic "Current
/// directory" row (row 0). Summarises the cwd workspace that would be
/// launched: workdir + the auto-mount derived from cwd + "any agent".
///
/// Keeps the General/Mounts/Agents three-block vertical layout of
/// `render_details_pane` so operators see a familiar shape, but uses
/// a dedicated General block (no "last used" row — not meaningful for
/// a non-persistent launch) with the " Current directory " title.
fn render_current_dir_details_pane(frame: &mut Frame, area: Rect, cwd: &std::path::Path) {
    let cwd_str = cwd.display().to_string();
    let workdir_short = crate::tui::shorten_home(&cwd_str);

    // The single auto-mount mirrors `workspace::current_dir_workspace`:
    // src = dst = cwd, rw. Built here as a local so we can pass it into
    // the shared `render_mounts_subpanel` helper.
    let mounts = [crate::workspace::MountConfig {
        src: cwd_str.clone(),
        dst: cwd_str,
        readonly: false,
    }];

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                           // General: workdir + 2 borders
            Constraint::Length(mount_block_height(&mounts)), // Mounts: header + N rows + 2 borders
            Constraint::Length(env_block_height(None)),      // Environments: empty placeholder
            Constraint::Length(agents_block_height(agents_block_agent_count(
                None,
                &AppConfig::default(),
            ))), // Agents: default + blank + per-agent name rows + 2 borders
        ])
        .split(area);

    // General — titled the same as the saved-workspace pane so the four
    // sub-panel titles (General / Mounts / Environments / Agents) match
    // across both panes. The "Current directory" signpost is already
    // visible as the left-list row label, so repeating it here was
    // redundant.
    let general_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " General ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    // Two-space prefix keeps the label aligned with Mounts and Agents — see
    // `render_general_subpanel` for the shared convention.
    let general_lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled("Working dir ", Style::default().fg(WHITE)),
        Span::raw(workdir_short),
    ])];
    frame.render_widget(
        Paragraph::new(general_lines)
            .block(general_block)
            .style(Style::default().fg(PHOSPHOR_GREEN)),
        rows[0],
    );

    render_mounts_subpanel(frame, rows[1], &mounts);
    // Environments block — synthetic cwd workspace has no env vars,
    // so this always renders the empty-state placeholder. Kept for
    // structural parity with the saved-workspace pane.
    render_environments_subpanel(frame, rows[2], None);

    // Agents block — reuse the no-`ws_config` branch of the shared renderer,
    // which lists every globally-configured agent (without per-agent
    // overrides since the cwd workspace has none).
    render_agents_subpanel(frame, rows[3], None, &AppConfig::default());
}

/// Right-pane description shown when the cursor is on the "+ New workspace"
/// sentinel. Explains what a workspace is and why the operator might create
/// one — compacted from `docs/src/content/docs/guides/workspaces.mdx`
/// sections "What is a workspace?" + "Why save a workspace?".
fn render_sentinel_description_pane(frame: &mut Frame, area: Rect) {
    // Two stacked sub-panels so the section titles render as block titles
    // with the same PHOSPHOR_DARK border used by General/Mounts/Agents.
    // The "What is a workspace?" intro is short (fits in 4 rows); the
    // rest of the area hosts the bullet list + closing hint.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // "What is a workspace?" intro (2 text rows + 2 borders + 1 pad)
            Constraint::Min(9),    // "Why create one?" bullets + hint
        ])
        .split(area);

    let intro_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " What is a workspace? ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let intro_lines = vec![
        Line::from(Span::styled(
            "  A workspace saves a project boundary once so you",
            Style::default().fg(PHOSPHOR_GREEN),
        )),
        Line::from(Span::styled(
            "  can launch agents into it from anywhere \u{2014} without",
            Style::default().fg(PHOSPHOR_GREEN),
        )),
        Line::from(Span::styled(
            "  retyping mount paths.",
            Style::default().fg(PHOSPHOR_GREEN),
        )),
    ];
    frame.render_widget(Paragraph::new(intro_lines).block(intro_block), rows[0]);

    let why_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Why create one? ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let bullet_style = Style::default().fg(PHOSPHOR_GREEN);
    let bullets = [
        "Name a project once, launch from any cwd",
        "Keep extra mounts consistent across sessions",
        "Reuse one boundary with different agent classes",
        "Set a default agent or restrict which classes apply",
        "Let `jackin console` auto-detect and preselect it",
    ];
    let mut why_lines: Vec<Line<'static>> = bullets
        .iter()
        .map(|b| Line::from(Span::styled(format!("  \u{2022} {b}"), bullet_style)))
        .collect();
    why_lines.push(Line::from(""));
    why_lines.push(Line::from(Span::styled(
        "  Press Enter to start the setup wizard.",
        Style::default()
            .fg(PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC),
    )));
    frame.render_widget(Paragraph::new(why_lines).block(why_block), rows[1]);
}

fn render_general_subpanel(frame: &mut Frame, area: Rect, ws: &WorkspaceSummary) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " General ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));

    // Each content row is prefixed with two spaces to match the Mounts and
    // Agents sub-panels (see `SUBPANEL_CONTENT_INDENT`). Without the prefix the
    // label sat flush against the block's left border, breaking column
    // alignment with the other two blocks in the same pane.
    //
    // The `Last used` row used to live here; it now sits at the top of the
    // Agents sub-panel where it semantically belongs (agent-identity data,
    // not path/workspace-identity data).
    let lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled("Working dir ", Style::default().fg(WHITE)),
        Span::raw(crate::tui::shorten_home(&ws.workdir)),
    ])];

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

/// Number of leading spaces every content row in the General / Mounts /
/// Agents sub-panels is prefixed with, so the first visible character lines
/// up across all three blocks (at `border_col + SUBPANEL_CONTENT_INDENT`).
/// Pinned by `subpanel_content_column_alignment` in the visual regression
/// tests.
#[cfg(test)]
const SUBPANEL_CONTENT_INDENT: usize = 2;

fn render_mounts_subpanel(frame: &mut Frame, area: Rect, mounts: &[crate::workspace::MountConfig]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Mounts ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));

    let mut lines: Vec<Line> = Vec::new();

    if mounts.is_empty() {
        // No data rows — fall back to the minimum column width so the header
        // still shows sensible column boundaries.
        lines.push(render_mount_header(mount_path_width(&[])));
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(PHOSPHOR_DIM),
        )));
    } else {
        // Plain-text labels — the operator uses the `o` key on a selected
        // mount row to open the GitHub URL in a real browser instead.
        let rows = format_mount_rows(mounts);
        let path_w = mount_path_width(&rows);
        lines.push(render_mount_header(path_w));
        lines.extend(render_mount_lines(&rows, path_w));
    }

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

/// Render an env-key list inside an existing block. Each key gets a
/// two-space indent (matching `SUBPANEL_CONTENT_INDENT`); op:// values
/// are tagged with a leading `[op] ` marker — never values, plain or
/// op://. Keys are taken in BTreeMap-iteration order (alphabetical).
fn env_key_lines(
    env: &std::collections::BTreeMap<String, String>,
    extra_indent: usize,
) -> Vec<Line<'static>> {
    let pad = " ".repeat(2 + extra_indent);
    env.iter()
        .map(|(key, value)| {
            if crate::operator_env::is_op_reference(value) {
                Line::from(vec![
                    Span::raw(pad.clone()),
                    Span::styled(
                        "[op] ",
                        Style::default()
                            .fg(PHOSPHOR_DIM)
                            .add_modifier(Modifier::ITALIC),
                    ),
                    Span::styled(key.clone(), Style::default().fg(PHOSPHOR_GREEN)),
                ])
            } else {
                Line::from(vec![
                    Span::raw(pad.clone()),
                    Span::styled(key.clone(), Style::default().fg(PHOSPHOR_GREEN)),
                ])
            }
        })
        .collect()
}

/// Right-pane Environments block — single block holding all env-related
/// content for the workspace. Layout:
///
/// ```text
///   All agents:
///     KEY_A
///     KEY_B
///     [op] KEY_C
///
///   <agent>:
///     KEY_D
///     [op] KEY_E
/// ```
///
/// Workspace-level keys (`WorkspaceConfig.env`) appear under the
/// `All agents:` sub-header. Each agent with non-empty overrides
/// (`WorkspaceAgentOverride.env`) gets its own sub-section, separated
/// from neighbours by a blank spacer line. Agents without overrides
/// are omitted entirely — their absence is the signal. When both the
/// workspace env map AND every agent override map are empty, the
/// block falls back to a single `(no environment variables)`
/// placeholder. Op:// values render with a leading `[op] ` marker.
/// Values themselves never appear — only key names.
fn render_environments_subpanel(
    frame: &mut Frame,
    area: Rect,
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Environments ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));

    let mut lines: Vec<Line> = Vec::new();

    let workspace_env = ws_config.map(|w| &w.env);
    // Collect (agent_name, env_map) pairs for agents with overrides,
    // skipping empty ones. BTreeMap iteration is alphabetical.
    let agent_sections: Vec<(&str, &std::collections::BTreeMap<String, String>)> = ws_config
        .map(|w| {
            w.agents
                .iter()
                .filter(|(_, o)| !o.env.is_empty())
                .map(|(name, o)| (name.as_str(), &o.env))
                .collect()
        })
        .unwrap_or_default();

    let workspace_has_keys = workspace_env.is_some_and(|m| !m.is_empty());
    let any_content = workspace_has_keys || !agent_sections.is_empty();

    if any_content {
        if workspace_has_keys {
            // `All agents:` sub-header + workspace key list.
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "All agents:",
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
            ]));
            if let Some(map) = workspace_env {
                lines.extend(env_key_lines(map, 2));
            }
        }
        // Per-agent override sub-sections.
        for (i, (agent, env)) in agent_sections.iter().enumerate() {
            let leading_blank = workspace_has_keys || i > 0;
            if leading_blank {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{agent}:"),
                    Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.extend(env_key_lines(env, 2));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  (no environment variables)",
            Style::default()
                .fg(PHOSPHOR_DIM)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

fn render_agents_subpanel(
    frame: &mut Frame,
    area: Rect,
    ws_config: Option<&crate::workspace::WorkspaceConfig>,
    config: &AppConfig,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Agents ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));

    let allowed = ws_config.map_or(&[][..], |w| w.allowed_agents.as_slice());
    // `all-agents-allowed` covers both the "no ws_config" case and the
    // empty-list shorthand on a real workspace. See `agent_allow` for the
    // canonical rule.
    let all_allowed = ws_config.is_none_or(super::super::agent_allow::allows_all_agents);

    let mut lines: Vec<Line> = Vec::new();

    // `Default: <agent>` — kept at the top of the Agents block so the
    // operator sees the workspace-default before scanning the agent
    // list. `Last used` is intentionally absent — that's launch-time
    // history, not part of the workspace's saved shape, and the
    // editor's General tab cleanup demoted it accordingly.
    //
    // Per-agent env detail moved to the consolidated Environments
    // block above, so this list is just names — one per line, with a
    // trailing star on the default-agent row.
    let default = ws_config.and_then(|w| w.default_agent.as_deref());
    let (value_text, value_style): (String, Style) = default.map_or_else(
        || ("(none)".to_string(), Style::default().fg(PHOSPHOR_DIM)),
        |name| (name.to_string(), Style::default().fg(PHOSPHOR_GREEN)),
    );
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("Default ", Style::default().fg(WHITE)),
        Span::styled(value_text, value_style),
    ]));
    lines.push(Line::from(""));

    // Agent listing. When `allowed_agents` is non-empty, that's the
    // operator's curated subset; when empty (the "all agents allowed"
    // shorthand) we list every globally-configured agent — same source
    // the editor's Agents tab iterates over.
    let agent_names: Vec<&str> = if all_allowed {
        config.agents.keys().map(String::as_str).collect()
    } else {
        allowed.iter().map(String::as_str).collect()
    };

    let name_style = |agent: &str| {
        if config.agents.contains_key(agent) {
            Style::default().fg(PHOSPHOR_GREEN)
        } else {
            Style::default().fg(PHOSPHOR_DIM)
        }
    };
    let star_style = Style::default().fg(PHOSPHOR_DIM);

    for agent in &agent_names {
        let is_default = Some(*agent) == default;
        let mut spans = vec![Span::styled(format!("  {agent}"), name_style(agent))];
        if is_default {
            spans.push(Span::styled(" \u{2605}", star_style));
        }
        lines.push(Line::from(spans));
    }

    let p = Paragraph::new(lines)
        .block(block)
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

#[cfg(test)]
mod mount_table_tests {
    use super::{MOUNT_MODE_COL_WIDTH, mount_path_width, render_mount_header, render_mount_lines};

    /// Collapse a `Line` into a single plain string (concat of all span contents).
    fn line_text(line: &ratatui::text::Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    /// Return the character index of the start of the `Mode` column (i.e. the
    /// "M" in "Mode" for the header, or the first char of "ro"/"rw" for a data
    /// row). Both are found at: `"  " + path_w + "  "` — so the index equals
    /// `2 + path_w + 2` for a header and for data rows that have no selection
    /// prefix (and the selection prefix is always two chars too — "▸ " or
    /// "  " — so the column boundary is stable).
    fn mode_col_start(line: &ratatui::text::Line<'_>) -> usize {
        let s = line_text(line);
        // The Mode column is the first two-letter "rw"/"ro" after the gap,
        // or the literal "Mode" for the header. Scan for the first non-space
        // character after the gap-of-two-spaces that follows the path.
        // Simpler: find the offset of the two-space gap before Mode.
        // Header: "  Path<pad>  Mode<pad>Type"
        // Data:   "  path<pad>  rw<pad>type"
        // In both cases the left edge of "Mode"/"rw" is exactly 2 + path_w + 2
        // from the start — we recover it by scanning for the first non-space
        // char at position >= 4 (past the left gutter + at least one path char).
        // Instead, just look for the substring "  M" (Mode header) or "  r"
        // (data row, always "rw"/"ro" starting with r).
        for (i, c) in s.chars().enumerate() {
            if i < 4 {
                continue;
            }
            if c == 'M' || c == 'r' {
                // Make sure this is preceded by the two-space gap — the first
                // such occurrence past the left gutter is the column boundary.
                let prev_two: String = s.chars().skip(i.saturating_sub(2)).take(2).collect();
                if prev_two == "  " {
                    return i;
                }
            }
        }
        panic!("mode column not found in line: {s:?}");
    }

    #[test]
    fn header_and_data_rows_share_path_column_width() {
        // Short path + long path forces path_w to be the length of the long one.
        let rows: Vec<(String, &str, String)> = vec![
            ("~/short".into(), "rw", "git · main".into()),
            (
                "~/Projects/very/deeply/nested/directory".into(),
                "ro",
                "dir".into(),
            ),
        ];
        let path_w = mount_path_width(&rows);
        assert!(path_w >= "~/Projects/very/deeply/nested/directory".len());

        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);

        let header_mode_col = mode_col_start(&header);
        let data0_mode_col = mode_col_start(&data[0]);
        let data1_mode_col = mode_col_start(&data[1]);

        assert_eq!(
            header_mode_col, data0_mode_col,
            "header 'mode' column must align with data row 0"
        );
        assert_eq!(
            header_mode_col, data1_mode_col,
            "header 'mode' column must align with data row 1"
        );
    }

    #[test]
    fn single_row_still_uses_minimum_column_width() {
        // Single short mount — path_w should stay at the floor so the
        // table is still visibly tabular.
        let rows: Vec<(String, &str, String)> = vec![(
            "~/Projects/ChainArgos/blockchain-nodes".into(),
            "rw",
            "git · main".into(),
        )];
        let path_w = mount_path_width(&rows);
        assert_eq!(path_w, "~/Projects/ChainArgos/blockchain-nodes".len());

        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);
        assert_eq!(mode_col_start(&header), mode_col_start(&data[0]));
    }

    #[test]
    fn empty_rows_uses_floor_for_header() {
        // Empty case: header should still render with the floor width and
        // include the two-space gap between "Mode" and "Type".
        let path_w = mount_path_width(&[]);
        assert_eq!(path_w, 10);
        let header = render_mount_header(path_w);
        // "  <path padded>  <mode padded>  Type"
        let expected = format!(
            "  {path:<path_w$}  {mode:<mw$}  Type",
            path = "Path",
            mode = "Mode",
            path_w = path_w,
            mw = MOUNT_MODE_COL_WIDTH,
        );
        let s = line_text(&header);
        assert_eq!(s, expected);
    }

    #[test]
    fn header_has_two_space_gap_between_mode_and_type() {
        // Regression for the "Mode Type" spacing bug: header must emit a
        // literal two-space gap between the `Mode` column and the `Type`
        // label, mirroring the gap data rows emit between `rw`/`ro` and the
        // kind. Additionally pins the type-column alignment: the `Type`
        // header label must start at the same character offset as the data
        // row's kind label (e.g. "folder") — without padding `rw`/`ro` to
        // the full width of "Mode", the kind column would render 2 chars
        // left of the header.
        let rows: Vec<(String, &str, String)> = vec![("~/p".into(), "rw", "folder".into())];
        let path_w = mount_path_width(&rows);
        let header = render_mount_header(path_w);
        let data = render_mount_lines(&rows, path_w);
        let header_text = line_text(&header);
        let data_text = line_text(&data[0]);
        assert!(
            header_text.contains("Mode  Type"),
            "expected 'Mode  Type' (two spaces between Mode and Type); got {header_text:?}"
        );
        let header_type_offset = header_text.find("Type").expect("header has 'Type'");
        let data_kind_offset = data_text.find("folder").expect("data row has 'folder'");
        assert_eq!(
            header_type_offset, data_kind_offset,
            "Type column misaligned: header at {header_type_offset}, data at {data_kind_offset}"
        );
    }
}

#[cfg(test)]
mod mount_block_height_tests {
    //! Pins the Mounts sub-panel height formula shared by
    //! `render_details_pane` and `render_current_dir_details_pane`. Guards
    //! against the "phantom empty row" regression where a fixed
    //! `Constraint::Length(5)` over-allocated by 1 for a single-mount
    //! current-directory workspace.
    use super::mount_block_height;
    use crate::workspace::MountConfig;

    fn mount(path: &str) -> MountConfig {
        MountConfig {
            src: path.into(),
            dst: path.into(),
            readonly: false,
        }
    }

    #[test]
    fn empty_mounts_reserves_row_for_none_placeholder() {
        // 0 data rows + "(none)" placeholder (1 row) + 1 header + 2 borders = 4.
        assert_eq!(mount_block_height(&[]), 4);
    }

    #[test]
    fn single_mount_fits_in_four_rows() {
        // Regression: the current-dir pane used to hard-code `Length(5)`
        // which left an extra empty line inside the block. Correct total
        // for a 1-mount workspace is 1 data + 1 header + 2 borders = 4.
        assert_eq!(mount_block_height(&[mount("/tmp/a")]), 4);
    }

    #[test]
    fn multiple_mounts_scale_linearly() {
        assert_eq!(mount_block_height(&[mount("/tmp/a"), mount("/tmp/b")]), 5);
        assert_eq!(
            mount_block_height(&[mount("/a"), mount("/b"), mount("/c")]),
            6
        );
    }

    #[test]
    fn many_mounts_clamp_to_twelve() {
        let mounts: Vec<MountConfig> = (0..20).map(|i| mount(&format!("/m/{i}"))).collect();
        assert_eq!(mount_block_height(&mounts), 12);
    }
}

#[cfg(test)]
mod subpanel_padding_tests {
    //! Visual regression tests pinning the leading-padding convention shared
    //! by the General / Mounts / Agents sub-panels. All three render content
    //! rows starting at the same column so the first visible character of
    //! the three blocks, giving the right pane a tidy left edge.
    use super::{
        SUBPANEL_CONTENT_INDENT, render_agents_subpanel, render_environments_subpanel,
        render_general_subpanel, render_mounts_subpanel,
    };
    use crate::config::AppConfig;
    use crate::console::manager::state::WorkspaceSummary;
    use crate::workspace::WorkspaceConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    /// Scan the first content row inside a sub-panel block (y = 1, skipping
    /// the top border at y = 0) for the first cell holding a printable
    /// non-space character, skipping the left vertical border. Returns the
    /// offset of that character *from the left border* — i.e. the indent —
    /// so values can be compared against `SUBPANEL_CONTENT_INDENT` directly.
    fn first_content_indent(terminal: &Terminal<TestBackend>) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        // Locate the left border column first so the returned value is the
        // relative indent, not the absolute column.
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, 1)].symbol();
            sym == "│" || sym == "║"
        })?;
        for x in (border_x + 1)..area.width {
            let sym = buf[(x, 1)].symbol();
            if sym.is_empty() || sym == " " {
                continue;
            }
            return Some((x - border_x - 1) as usize);
        }
        None
    }

    fn summary() -> WorkspaceSummary {
        WorkspaceSummary {
            name: "demo".into(),
            workdir: "/tmp/demo".into(),
            mount_count: 1,
            readonly_mount_count: 0,
            allowed_agent_count: 0,
            default_agent: None,
            last_agent: None,
        }
    }

    fn ws_config_with_allowed(names: &[&str], default: Option<&str>) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: "/tmp/demo".into(),
            mounts: vec![],
            allowed_agents: names.iter().map(|s| (*s).into()).collect(),
            default_agent: default.map(String::from),
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    /// The first visible character of row 0 inside each sub-panel block
    /// must sit at the shared `SUBPANEL_CONTENT_INDENT`. Without the General
    /// block's two-space prefix the `w` of `workdir` rendered at column 1
    /// (flush with the border) while Mounts/Agents rendered at column 2.
    #[test]
    fn subpanel_content_column_alignment() {
        // General
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_general_subpanel(f, Rect::new(0, 0, 40, 4), &summary());
        })
        .unwrap();
        let general_col = first_content_indent(&term).expect("general has content");

        // Mounts
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_mounts_subpanel(f, Rect::new(0, 0, 40, 4), &[]);
        })
        .unwrap();
        let mounts_col = first_content_indent(&term).expect("mounts has content");

        // Agents, "any agent" branch (no allowed list)
        let cfg = AppConfig::default();
        let backend = TestBackend::new(40, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 4), None, &cfg);
        })
        .unwrap();
        let agents_any_col = first_content_indent(&term).expect("agents 'any' has content");

        assert_eq!(
            general_col, SUBPANEL_CONTENT_INDENT,
            "General first char at col {general_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
        assert_eq!(
            mounts_col, SUBPANEL_CONTENT_INDENT,
            "Mounts first char at col {mounts_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
        assert_eq!(
            agents_any_col, SUBPANEL_CONTENT_INDENT,
            "Agents (any) first char at col {agents_any_col}, expected {SUBPANEL_CONTENT_INDENT}"
        );
    }

    /// Scan row `y` inside a sub-panel block for the first cell whose
    /// symbol equals `needle`, returning the offset from the left border.
    /// Used to locate the trailing star glyph on a default-agent row.
    fn find_symbol_indent(terminal: &Terminal<TestBackend>, y: u16, needle: &str) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        for x in (border_x + 1)..area.width {
            if buf[(x, y)].symbol() == needle {
                return Some((x - border_x - 1) as usize);
            }
        }
        None
    }

    /// Scan row `y` for the last printable non-space/border cell and
    /// return its relative offset from the left border. Used to confirm
    /// a non-default row has no trailing suffix past the name.
    fn last_printable_indent(terminal: &Terminal<TestBackend>, y: u16) -> Option<usize> {
        let buf = terminal.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        let right_border_x = ((border_x + 1)..area.width).find(|x| {
            let sym = buf[(*x, y)].symbol();
            sym == "│" || sym == "║"
        })?;
        let mut last: Option<usize> = None;
        for x in (border_x + 1)..right_border_x {
            let sym = buf[(x, y)].symbol();
            if !sym.is_empty() && sym != " " {
                last = Some((x - border_x - 1) as usize);
            }
        }
        last
    }

    /// Non-default agent rows render the name starting at
    /// `SUBPANEL_CONTENT_INDENT` (col 2 from the border). With the
    /// trailing-star convention no glyph precedes the name.
    ///
    /// With the lean Agents block (env detail moved to the
    /// Environments block), the sub-panel lays out for two allowed
    /// agents (alpha default, beta non-default):
    ///   y=0 top border
    ///   y=1 `  Default <name>`
    ///   y=2 blank spacer
    ///   y=3 alpha row (default)
    ///   y=4 beta row (non-default)
    #[test]
    fn agents_subpanel_non_default_agent_name_starts_at_col_2() {
        let ws = ws_config_with_allowed(&["alpha", "beta"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());
        cfg.agents
            .insert("beta".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(40, 7);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 7), Some(&ws), &cfg);
        })
        .unwrap();

        // Locate the first printable char on the beta row (y=4).
        let buf = term.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width)
            .find(|x| {
                let sym = buf[(*x, 4)].symbol();
                sym == "│" || sym == "║"
            })
            .expect("left border on beta row");
        let name_col = ((border_x + 1)..area.width)
            .find(|x| {
                let sym = buf[(*x, 4)].symbol();
                !sym.is_empty() && sym != " "
            })
            .map(|x| (x - border_x - 1) as usize)
            .expect("beta row has content");
        assert_eq!(
            name_col, SUBPANEL_CONTENT_INDENT,
            "non-default agent name should start at col {SUBPANEL_CONTENT_INDENT}, got {name_col}"
        );

        // And there must be no trailing star on the non-default row.
        let last_col = last_printable_indent(&term, 4).expect("beta row has content");
        // `beta` is 4 chars starting at col 2 ⇒ last printable at col 5.
        // A trailing star would push last_col to col 7 (space + star).
        assert_eq!(
            last_col,
            SUBPANEL_CONTENT_INDENT + "beta".len() - 1,
            "non-default agent row must have no trailing suffix past the name",
        );
    }

    /// Default agent row carries a trailing star glyph positioned after
    /// the agent name (separated by a space), not a leading star.
    ///
    /// Agents sub-panel layout: top border at y=0, `Default <name>` at
    /// y=1, blank at y=2, first agent row at y=3. For a single-allowed
    /// workspace that agent IS the default.
    #[test]
    fn agents_subpanel_default_agent_has_trailing_star() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
        })
        .unwrap();

        let star_col = find_symbol_indent(&term, 3, "\u{2605}")
            .expect("default agent row should contain a star glyph");
        let expected = SUBPANEL_CONTENT_INDENT + "alpha".len() + 1;
        assert_eq!(
            star_col, expected,
            "default agent star should trail the name at col {expected}, got {star_col}"
        );
    }

    /// Default agent row's name column matches non-default rows (and the
    /// `SUBPANEL_CONTENT_INDENT` convention). The trailing star must not
    /// shift the name right.
    ///
    /// y=1 is the `Default <agent>` row, whose label also starts at
    /// `SUBPANEL_CONTENT_INDENT`. The invariant the test pins (every
    /// content row starts at col 2) still holds — what we're confirming
    /// is that the block's leading indent is consistent. We check the
    /// agent row explicitly to guard against the trailing-star breaking
    /// the name-column alignment.
    #[test]
    fn agents_subpanel_default_agent_name_starts_at_col_2_regardless_of_star() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(40, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
        })
        .unwrap();

        // Locate the first printable char on the alpha row (y=3).
        let buf = term.backend().buffer();
        let area = buf.area;
        let border_x = (0..area.width)
            .find(|x| {
                let sym = buf[(*x, 3)].symbol();
                sym == "│" || sym == "║"
            })
            .expect("left border on alpha row");
        let name_col = ((border_x + 1)..area.width)
            .find(|x| {
                let sym = buf[(*x, 3)].symbol();
                !sym.is_empty() && sym != " "
            })
            .map(|x| (x - border_x - 1) as usize)
            .expect("alpha row has content");
        assert_eq!(
            name_col, SUBPANEL_CONTENT_INDENT,
            "default agent name should start at col {SUBPANEL_CONTENT_INDENT} even with the trailing star, got {name_col}"
        );
    }

    // ── General sub-panel: Last-used row was already removed ──────────

    /// The General sub-panel no longer shows `Last used` — it only renders
    /// `Working dir`. Guards against a regression that reintroduces the row
    /// and grows the block back to 4 rows.
    #[test]
    fn general_subpanel_no_longer_shows_last_used() {
        let mut s = summary();
        s.last_agent = Some("alpha".into());

        let backend = TestBackend::new(60, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_general_subpanel(f, Rect::new(0, 0, 60, 4), &s);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            assert!(
                !row.contains("Last used"),
                "General sub-panel must not render `Last used`; got row {y}: {row:?}"
            );
        }
    }

    // ── Agents sub-panel: Default row + per-agent overrides ───────────

    /// Render the Agents sub-panel into a `TestBackend` of the given size
    /// and return one row of the buffer at `y` as a plain string. Used
    /// throughout this section to scrape per-row text after layout shifts.
    fn render_agents_row(
        ws: Option<&crate::workspace::WorkspaceConfig>,
        cfg: &AppConfig,
        width: u16,
        height: u16,
        y: u16,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, width, height), ws, cfg);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let area = buf.area;
        let mut row = String::new();
        for x in 0..area.width {
            row.push_str(buf[(x, y)].symbol());
        }
        row
    }

    /// The Agents sub-panel renders `Default <agent>` at the top, above
    /// the blank spacer and the per-agent rows.
    #[test]
    fn agents_subpanel_shows_default_at_top() {
        let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());

        let row = render_agents_row(Some(&ws), &cfg, 60, 6, 1);
        assert!(
            row.contains("Default"),
            "Agents row 1 must hold `Default`; got {row:?}"
        );
        assert!(
            row.contains("alpha"),
            "Agents row 1 must hold the default agent name; got {row:?}"
        );
    }

    /// When `default_agent` is `None`, the Default row shows `(none)`.
    #[test]
    fn agents_subpanel_default_none_renders_placeholder() {
        let ws = ws_config_with_allowed(&[], None);
        let cfg = AppConfig::default();

        let row = render_agents_row(Some(&ws), &cfg, 60, 6, 1);
        assert!(
            row.contains("Default") && row.contains("(none)"),
            "Default row should show `(none)` when no default agent is set; got {row:?}"
        );
    }

    /// `Last used` must no longer appear anywhere in the Agents
    /// sub-panel — it was demoted as part of the preview cleanup that
    /// nested per-agent overrides under each agent name.
    #[test]
    fn agents_subpanel_no_longer_shows_last_used() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.last_agent = Some("beta".into());
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(60, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 8), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            assert!(
                !row.contains("Last used"),
                "Agents sub-panel must not render `Last used`; got row {y}: {row:?}"
            );
        }
    }

    /// The Agents block is now a lean default + name list; per-agent
    /// env overrides moved to the consolidated Environments block.
    /// This test pins that the Agents sub-panel does NOT mention any
    /// override key names — the keys belong only in the Environments
    /// block now.
    #[test]
    fn preview_agents_block_no_longer_lists_overrides() {
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        let mut overrides = crate::workspace::WorkspaceAgentOverride::default();
        overrides.env.insert("API_KEY".into(), "literal".into());
        overrides
            .env
            .insert("LOG_LEVEL".into(), "op://Vault/Item/field".into());
        ws.agents.insert("alpha".into(), overrides);

        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(60, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 8), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        // Per-agent override keys must NOT appear in the Agents block —
        // they live in the Environments block now.
        assert!(
            !joined.contains("API_KEY"),
            "override key API_KEY must NOT appear in the Agents block; got {joined}"
        );
        assert!(
            !joined.contains("LOG_LEVEL"),
            "override key LOG_LEVEL must NOT appear in the Agents block; got {joined}"
        );
        assert!(
            !joined.contains("[op]"),
            "`[op]` marker must NOT appear in the Agents block; got {joined}"
        );
        assert!(
            !joined.contains("(no overrides)"),
            "`(no overrides)` placeholder must NOT appear in the Agents block; got {joined}"
        );
        // Default + agent name still render.
        assert!(
            joined.contains("Default") && joined.contains("alpha"),
            "Agents block must still show default + agent name; got {joined}"
        );
    }

    /// When `allowed_agents` is empty (the "all agents allowed"
    /// shorthand), the preview lists every globally-configured agent —
    /// matching what the editor's Agents tab shows. No `any agent`
    /// placeholder.
    #[test]
    fn preview_agents_block_lists_all_global_agents_when_allowed_empty() {
        let ws = ws_config_with_allowed(&[], None);
        let mut cfg = AppConfig::default();
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());
        cfg.agents
            .insert("beta".into(), crate::config::AgentSource::default());

        let backend = TestBackend::new(60, 12);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_subpanel(f, Rect::new(0, 0, 60, 12), Some(&ws), &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("alpha"),
            "alpha should be listed under all-allowed shorthand; got {joined}"
        );
        assert!(
            joined.contains("beta"),
            "beta should be listed under all-allowed shorthand; got {joined}"
        );
        assert!(
            !joined.contains("any agent"),
            "old `any agent` placeholder should be gone; got {joined}"
        );
    }

    // ── Environments sub-panel ─────────────────────────────────────────

    /// The Environments preview block lists workspace-level env keys in
    /// alphabetical order (BTreeMap iteration order) under the
    /// `All agents:` sub-header. Key names only — plain or op:// values
    /// never render. Op:// values get a `[op]` marker matching the
    /// editor convention.
    #[test]
    fn preview_includes_environments_block_with_workspace_env_keys() {
        let mut ws = ws_config_with_allowed(&[], None);
        ws.env.insert("DB_URL".into(), "postgres://...".into());
        ws.env.insert("API_KEY".into(), "literal-secret".into());

        let backend = TestBackend::new(60, 6);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(f, Rect::new(0, 0, 60, 6), Some(&ws));
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("Environments"),
            "block title `Environments` must appear; got {joined}"
        );
        assert!(
            joined.contains("All agents:"),
            "`All agents:` sub-header must appear above the workspace keys; got {joined}"
        );
        assert!(
            joined.contains("API_KEY"),
            "API_KEY env key must appear; got {joined}"
        );
        assert!(
            joined.contains("DB_URL"),
            "DB_URL env key must appear; got {joined}"
        );
        // Values must never appear in the preview.
        assert!(
            !joined.contains("postgres://"),
            "plain env values must not render; got {joined}"
        );
        assert!(
            !joined.contains("literal-secret"),
            "plain env values must not render; got {joined}"
        );
    }

    /// The Environments preview lays out workspace env keys under
    /// `All agents:` first, then per-agent override sub-sections in
    /// alphabetical order. Each agent sub-section is headed by
    /// `<agent>:` and contains the override keys nested under it.
    #[test]
    fn preview_environments_block_lists_all_agents_then_per_agent_subsections() {
        let mut ws = ws_config_with_allowed(&["beta", "alpha"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());
        ws.env.insert("DB_URL".into(), "postgres://...".into());

        let mut alpha_overrides = crate::workspace::WorkspaceAgentOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.agents.insert("alpha".into(), alpha_overrides);

        let mut beta_overrides = crate::workspace::WorkspaceAgentOverride::default();
        beta_overrides.env.insert("DEBUG".into(), "1".into());
        ws.agents.insert("beta".into(), beta_overrides);

        let backend = TestBackend::new(60, 14);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(f, Rect::new(0, 0, 60, 14), Some(&ws));
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        // Find the y-row of each sub-header so we can pin their order.
        let mut all_y: Option<u16> = None;
        let mut alpha_y: Option<u16> = None;
        let mut beta_y: Option<u16> = None;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            if all_y.is_none() && row.contains("All agents:") {
                all_y = Some(y);
            }
            if alpha_y.is_none() && row.contains("alpha:") {
                alpha_y = Some(y);
            }
            if beta_y.is_none() && row.contains("beta:") {
                beta_y = Some(y);
            }
        }
        let a = all_y.expect("`All agents:` sub-header must appear");
        let al = alpha_y.expect("`alpha:` sub-header must appear");
        let bt = beta_y.expect("`beta:` sub-header must appear");
        assert!(
            a < al && al < bt,
            "Sub-section order must be All agents < alpha < beta; got y=({a},{al},{bt})"
        );
    }

    /// Agents listed in `allowed_agents` but with no env overrides do
    /// NOT appear in the Environments block — their absence is the
    /// signal that they have no overrides. The Agents block still
    /// lists them.
    #[test]
    fn preview_environments_block_omits_agents_without_overrides() {
        let mut ws = ws_config_with_allowed(&["alpha", "beta"], Some("alpha"));
        ws.env.insert("API_KEY".into(), "literal".into());
        // Only alpha has overrides; beta is in the allowed list but
        // has no overrides.
        let mut alpha_overrides = crate::workspace::WorkspaceAgentOverride::default();
        alpha_overrides
            .env
            .insert("LOG_LEVEL".into(), "debug".into());
        ws.agents.insert("alpha".into(), alpha_overrides);

        let backend = TestBackend::new(60, 10);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(f, Rect::new(0, 0, 60, 10), Some(&ws));
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("alpha:"),
            "alpha has overrides — its sub-section must appear; got {joined}"
        );
        assert!(
            !joined.contains("beta:"),
            "beta has no overrides — its name must NOT appear in the Environments block; got {joined}"
        );
    }

    /// Op:// references in the workspace env get a leading `[op]` marker.
    /// The bare reference itself (e.g. "op://Vault/Item/field") must
    /// never appear — only the marker tag.
    #[test]
    fn preview_environments_marks_op_references_with_op_marker() {
        let mut ws = ws_config_with_allowed(&[], None);
        ws.env
            .insert("STRIPE_KEY".into(), "op://Vault/Item/field".into());

        let backend = TestBackend::new(60, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(f, Rect::new(0, 0, 60, 4), Some(&ws));
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("[op]"),
            "op:// reference must be tagged with `[op]` marker; got {joined}"
        );
        assert!(
            joined.contains("STRIPE_KEY"),
            "key name must still appear next to `[op]`; got {joined}"
        );
        assert!(
            !joined.contains("op://"),
            "raw op:// reference must never render in the preview; got {joined}"
        );
    }

    /// Empty workspace env renders an italic-dim placeholder, so the
    /// block is still visibly demarcated even with no data.
    #[test]
    fn preview_environments_block_empty_renders_placeholder() {
        let ws = ws_config_with_allowed(&[], None);

        let backend = TestBackend::new(60, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_environments_subpanel(f, Rect::new(0, 0, 60, 4), Some(&ws));
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        let mut joined = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("(no environment variables)"),
            "empty workspace env must render the placeholder; got {joined}"
        );
    }

    /// The right-pane preview blocks render in the order
    /// General → Mounts → Environments → Agents. Pinned by scraping the
    /// block-title labels off a full-pane render and confirming their
    /// y-order.
    #[test]
    fn preview_block_order_is_general_mounts_environments_agents() {
        // Build a workspace with a mount, an env var, and an agent so
        // every block has visible content.
        let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
        ws.workdir = "/workspace/demo".into();
        ws.mounts.push(crate::workspace::MountConfig {
            src: "/tmp/demo".into(),
            dst: "/workspace/demo".into(),
            readonly: false,
        });
        ws.env.insert("API_KEY".into(), "literal".into());

        let mut cfg = AppConfig::default();
        cfg.workspaces.insert("demo".into(), ws);
        cfg.agents
            .insert("alpha".into(), crate::config::AgentSource::default());

        let summary = WorkspaceSummary {
            name: "demo".into(),
            workdir: "/workspace/demo".into(),
            mount_count: 1,
            readonly_mount_count: 0,
            allowed_agent_count: 1,
            default_agent: Some("alpha".into()),
            last_agent: None,
        };

        let backend = TestBackend::new(60, 24);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            super::render_details_pane(f, Rect::new(0, 0, 60, 24), &summary, &cfg);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let area = buf.area;
        // For each block, find the y-row that holds its title (titles
        // are unique strings so we can scrape by row content).
        let mut general_y: Option<u16> = None;
        let mut mounts_y: Option<u16> = None;
        let mut envs_y: Option<u16> = None;
        let mut agents_y: Option<u16> = None;
        for y in 0..area.height {
            let mut row = String::new();
            for x in 0..area.width {
                row.push_str(buf[(x, y)].symbol());
            }
            if general_y.is_none() && row.contains(" General ") {
                general_y = Some(y);
            }
            if mounts_y.is_none() && row.contains(" Mounts ") {
                mounts_y = Some(y);
            }
            if envs_y.is_none() && row.contains(" Environments ") {
                envs_y = Some(y);
            }
            if agents_y.is_none() && row.contains(" Agents ") {
                agents_y = Some(y);
            }
        }

        let g = general_y.expect("General block title must appear");
        let m = mounts_y.expect("Mounts block title must appear");
        let e = envs_y.expect("Environments block title must appear");
        let a = agents_y.expect("Agents block title must appear");
        assert!(
            g < m && m < e && e < a,
            "block order must be General < Mounts < Environments < Agents; got y=({g},{m},{e},{a})"
        );
    }
}
