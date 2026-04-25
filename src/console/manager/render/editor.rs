//! Editor-stage rendering: full-screen editor with header, tab bar,
//! per-tab body renderers (General / Mounts / Agents / Secrets), and the
//! contextual footer composition that varies with the active tab + cursor.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::super::state::{EditorMode, EditorState, EditorTab, FieldFocus};
use super::list::{
    MOUNT_ISO_COL_WIDTH, MOUNT_MODE_COL_WIDTH, format_mount_rows, mount_path_width,
    render_mount_header,
};
use super::{
    FooterItem, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, render_footer, render_header,
};
use crate::config::AppConfig;

// ── Editor stage ────────────────────────────────────────────────────

pub fn render_editor(frame: &mut Frame, state: &EditorState<'_>, config: &AppConfig) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(2), // tab strip
            Constraint::Min(8),    // tab body
            Constraint::Length(2), // footer
        ])
        .split(area);

    let title = match &state.mode {
        EditorMode::Edit { name } => format!("edit · {name}"),
        EditorMode::Create => "new workspace".to_string(),
    };
    render_header(frame, chunks[0], &title);

    render_tab_strip(frame, chunks[1], state.active_tab);

    match state.active_tab {
        EditorTab::General => render_general_tab(frame, chunks[2], state),
        EditorTab::Mounts => render_mounts_tab(frame, chunks[2], state),
        EditorTab::Agents => render_agents_tab(frame, chunks[2], state, config),
        EditorTab::Secrets => render_secrets_stub(frame, chunks[2]),
    }

    // Contextual footer: row-specific hints + base stage hints.
    let mut items: Vec<FooterItem> = Vec::new();

    // Row-specific group (may be empty).
    let row_items = contextual_row_items(state);
    if !row_items.is_empty() {
        items.extend(row_items);
        items.push(FooterItem::GroupSep);
    }

    // Save group — label varies with dirty/clean.
    items.push(FooterItem::Key("S"));
    items.push(FooterItem::Text("save workspace"));
    if state.is_dirty() {
        items.push(FooterItem::Dyn(format!(
            "({} changes)",
            state.change_count()
        )));
    }

    // Tab-for-next-tab and ↑↓-for-cursor-move are universal across every
    // editor tab — they don't need to be advertised in the base footer.

    // Exit group — discard if dirty, back if clean.
    items.push(FooterItem::GroupSep);
    items.push(FooterItem::Key("Esc"));
    if state.is_dirty() {
        items.push(FooterItem::Text("discard"));
    } else {
        items.push(FooterItem::Text("back"));
    }

    render_footer(frame, chunks[3], &items);

    // Error banner overlay — top line of the body. Only rendered when
    // `save_flow` is in the `Error` state AND no ErrorPopup modal is up
    // (the popup is the commit-time error surface; the banner is the
    // pre-commit validation surface — they share the `Error` variant but
    // present differently).
    if state.modal.is_none()
        && let Some(err) = state.save_flow.error_message()
    {
        let banner_area = Rect {
            x: chunks[2].x,
            y: chunks[2].y,
            width: chunks[2].width,
            height: 1,
        };
        let banner = Paragraph::new(format!("✗ {err}")).style(
            Style::default()
                .fg(Color::Rgb(255, 94, 122))
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(ratatui::widgets::Clear, banner_area);
        frame.render_widget(banner, banner_area);
    }
}

/// Compute a row-specific hint fragment based on the active tab and cursor.
/// Returns an empty vec when the current position has no action.
fn contextual_row_items(state: &EditorState<'_>) -> Vec<FooterItem> {
    let FieldFocus::Row(cursor) = state.active_field;
    match state.active_tab {
        EditorTab::General => {
            // Row indices are uniform across both modes for Enter-affordance
            // purposes. Create mode has rows 0-1; Edit mode also has 2-3
            // (default agent, last used) which are read-only and surface no
            // Enter action either way.
            //   row 0 = Name        (editable in both modes — Enter opens rename)
            //   row 1 = Working dir (editable — Enter opens workdir picker)
            match cursor {
                0 => vec![FooterItem::Key("Enter"), FooterItem::Text("rename")],
                1 => vec![
                    FooterItem::Key("Enter"),
                    FooterItem::Text("pick working directory"),
                ],
                _ => Vec::new(),
            }
        }
        EditorTab::Mounts => {
            let mount_count = state.pending.mounts.len();
            if cursor < mount_count {
                let mut items = vec![
                    FooterItem::Key("D"),
                    FooterItem::Text("remove"),
                    FooterItem::Sep,
                    FooterItem::Key("A"),
                    FooterItem::Text("add"),
                ];
                // Surface `O open in GitHub` when the cursor is on a mount
                // whose source resolves to a GitHub-hosted git repo with a
                // web URL. Editor-only — the list view's mounts pane is
                // a preview, not a focus target.
                if let Some(m) = state.pending.mounts.get(cursor)
                    && matches!(
                        super::super::mount_info::inspect(&m.src),
                        super::super::mount_info::MountKind::Git {
                            host: super::super::mount_info::GitHost::Github,
                            web_url: Some(_),
                            ..
                        }
                    )
                {
                    items.push(FooterItem::Sep);
                    items.push(FooterItem::Key("O"));
                    items.push(FooterItem::Text("open in GitHub"));
                }
                // `R` toggles the readonly flag on the highlighted mount row
                // (rw ↔ ro). Sentinel row omits this hint — there's nothing
                // to toggle yet.
                items.push(FooterItem::Sep);
                items.push(FooterItem::Key("R"));
                items.push(FooterItem::Text("toggle ro/rw"));
                items
            } else {
                // Sentinel "+ Add mount" row — both Enter and A invoke the
                // same add-mount flow, so render as a single combined key.
                vec![FooterItem::Key("Enter/A"), FooterItem::Text("add")]
            }
        }
        EditorTab::Agents => vec![
            FooterItem::Key("Space"),
            FooterItem::Text("toggle"),
            FooterItem::Sep,
            FooterItem::Key("D"),
            FooterItem::Text("default"),
        ],
        EditorTab::Secrets => Vec::new(),
    }
}

fn render_tab_strip(frame: &mut Frame, area: Rect, active: EditorTab) {
    let labels = [
        (EditorTab::General, "General"),
        (EditorTab::Mounts, "Mounts"),
        (EditorTab::Agents, "Agents"),
        (EditorTab::Secrets, "Secrets ⏳"),
    ];
    let mut spans = Vec::new();
    for (tab, label) in labels {
        let style = if tab == active {
            Style::default()
                .bg(PHOSPHOR_GREEN)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else if tab == EditorTab::Secrets {
            Style::default()
                .fg(Color::Rgb(90, 90, 90))
                .add_modifier(Modifier::ITALIC)
        } else {
            Style::default().fg(PHOSPHOR_DIM)
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_general_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));

    let FieldFocus::Row(cursor) = state.active_field;

    let is_edit = matches!(&state.mode, EditorMode::Edit { .. });

    let name_dirty = match &state.mode {
        EditorMode::Edit { name } => state.pending_name.as_deref().is_some_and(|n| n != name),
        EditorMode::Create => false,
    };
    let name_value = match &state.mode {
        EditorMode::Edit { name } => state.pending_name.as_deref().unwrap_or(name.as_str()),
        EditorMode::Create => state.pending_name.as_deref().unwrap_or("(new)"),
    };

    // In Create mode the row numbering is:
    //   0 = name (editable — Enter opens rename TextInput, pre-filled from prelude)
    //   1 = workdir
    // In Edit mode:
    //   0 = name (editable), 1 = workdir, 2 = default agent (ro), 3 = last used (ro)
    let mut rows: Vec<Line> = Vec::new();

    if is_edit {
        // Edit mode: name is an editable row at index 0.
        rows.push(render_editor_row(0, cursor, "Name", name_value, name_dirty));
        let workdir_display = crate::tui::shorten_home(&state.pending.workdir);
        rows.push(render_editor_row(
            1,
            cursor,
            "Working dir",
            &workdir_display,
            state.pending.workdir != state.original.workdir,
        ));
        // Default agent — read-only here; set via Agents tab.
        rows.push(render_editor_readonly_row(
            2,
            cursor,
            "Default agent",
            state.pending.default_agent.as_deref().unwrap_or("(none)"),
        ));
        // Last used — read-only.
        rows.push(render_editor_readonly_row(
            3,
            cursor,
            "Last used",
            state.original.last_agent.as_deref().unwrap_or("(none)"),
        ));
    } else {
        // Create mode: name is editable (Enter opens the rename TextInput)
        // but we don't show an `● unsaved` marker because there's no
        // "original" workspace to diff against — the save_count already
        // tracks field-level changes.
        rows.push(render_editor_row(0, cursor, "Name", name_value, false));
        let workdir_display = crate::tui::shorten_home(&state.pending.workdir);
        rows.push(render_editor_row(
            1,
            cursor,
            "Working dir",
            &workdir_display,
            false,
        ));
        // Hide "Default agent" and "Last used" in Create mode — they have no meaning yet.
    }

    frame.render_widget(Paragraph::new(rows).block(block), area);
}

/// Render a field row with cursor highlight when `row == cursor`.
fn render_editor_row(
    row: usize,
    cursor: usize,
    label: &str,
    value: &str,
    dirty: bool,
) -> Line<'static> {
    let selected = row == cursor;
    let prefix = if selected { "▸ " } else { "  " };
    // Labels stay white regardless of focus — focus is signalled by the
    // `▸` prefix and the bold weight, not by a colour shift.
    let label_style = if selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let mut spans = vec![Span::styled(format!("{prefix}{label:15}"), label_style)];
    let value_style = if selected {
        Style::default()
            .fg(PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(PHOSPHOR_GREEN)
    };
    spans.push(Span::styled(value.to_string(), value_style));
    if dirty {
        spans.push(Span::styled(
            "    ● unsaved",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn render_editor_readonly_row(
    row: usize,
    cursor: usize,
    label: &str,
    value: &str,
) -> Line<'static> {
    let selected = row == cursor;
    let prefix = if selected { "▸ " } else { "  " };
    // Read-only rows: label stays white (bold when focused) like editable
    // rows; value + `(read-only)` suffix render in dim phosphor so the
    // operator can visually skim editable vs fixed fields.
    let label_style = if selected {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    Line::from(vec![
        Span::styled(format!("{prefix}{label:15}"), label_style),
        Span::styled(value.to_string(), Style::default().fg(PHOSPHOR_DIM)),
        Span::styled(
            " (read-only)",
            Style::default()
                .fg(PHOSPHOR_DIM)
                .add_modifier(Modifier::ITALIC),
        ),
    ])
}

fn render_mounts_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let FieldFocus::Row(cursor) = state.active_field;

    let white = WHITE;

    // Build aligned table rows for all mounts.
    let rows = format_mount_rows(&state.pending.mounts);
    let path_w = mount_path_width(&rows);

    // Header row — shares path_w so the "mode" and "type" columns line up
    // with data rows regardless of path width.
    let mut lines: Vec<Line> = vec![render_mount_header(path_w)];

    lines.extend(rows.iter().enumerate().map(|(i, (path, mode, iso, kind))| {
        let selected = i == cursor;
        let prefix = if selected { "▸ " } else { "  " };
        let base_style = if selected {
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };
        let dim_style = Style::default()
            .fg(PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC);
        Line::from(vec![
            Span::styled(format!("{prefix}{path:<path_w$}  "), base_style),
            Span::styled(
                format!("{mode:<MOUNT_MODE_COL_WIDTH$}"),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            // Two-space gap before the iso column — matches the header.
            Span::raw("  "),
            Span::styled(
                format!("{iso:<MOUNT_ISO_COL_WIDTH$}"),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            // Two-space gap before the type column — matches the header.
            Span::raw("  "),
            Span::styled(kind.clone(), dim_style),
        ])
    }));

    // Sentinel row: + Add mount — selectable, styled distinctly from mounts.
    let sentinel_idx = state.pending.mounts.len();
    let sentinel_selected = cursor == sentinel_idx;
    let sentinel_prefix = if sentinel_selected { "▸ " } else { "  " };
    let sentinel_style = if sentinel_selected {
        Style::default().fg(white).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(white)
    };
    lines.push(Line::from(Span::styled(
        format!("{sentinel_prefix}+ Add mount"),
        sentinel_style,
    )));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_agents_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>, config: &AppConfig) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let FieldFocus::Row(cursor) = state.active_field;

    // Status line: "Allowed agents:  [ all ]" or "[ custom ]   (3 of 5 allowed)"
    let is_all = super::super::agent_allow::allows_all_agents(&state.pending);
    let total = config.agents.len();
    let allowed_count = state.pending.allowed_agents.len();

    let badge_text = if is_all { "  all  " } else { "  custom  " };
    let badge_bg = if is_all { PHOSPHOR_GREEN } else { WHITE };
    let badge_style = Style::default()
        .bg(badge_bg)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let mut status_spans = vec![
        Span::styled(
            "  Allowed agents:  ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
        Span::styled(badge_text, badge_style),
    ];
    if !is_all {
        status_spans.push(Span::styled(
            format!("   ({allowed_count} of {total} allowed)"),
            Style::default()
                .fg(Color::Rgb(180, 255, 180))
                .add_modifier(Modifier::ITALIC),
        ));
    }
    let status_line = Line::from(status_spans);

    // Blank spacer between the status line and the agent rows. The old
    // `allowed?  ·  agent` column header got dropped — the `[x]` / `[ ]`
    // prefix on each row already signals the toggle semantics, so a
    // dedicated header added noise without clarity.
    let mut lines = vec![status_line, Line::from("")];

    // Agent rows. Cursor is 0-based into config.agents (no header offset).
    //
    // `[x]` reflects the *effectively allowed* state, not literal list
    // membership. An empty `allowed_agents` list is the shorthand for
    // "all agents allowed" (matches the `all` badge above) — in that
    // mode every row renders `[x]`. Otherwise only agents named in the
    // list render `[x]`.
    for (i, (agent_name, _)) in config.agents.iter().enumerate() {
        let selected = i == cursor;
        let effectively_allowed =
            super::super::agent_allow::agent_is_effectively_allowed(&state.pending, agent_name);
        let is_default = state.pending.default_agent.as_deref() == Some(agent_name.as_str());
        let check = if effectively_allowed { "[x]" } else { "[ ]" };
        let star = if is_default { "★" } else { " " };
        let prefix = if selected { "▸ " } else { "  " };
        let text = format!("{prefix}{check}  {star} {agent_name}");
        let style = if selected {
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_secrets_stub(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let body = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Secrets management lands in PR 3 of this series.",
            Style::default()
                .fg(PHOSPHOR_DIM)
                .add_modifier(Modifier::ITALIC),
        )),
    ];
    frame.render_widget(Paragraph::new(body).block(block), area);
}

#[cfg(test)]
mod contextual_row_items_tests {
    //! Row-specific footer-hint composition for the editor tabs.

    use super::super::FooterItem;
    use super::contextual_row_items;
    use crate::console::manager::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::{MountConfig, WorkspaceConfig};

    /// Collect every `FooterItem::Text` label from a hint list.
    fn text_labels(items: &[FooterItem]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|it| {
                if let FooterItem::Text(t) = it {
                    Some(*t)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Collect every `FooterItem::Key` glyph from a hint list.
    fn key_glyphs(items: &[FooterItem]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|it| {
                if let FooterItem::Key(k) = it {
                    Some(*k)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Build an editor state sitting on the Mounts tab with a single mount
    /// pointing at `src`. The cursor is on row 0 (the mount we just added).
    fn editor_at_mounts_row0(src: &str) -> EditorState<'static> {
        let ws = WorkspaceConfig {
            workdir: String::new(),
            mounts: vec![MountConfig {
                src: src.to_string(),
                dst: src.to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    #[test]
    fn github_mount_row_includes_open_in_github_hint() {
        // Build a synthetic GitHub repo on-disk so `mount_info::inspect`
        // classifies the source as `MountKind::Git { host: Github, web_url: Some }`.
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            git_dir.join("config"),
            r#"[remote "origin"]
    url = git@github.com:owner/repo.git
"#,
        )
        .unwrap();

        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        let hint = contextual_row_items(&editor);
        let keys = key_glyphs(&hint);
        let labels = text_labels(&hint);
        assert!(
            keys.contains(&"O"),
            "GitHub mount row must include `O` key hint; got keys={keys:?}"
        );
        assert!(
            labels.contains(&"open in GitHub"),
            "GitHub mount row must include `open in GitHub` label; got labels={labels:?}"
        );
        // Composes with the existing D/A pair, so all three keys are present.
        assert!(keys.contains(&"D"));
        assert!(keys.contains(&"A"));
    }

    #[test]
    fn non_github_mount_row_omits_open_in_github_hint() {
        // Plain folder (no .git) — no GitHub URL, so `O` must not appear.
        let tmp = tempfile::tempdir().unwrap();
        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        let hint = contextual_row_items(&editor);
        let keys = key_glyphs(&hint);
        assert!(
            !keys.contains(&"O"),
            "plain-folder mount must not include `O`; got keys={keys:?}"
        );
        // But the existing D/A hints must still be present.
        assert!(keys.contains(&"D"));
        assert!(keys.contains(&"A"));
    }

    #[test]
    fn mount_row_includes_toggle_readonly_hint() {
        // Every mount-data row must surface `R toggle ro/rw`, regardless of
        // whether the row is a GitHub repo. Plain-folder case — confirms the
        // hint composes alongside D/A even without the O extension.
        let tmp = tempfile::tempdir().unwrap();
        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        let hint = contextual_row_items(&editor);
        let keys = key_glyphs(&hint);
        let labels = text_labels(&hint);
        assert!(
            keys.contains(&"R"),
            "mount data row must include `R` key hint; got keys={keys:?}"
        );
        assert!(
            labels.contains(&"toggle ro/rw"),
            "mount data row must include `toggle ro/rw` label; got labels={labels:?}"
        );
    }

    #[test]
    fn mounts_sentinel_row_omits_toggle_readonly_hint() {
        // The `+ Add mount` sentinel has nothing to toggle — R must not
        // appear on that row's footer. Confirms the hint is cursor-aware.
        let tmp = tempfile::tempdir().unwrap();
        let mut editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        editor.active_field = FieldFocus::Row(editor.pending.mounts.len());
        let hint = contextual_row_items(&editor);
        let keys = key_glyphs(&hint);
        assert!(
            !keys.contains(&"R"),
            "sentinel row must not advertise R; got keys={keys:?}"
        );
    }

    /// Guard that every footer hint built by `contextual_row_items` exposes
    /// single-letter hotkeys in uppercase. Multi-character glyphs (Enter,
    /// Tab, Esc, arrows, `*`) pass through unchanged.
    #[test]
    fn footer_hotkeys_are_uppercase() {
        // A representative spread: Mounts (data row + sentinel) + Agents.
        // General row 0 Edit + Create uses only `Enter`, which is multi-char.
        let tmp = tempfile::tempdir().unwrap();
        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());

        // Mounts data-row hint.
        let mounts_row = contextual_row_items(&editor);
        assert_hint_hotkeys_uppercase(&mounts_row, "Mounts row 0");

        // Mounts sentinel "+ Add mount" row.
        let mut sentinel_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        sentinel_editor.active_field = FieldFocus::Row(sentinel_editor.pending.mounts.len());
        let sentinel_row = contextual_row_items(&sentinel_editor);
        assert_hint_hotkeys_uppercase(&sentinel_row, "Mounts sentinel");

        // Agents tab uses Space + `*` — both multi-char / non-alpha.
        let mut agents_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        agents_editor.active_tab = EditorTab::Agents;
        let agents_row = contextual_row_items(&agents_editor);
        assert_hint_hotkeys_uppercase(&agents_row, "Agents");
    }

    /// Scan a footer-hint list and assert every single-character `Key`
    /// alphabetic glyph is uppercase. Multi-character glyphs (Enter, Tab,
    /// Esc, arrows, etc.) and non-alpha keys (`*`) pass through.
    fn assert_hint_hotkeys_uppercase(hint: &[FooterItem], context: &str) {
        for item in hint {
            if let FooterItem::Key(k) = item {
                let chars: Vec<char> = k.chars().collect();
                if chars.len() == 1 {
                    let c = chars[0];
                    if c.is_alphabetic() {
                        assert!(
                            c.is_uppercase(),
                            "[{context}] single-letter hotkey must be uppercase; got {k:?}"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod agents_tab_render_tests {
    //! Pins the `[x]` / `[ ]` glyph on each agent row to the
    //! *effectively allowed* state, not literal `allowed_agents` list
    //! membership. An empty list is the shorthand for "all allowed",
    //! and every row must render `[x]` in that mode.
    use super::render_agents_tab;
    use crate::config::{AgentSource, AppConfig};
    use crate::console::manager::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::WorkspaceConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn ws_with_allowed(names: &[&str]) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: String::new(),
            mounts: Vec::new(),
            allowed_agents: names.iter().map(|s| (*s).into()).collect(),
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.agents.insert((*name).into(), AgentSource::default());
        }
        config
    }

    fn render_to_dump(ws: WorkspaceConfig, config: &AppConfig) -> String {
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Agents;
        editor.active_field = FieldFocus::Row(0);
        let backend = TestBackend::new(60, 10);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_agents_tab(f, Rect::new(0, 0, 60, 10), &editor, config);
        })
        .unwrap();
        let buf = term.backend().buffer();
        // Collapse the buffer to newline-delimited rows so the test
        // assertion can match per-row semantics ("row N contains `[x]`").
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn in_all_mode_all_rows_render_as_checked() {
        // Empty `allowed_agents` ⇒ "all" mode ⇒ every row is `[x]`.
        let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
        let ws = ws_with_allowed(&[]);
        let dump = render_to_dump(ws, &cfg);

        // Every agent name should appear on a line that also carries `[x]`.
        for name in ["alpha", "beta", "gamma"] {
            let line = dump
                .lines()
                .find(|l| l.contains(name))
                .unwrap_or_else(|| panic!("agent `{name}` not rendered in:\n{dump}"));
            assert!(
                line.contains("[x]"),
                "in 'all' mode agent `{name}` row must render `[x]`; got `{line}`"
            );
            assert!(
                !line.contains("[ ]"),
                "in 'all' mode agent `{name}` must not render `[ ]`; got `{line}`"
            );
        }
    }

    #[test]
    fn in_custom_mode_only_listed_agents_show_checked() {
        // Non-empty list ⇒ "custom" mode ⇒ only listed rows are `[x]`.
        let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
        let ws = ws_with_allowed(&["beta"]);
        let dump = render_to_dump(ws, &cfg);

        let beta_line = dump
            .lines()
            .find(|l| l.contains("beta"))
            .expect("beta must render");
        assert!(
            beta_line.contains("[x]"),
            "listed agent `beta` must render `[x]`; got `{beta_line}`"
        );

        for name in ["alpha", "gamma"] {
            let line = dump
                .lines()
                .find(|l| l.contains(name))
                .unwrap_or_else(|| panic!("agent `{name}` not rendered in:\n{dump}"));
            assert!(
                line.contains("[ ]"),
                "unlisted agent `{name}` must render `[ ]` in 'custom' mode; got `{line}`"
            );
        }
    }
}
