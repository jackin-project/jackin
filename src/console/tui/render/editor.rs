//! Editor-stage rendering.
//!
//! Full-screen editor with header, tab bar, per-tab body renderers
//! (General / Mounts / Roles / Secrets), and the contextual footer
//! composition that varies with the active tab + cursor.

use crate::config::AppConfig;
use crate::console::domain::resolve_panel_mode;
use crate::console::tui::components::mount_display::format_mount_rows_with_cache;
pub use crate::console::tui::state::AuthRow;
#[cfg(test)]
pub(crate) use crate::console::tui::state::SecretsRow;
use crate::console::tui::state::{
    EditorMode, EditorState, EditorTab, FieldFocus, SecretsScopeTag,
};
use crate::console::tui::render::env_value_secret_display;
pub(crate) use crate::console::tui::state::{
    auth_flat_rows, secrets_flat_rows, synthesize_appconfig_for_auth, workspace_name_for_panel,
};
#[cfg(test)]
pub(crate) use crate::console::tui::state::{
    eligible_agents_for_override, resolve_auth_row_target,
};
use crate::operator_env::EnvValue;
use jackin_console::tui::components::editor_rows::{
    AuthSourceDisplay, AuthSourceValue, auth_source_display_for_required_env, render_tab_strip,
};
use jackin_console::tui::screens::editor::view::{
    EditorAuthLineRow, EditorRoleRow, auth_lines as editor_auth_lines, editor_frame_areas,
    general_lines as editor_general_lines,
    mount_lines as editor_mount_lines, role_lines as editor_role_lines,
    secret_lines as editor_secret_lines, tab_labels,
};
use jackin_console::tui::view::{footer_height, render_footer, render_header};
use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
};

// ── Editor stage ────────────────────────────────────────────────────

pub(super) fn render_editor(
    frame: &mut Frame,
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
    op_available: bool,
) {
    let items =
        crate::console::tui::render::footer::editor::editor_footer_items(state, config, op_available);
    let footer_h = footer_height(&items, area.width).max(1);
    let areas = editor_frame_areas(area, footer_h);

    let title = match &state.mode {
        EditorMode::Edit { name } => format!("edit workspace · {name}"),
        EditorMode::Create => "create workspace".to_string(),
    };
    render_header(frame, areas.header, &title);
    render_editor_tab_strip(
        frame,
        areas.tabs,
        state.active_tab,
        state.tab_bar_focused,
        state.hovered_tab,
    );

    match state.active_tab {
        EditorTab::General => render_general_tab(frame, areas.body, state),
        EditorTab::Mounts => render_mounts_tab(frame, areas.body, state),
        EditorTab::Roles => render_roles_tab(frame, areas.body, state, config),
        EditorTab::Secrets => render_secrets_tab(frame, areas.body, state, config),
        EditorTab::Auth => render_auth_tab(frame, areas.body, state, config),
    }

    render_footer(frame, areas.footer, &items);
}

fn render_editor_tab_strip(
    frame: &mut Frame,
    area: Rect,
    active: EditorTab,
    tab_bar_focused: bool,
    hovered: Option<usize>,
) {
    render_tab_strip(frame, area, &tab_labels(active), tab_bar_focused, hovered);
}

fn render_general_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>) {
    let rows = general_tab_lines(state);
    let focused =
        !state.tab_bar_focused && state.tab_content_scroll_focused && state.modal.is_none();
    super::render_scrollable_block_at(
        frame,
        area,
        rows,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        None,
    );
}

fn general_tab_lines(state: &EditorState<'_>) -> Vec<Line<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    let show_cursor =
        !state.tab_bar_focused && state.tab_content_scroll_focused && state.modal.is_none();

    let name_value = match &state.mode {
        EditorMode::Edit { name } => state.pending_name.as_deref().unwrap_or(name.as_str()),
        EditorMode::Create => state.pending_name.as_deref().unwrap_or("(new)"),
    };
    let workdir_display = crate::tui::shorten_home(&state.pending.workdir);

    editor_general_lines(
        cursor,
        show_cursor,
        name_value,
        &workdir_display,
        state.pending.keep_awake.enabled,
        state.pending.git_pull_on_entry,
    )
}

fn render_mounts_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>) {
    let lines = mounts_tab_lines(state);
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.workspace_mounts_scroll_x,
        state.tab_scroll_y,
        state.workspace_mounts_scroll_focused && state.modal.is_none(),
        None,
    );
}

fn mounts_tab_lines(state: &EditorState<'_>) -> Vec<Line<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    let show_cursor =
        !state.tab_bar_focused && state.workspace_mounts_scroll_focused && state.modal.is_none();
    let rows = format_mount_rows_with_cache(&state.pending.mounts, &state.mount_info_cache);
    editor_mount_lines(&rows, cursor, state.hovered_mount_row, show_cursor)
}

fn render_roles_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>, config: &AppConfig) {
    let lines = roles_tab_lines(state, config);
    let focused =
        !state.tab_bar_focused && state.tab_content_scroll_focused && state.modal.is_none();
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        None,
    );
}

fn roles_tab_lines(state: &EditorState<'_>, config: &AppConfig) -> Vec<Line<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    let show_cursor =
        !state.tab_bar_focused && state.tab_content_scroll_focused && state.modal.is_none();

    let is_all = jackin_console::workspace::allows_all_agents(&state.pending);
    let allowed_count = state.pending.allowed_roles.len();
    let rows: Vec<EditorRoleRow> = config
        .roles
        .keys()
        .map(|role_name| EditorRoleRow {
            name: role_name.clone(),
            effectively_allowed: jackin_console::workspace::agent_is_effectively_allowed(
                &state.pending,
                role_name,
            ),
            is_default: state.pending.default_role.as_deref() == Some(role_name.as_str()),
        })
        .collect();

    editor_role_lines(&rows, allowed_count, is_all, cursor, show_cursor)
}

// Linear match per row kind reads better than scattered helpers.
#[allow(clippy::too_many_lines)]
fn render_secrets_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>, config: &AppConfig) {
    let lines = secrets_tab_lines(area, state, config);
    let focused =
        !state.tab_bar_focused && state.tab_content_scroll_focused && state.modal.is_none();
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        None,
    );
}

fn secrets_tab_lines(
    area: Rect,
    state: &EditorState<'_>,
    config: &AppConfig,
) -> Vec<Line<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    let show_cursor =
        !state.tab_bar_focused && state.tab_content_scroll_focused && state.modal.is_none();

    let rows = secrets_flat_rows(state);
    editor_secret_lines(
        &rows,
        cursor,
        show_cursor,
        area.width,
        |scope, key| match scope {
            SecretsScopeTag::Workspace => state.pending.env.get(key).map(env_value_secret_display),
            SecretsScopeTag::Role(role) => state
                .pending
                .roles
                .get(role)
                .and_then(|role_override| role_override.env.get(key))
                .map(env_value_secret_display),
        },
        |scope, key| state.unmasked_rows.contains(&(scope.clone(), key.to_string())),
        |role| config.roles.contains_key(role),
        |role| state.pending.roles.get(role).map_or(0, |o| o.env.len()),
    )
}

/// Render the Auth tab directly from [`auth_flat_rows`].
///
/// Materializes a synthetic [`AppConfig`] from the editor's pending workspace
/// merged with the (mostly read-only) global layer of the live config so
/// in-flight edits are reflected immediately.
fn render_auth_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>, config: &AppConfig) {
    let lines = auth_tab_lines(state, config);
    let title = state.auth_selected_kind.map(|k| format!(" {} ", k.label()));
    let focused =
        !state.tab_bar_focused && state.tab_content_scroll_focused && state.modal.is_none();
    super::render_scrollable_block_at(
        frame,
        area,
        lines,
        state.tab_scroll_x,
        state.tab_scroll_y,
        focused,
        title.as_deref(),
    );
}

fn auth_tab_lines(state: &EditorState<'_>, config: &AppConfig) -> Vec<Line<'static>> {
    let synthesized = synthesize_appconfig_for_auth(state, config);
    let workspace_name = workspace_name_for_panel(state);
    let rows = auth_flat_rows(state, config);

    let FieldFocus::Row(cursor) = state.active_field;
    let max_idx = rows.len().saturating_sub(1);
    let cursor_clamped = cursor.min(max_idx);
    let show_cursor =
        !state.tab_bar_focused && state.tab_content_scroll_focused && state.modal.is_none();

    let display_rows: Vec<EditorAuthLineRow> = rows
        .iter()
        .map(|row| auth_display_row(row, &synthesized, &workspace_name))
        .collect();
    editor_auth_lines(&display_rows, cursor_clamped, show_cursor)
}

fn auth_display_row(
    row: &AuthRow,
    synthesized: &AppConfig,
    workspace_name: &str,
) -> EditorAuthLineRow {
    use crate::console::tui::components::auth_panel::mode_str;

    match row {
        AuthRow::AuthKindRow { kind } => EditorAuthLineRow::AuthKind {
            label: kind.label().to_string(),
        },
        AuthRow::WorkspaceMode { kind } => {
            let ws = synthesized.workspaces.get(workspace_name);
            let explicit = ws.and_then(|ws| explicit_workspace_mode(ws, *kind));
            let mode = explicit
                .unwrap_or_else(|| resolve_panel_mode(synthesized, *kind, workspace_name, ""));
            EditorAuthLineRow::WorkspaceMode {
                mode_label: mode_str(mode).to_string(),
                inherited: explicit.is_none(),
            }
        }
        AuthRow::WorkspaceSource { kind } => EditorAuthLineRow::WorkspaceSource {
            display: editor_auth_source_display(synthesized, workspace_name, "", *kind),
        },
        AuthRow::RoleHeader { role, expanded } => EditorAuthLineRow::RoleHeader {
            role: role.clone(),
            expanded: *expanded,
        },
        AuthRow::RoleMode { role, kind } => {
            let mode = resolve_panel_mode(synthesized, *kind, workspace_name, role);
            EditorAuthLineRow::RoleMode {
                mode_label: mode_str(mode).to_string(),
            }
        }
        AuthRow::RoleSource { role, kind } => EditorAuthLineRow::RoleSource {
            display: editor_auth_source_display(synthesized, workspace_name, role, *kind),
        },
        AuthRow::AddSentinel { eligible } => EditorAuthLineRow::AddSentinel {
            eligible: *eligible,
        },
        AuthRow::Spacer => EditorAuthLineRow::Spacer,
    }
}

fn editor_auth_source_display(
    synthesized: &AppConfig,
    workspace_name: &str,
    role: &str,
    kind: jackin_console::tui::auth::AuthKind,
) -> AuthSourceDisplay {
    use crate::console::tui::components::auth_panel::mode_str;

    let mode = resolve_panel_mode(synthesized, kind, workspace_name, role);
    let env_name = kind.required_env_var(mode);

    let value = env_name
        .and_then(|env_name| auth_source_value(synthesized, workspace_name, role, env_name, kind))
        .map(|value| match value {
            EnvValue::OpRef(r) => AuthSourceValue::OpRefPath(r.path.clone()),
            EnvValue::Plain(s) => AuthSourceValue::Plain(s.clone()),
        });

    auth_source_display_for_required_env(env_name, value, mode_str(mode))
}

/// Explicit workspace-level mode for a kind, if any.
fn explicit_workspace_mode(
    ws: &crate::workspace::WorkspaceConfig,
    kind: jackin_console::tui::auth::AuthKind,
) -> Option<jackin_console::tui::auth::AuthMode> {
    use crate::console::domain::{auth_mode_from_auth_forward, auth_mode_from_github};
    use jackin_console::tui::auth::{AuthKind, AuthMode};
    match kind {
        AuthKind::Claude => ws
            .claude
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.auth_forward)),
        AuthKind::Codex => ws
            .codex
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.0.auth_forward)),
        AuthKind::Amp => ws
            .amp
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.0.auth_forward)),
        AuthKind::Kimi => ws
            .kimi
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.0.auth_forward)),
        AuthKind::Opencode => ws
            .opencode
            .as_ref()
            .map(|c| auth_mode_from_auth_forward(c.0.auth_forward)),
        AuthKind::Github => ws
            .github
            .as_ref()
            .map(|g| auth_mode_from_github(g.auth_forward)),
        AuthKind::Zai => {
            if ws.env.contains_key("ZAI_API_KEY") {
                Some(AuthMode::ApiKey)
            } else {
                None
            }
        }
    }
}

/// Walk env layers for a credential lookup. Github's env map lives
/// under `[…github.env]` (parallel to global `[github.env]`); the
/// agent kinds use `[…env]` directly.
fn auth_source_value<'a>(
    synthesized: &'a AppConfig,
    workspace_name: &str,
    role: &str,
    env_name: &str,
    kind: jackin_console::tui::auth::AuthKind,
) -> Option<&'a EnvValue> {
    use jackin_console::tui::auth::AuthKind;
    match kind {
        AuthKind::Github => github_source_value(synthesized, workspace_name, role, env_name),
        AuthKind::Claude
        | AuthKind::Codex
        | AuthKind::Amp
        | AuthKind::Kimi
        | AuthKind::Opencode
        | AuthKind::Zai => agent_env_source_value(synthesized, workspace_name, role, env_name),
    }
}

fn agent_env_source_value<'a>(
    synthesized: &'a AppConfig,
    workspace_name: &str,
    role: &str,
    env_name: &str,
) -> Option<&'a EnvValue> {
    if !role.is_empty()
        && let Some(value) = synthesized
            .workspaces
            .get(workspace_name)
            .and_then(|ws| ws.roles.get(role))
            .and_then(|ro| ro.env.get(env_name))
    {
        return Some(value);
    }
    if let Some(value) = synthesized
        .workspaces
        .get(workspace_name)
        .and_then(|ws| ws.env.get(env_name))
    {
        return Some(value);
    }
    if !role.is_empty()
        && let Some(value) = synthesized
            .roles
            .get(role)
            .and_then(|r| r.env.get(env_name))
    {
        return Some(value);
    }
    synthesized.env.get(env_name)
}

/// Lookup an env value for the GitHub kind, layered most-specific first
/// across the `[github.env]` blocks. Mirrors
/// [`crate::config::build_github_env_layers`] precedence.
fn github_source_value<'a>(
    synthesized: &'a AppConfig,
    workspace_name: &str,
    role: &str,
    env_name: &str,
) -> Option<&'a EnvValue> {
    if !role.is_empty()
        && let Some(value) = synthesized
            .workspaces
            .get(workspace_name)
            .and_then(|ws| ws.roles.get(role))
            .and_then(|ro| ro.github.as_ref())
            .and_then(|g| g.env.get(env_name))
    {
        return Some(value);
    }
    if let Some(value) = synthesized
        .workspaces
        .get(workspace_name)
        .and_then(|ws| ws.github.as_ref())
        .and_then(|g| g.env.get(env_name))
    {
        return Some(value);
    }
    synthesized
        .github
        .as_ref()
        .and_then(|g| g.env.get(env_name))
}

#[cfg(test)]
mod contextual_row_items_tests {
    //! Row-specific footer-hint composition for the editor tabs.

    use crate::config::{AppConfig, RoleSource};
    use crate::console::tui::render::footer::editor::contextual_row_items;
    use crate::console::tui::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use jackin_tui::HintSpan;

    /// Collect every `HintSpan::Text` label from a hint list.
    fn text_labels<'a>(items: &'a [HintSpan<'a>]) -> Vec<&'a str> {
        items
            .iter()
            .filter_map(|it| {
                if let HintSpan::Text(t) = it {
                    Some(*t)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Collect every `HintSpan::Key` glyph from a hint list.
    fn key_glyphs<'a>(items: &'a [HintSpan<'a>]) -> Vec<&'a str> {
        items
            .iter()
            .filter_map(|it| {
                if let HintSpan::Key(k) = it {
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
            mounts: vec![MountConfig {
                src: src.to_string(),
                dst: src.to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.roles.insert((*name).into(), RoleSource::default());
        }
        config
    }

    #[test]
    fn github_mount_row_includes_open_in_github_hint() {
        // Build a synthetic GitHub repo on-disk so `mount_info::inspect`
        // classifies the source as `MountKind::Git { origin: Some(GitOrigin::Github { .. }) }`.
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
        editor.mount_info_cache.store_entries([(
            tmp.path().display().to_string(),
            jackin_console::mount_info::inspect(&tmp.path().display().to_string()),
        )]);
        let config = AppConfig::default();
        let hint = contextual_row_items(&editor, &config, true);
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
        let config = AppConfig::default();
        let hint = contextual_row_items(&editor, &config, true);
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
        let config = AppConfig::default();
        let hint = contextual_row_items(&editor, &config, true);
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
        let config = AppConfig::default();
        let hint = contextual_row_items(&editor, &config, true);
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
        // A representative spread: Mounts (data row + sentinel) + Roles.
        // General row 0 Edit + Create uses only `Enter`, which is multi-char.
        let tmp = tempfile::tempdir().unwrap();
        let editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        let config = config_with_agents(&["agent-smith"]);

        // Mounts data-row hint.
        let mounts_row = contextual_row_items(&editor, &config, true);
        assert_hint_hotkeys_uppercase(&mounts_row, "Mounts row 0");

        // Mounts sentinel "+ Add mount" row.
        let mut sentinel_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        sentinel_editor.active_field = FieldFocus::Row(sentinel_editor.pending.mounts.len());
        let sentinel_row = contextual_row_items(&sentinel_editor, &config, true);
        assert_hint_hotkeys_uppercase(&sentinel_row, "Mounts sentinel");

        // Roles tab uses Space + `*` — both multi-char / non-alpha.
        let mut roles_editor = editor_at_mounts_row0(tmp.path().to_str().unwrap());
        roles_editor.active_tab = EditorTab::Roles;
        let roles_row = contextual_row_items(&roles_editor, &config, true);
        assert_hint_hotkeys_uppercase(&roles_row, "Roles");
    }

    /// Scan a footer-hint list and assert every single-character `Key`
    /// alphabetic glyph is uppercase. Multi-character glyphs (Enter, Tab,
    /// Esc, arrows, etc.) and non-alpha keys (`*`) pass through.
    fn assert_hint_hotkeys_uppercase(hint: &[HintSpan<'_>], context: &str) {
        for item in hint {
            if let HintSpan::Key(k) = item {
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
mod general_tab_render_tests {
    use super::render_general_tab;
    use crate::config::AppConfig;
    use crate::console::tui::layout::editor::prepare_editor_tab_for_area;
    use crate::console::tui::state::{EditorState, FieldFocus};
    use crate::workspace::WorkspaceConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    #[test]
    fn general_tab_clamps_horizontal_scroll_with_shared_scrollable_block() {
        let ws = WorkspaceConfig {
            workdir: "/workspace/path/that/is/long/enough/to/require/horizontal/scrolling".into(),
            ..Default::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_field = FieldFocus::Row(1);
        editor.tab_content_scroll_focused = true;
        editor.tab_scroll_x = u16::MAX;
        let area = Rect::new(0, 0, 42, 8);
        prepare_editor_tab_for_area(area, &mut editor, &AppConfig::default());

        let backend = TestBackend::new(42, 8);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_general_tab(f, area, &editor);
        })
        .unwrap();

        let viewport = super::super::scroll_viewport_width(area);
        assert_eq!(
            editor.tab_scroll_x,
            jackin_tui::components::scrollable_panel::max_offset(
                editor.tab_content_width,
                viewport
            )
        );
        assert!(editor.tab_scroll_x > 0);
    }
}

#[cfg(test)]
mod mounts_tab_render_tests {
    use super::render_editor;
    use crate::config::AppConfig;
    use crate::console::tui::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::{MountConfig, WorkspaceConfig};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn readonly_mount_renders_ro_mode() {
        let ws = WorkspaceConfig {
            mounts: vec![MountConfig {
                src: "/host/a".into(),
                dst: "/host/a".into(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Mounts;
        editor.tab_bar_focused = false;
        editor.active_field = FieldFocus::Row(0);

        let config = AppConfig::default();
        let backend = TestBackend::new(80, 10);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_editor(f, f.area(), &mut editor, &config, true);
        })
        .unwrap();

        let buf = term.backend().buffer();
        let found = (0..buf.area.height).any(|y| {
            let row = (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>();
            row.contains(" ro ") || row.trim_end().ends_with(" ro") || row.contains(" ro  ")
        });
        assert!(
            found,
            "readonly mount render must show `ro` in the mode column"
        );
    }
}

#[cfg(test)]
mod agents_tab_render_tests {
    //! Pins `[x]`/`[ ]` to the *effectively allowed* state — empty
    //! `allowed_roles` is the "all allowed" shorthand.
    use super::render_roles_tab;
    use crate::config::{AppConfig, RoleSource};
    use crate::console::tui::layout::editor::prepare_editor_tab_for_area;
    use crate::console::tui::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::WorkspaceConfig;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn ws_with_allowed(names: &[&str]) -> WorkspaceConfig {
        WorkspaceConfig {
            allowed_roles: names.iter().map(|s| (*s).into()).collect(),
            ..WorkspaceConfig::default()
        }
    }

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.roles.insert((*name).into(), RoleSource::default());
        }
        config
    }

    fn render_to_dump(ws: WorkspaceConfig, config: &AppConfig) -> String {
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Roles;
        editor.active_field = FieldFocus::Row(0);
        let backend = TestBackend::new(60, 10);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_roles_tab(f, Rect::new(0, 0, 60, 10), &editor, config);
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
        // Empty `allowed_roles` ⇒ "all" mode ⇒ every row is `[x]`.
        let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
        let ws = ws_with_allowed(&[]);
        let dump = render_to_dump(ws, &cfg);

        // Every role name should appear on a line that also carries `[x]`.
        for name in ["alpha", "beta", "gamma"] {
            let line = dump
                .lines()
                .find(|l| l.contains(name))
                .unwrap_or_else(|| panic!("role `{name}` not rendered in:\n{dump}"));
            assert!(
                line.contains("[x]"),
                "in 'all' mode role `{name}` row must render `[x]`; got `{line}`"
            );
            assert!(
                !line.contains("[ ]"),
                "in 'all' mode role `{name}` must not render `[ ]`; got `{line}`"
            );
        }
    }

    #[test]
    fn roles_tab_clamps_horizontal_scroll_with_shared_state() {
        let cfg =
            config_with_agents(&["chainargos/agent-brown-with-extra-long-role-name-for-scroll"]);
        let ws = ws_with_allowed(&[]);
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Roles;
        editor.active_field = FieldFocus::Row(0);
        editor.tab_content_scroll_focused = true;
        editor.tab_scroll_x = u16::MAX;
        let area = Rect::new(0, 0, 42, 8);
        prepare_editor_tab_for_area(area, &mut editor, &cfg);
        let backend = TestBackend::new(42, 8);
        let mut term = Terminal::new(backend).unwrap();

        term.draw(|f| {
            render_roles_tab(f, area, &editor, &cfg);
        })
        .unwrap();

        let viewport = super::super::scroll_viewport_width(area);
        assert_eq!(
            editor.tab_scroll_x,
            jackin_tui::components::scrollable_panel::max_offset(
                editor.tab_content_width,
                viewport
            )
        );
        assert!(editor.tab_scroll_x > 0);
    }

    /// The default-role row carries the `★` marker; non-default rows
    /// render a plain space in the marker column. Pins the glyph that
    /// the `*` keybinding produces in the rendered list.
    #[test]
    fn default_agent_row_carries_star_marker() {
        let cfg = config_with_agents(&["alpha", "beta", "gamma"]);
        let mut ws = ws_with_allowed(&[]);
        ws.default_role = Some("beta".into());
        let dump = render_to_dump(ws, &cfg);

        let beta_line = dump
            .lines()
            .find(|l| l.contains("beta"))
            .expect("beta must render");
        assert!(
            beta_line.contains('\u{2605}'),
            "default role row must carry the `★` marker; got `{beta_line}`"
        );

        let alpha_line = dump
            .lines()
            .find(|l| l.contains("alpha"))
            .expect("alpha must render");
        assert!(
            !alpha_line.contains('\u{2605}'),
            "non-default rows must not carry `★`; got `{alpha_line}`"
        );
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
            "listed role `beta` must render `[x]`; got `{beta_line}`"
        );

        for name in ["alpha", "gamma"] {
            let line = dump
                .lines()
                .find(|l| l.contains(name))
                .unwrap_or_else(|| panic!("role `{name}` not rendered in:\n{dump}"));
            assert!(
                line.contains("[ ]"),
                "unlisted role `{name}` must render `[ ]` in 'custom' mode; got `{line}`"
            );
        }
    }
}

#[cfg(test)]
mod secrets_tab_render_tests {
    //! Render-buffer tests for the Secrets tab. Verifies the masking
    //! default, the unmasked literal-value path, and that the flat-row
    //! builder honours `secrets_expanded` for per-role override sections.
    use super::render_secrets_tab;
    use crate::config::AppConfig;
    use crate::console::tui::state::{EditorState, EditorTab, FieldFocus, SecretsScopeTag};
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    /// Build an editor sitting on the Secrets tab with a single
    /// workspace-level env key (`DB_URL = postgres://localhost/db`).
    fn editor_with_workspace_env() -> EditorState<'static> {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DB_URL".into(), "postgres://localhost/db".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    /// Build an editor sitting on the Secrets tab with one role override
    /// carrying a single env key (`agent-smith`: `LOG_LEVEL = debug`).
    fn editor_with_agent_override() -> EditorState<'static> {
        let mut role_env = std::collections::BTreeMap::new();
        role_env.insert("LOG_LEVEL".into(), "debug".into());
        let mut roles = std::collections::BTreeMap::new();
        roles.insert(
            "agent-smith".into(),
            WorkspaceRoleOverride {
                env: role_env,
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
            },
        );
        let ws = WorkspaceConfig {
            roles,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    /// Render the Secrets tab to a 80x15 `TestBackend`, return the raw
    /// buffer as newline-delimited rows so tests can search for glyphs.
    fn render_to_dump(editor: &EditorState<'_>) -> String {
        let config = AppConfig::default();
        let backend = TestBackend::new(80, 15);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_secrets_tab(f, Rect::new(0, 0, 80, 15), editor, &config);
        })
        .unwrap();
        let buf = term.backend().buffer();
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
    fn secrets_tab_defaults_to_masked() {
        // `new_edit` leaves `unmasked_rows` empty, so every plain-text
        // value renders masked by default.
        let editor = editor_with_workspace_env();
        assert!(
            editor.unmasked_rows.is_empty(),
            "new_edit must leave unmasked_rows empty (default = all masked)"
        );
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("●●●●●●●●●●●"),
            "masked-default render must show the mask glyph; got:\n{dump}"
        );
        assert!(
            !dump.contains("postgres://localhost/db"),
            "masked-default render must hide the literal value; got:\n{dump}"
        );
    }

    #[test]
    fn secrets_tab_unmasked_shows_literal_value() {
        let mut editor = editor_with_workspace_env();
        editor
            .unmasked_rows
            .insert((SecretsScopeTag::Workspace, "DB_URL".into()));
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("postgres://localhost/db"),
            "unmasked render must show literal value; got:\n{dump}"
        );
        assert!(
            !dump.contains("●●●●●●●●●●●"),
            "unmasked render must not show the mask glyph; got:\n{dump}"
        );
    }

    #[test]
    fn secrets_tab_collapsed_agent_omits_key_rows() {
        // `secrets_expanded` is empty by default (set by `new_edit`), so
        // the role section header renders but its `LOG_LEVEL` key row
        // does not.
        let editor = editor_with_agent_override();
        assert!(editor.secrets_expanded.is_empty());
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("agent-smith"),
            "role header must render; got:\n{dump}"
        );
        assert!(
            !dump.contains("LOG_LEVEL"),
            "collapsed role section must omit key rows; got:\n{dump}"
        );
    }

    #[test]
    fn secrets_tab_expanded_agent_shows_key_rows() {
        let mut editor = editor_with_agent_override();
        editor.secrets_expanded.insert("agent-smith".into());
        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("agent-smith"),
            "role header must still render when expanded; got:\n{dump}"
        );
        assert!(
            dump.contains("LOG_LEVEL"),
            "expanded role section must show its key rows; got:\n{dump}"
        );
    }

    #[test]
    fn secrets_tab_cursor_skips_workspace_header_label() {
        let editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        let rows = super::secrets_flat_rows(&editor);
        assert!(
            !rows.is_empty(),
            "secrets_flat_rows must always include at least the WorkspaceAddSentinel"
        );
        assert!(
            matches!(rows.first(), Some(super::SecretsRow::WorkspaceAddSentinel)),
            "row 0 must be the focusable `+ Add` sentinel, not a header; got {:?}",
            rows.first()
        );
        assert!(
            matches!(editor.active_field, FieldFocus::Row(0)),
            "editor must open on row 0 = sentinel"
        );
    }

    /// Pins the exact flat-row sequence for a workspace with env vars,
    /// one expanded role (with keys), and one collapsed role. Cursor
    /// arithmetic in `input/editor.rs` is derived directly from this
    /// sequence, so a wrong order causes silent wrong-row selections.
    #[test]
    fn secrets_flat_rows_sequence_is_canonical() {
        use crate::workspace::WorkspaceRoleOverride;

        let mut env = std::collections::BTreeMap::new();
        env.insert("ALPHA".into(), "1".into());
        env.insert("BETA".into(), "2".into());

        let mut role_env = std::collections::BTreeMap::new();
        role_env.insert("KEY".into(), "v".into());

        let mut roles = std::collections::BTreeMap::new();
        roles.insert(
            "agent-a".into(),
            WorkspaceRoleOverride {
                env: role_env,
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
            },
        );
        roles.insert(
            "agent-b".into(),
            WorkspaceRoleOverride {
                env: std::collections::BTreeMap::new(),
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
            },
        );

        let ws = WorkspaceConfig {
            env,
            roles,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        // Expand agent-a, leave agent-b collapsed.
        editor.secrets_expanded.insert("agent-a".into());

        let rows = super::secrets_flat_rows(&editor);
        // Expected sequence:
        //  0  WorkspaceKeyRow("ALPHA")
        //  1  WorkspaceKeyRow("BETA")
        //  2  SectionSpacer
        //  3  WorkspaceAddSentinel
        //  4  SectionSpacer
        //  5  AgentHeader { role: "agent-a", expanded: true }
        //  6  AgentKeyRow { role: "agent-a", key: "KEY" }
        //  7  SectionSpacer
        //  8  AgentAddSentinel("agent-a")
        //  9  SectionSpacer
        // 10  AgentHeader { role: "agent-b", expanded: false }
        assert_eq!(rows.len(), 11, "unexpected row count: {rows:?}");
        assert!(matches!(&rows[0], super::SecretsRow::WorkspaceKeyRow(k) if k == "ALPHA"));
        assert!(matches!(&rows[1], super::SecretsRow::WorkspaceKeyRow(k) if k == "BETA"));
        assert!(matches!(&rows[2], super::SecretsRow::SectionSpacer));
        assert!(matches!(&rows[3], super::SecretsRow::WorkspaceAddSentinel));
        assert!(matches!(&rows[4], super::SecretsRow::SectionSpacer));
        assert!(
            matches!(&rows[5], super::SecretsRow::RoleHeader { role, expanded: true } if role == "agent-a")
        );
        assert!(
            matches!(&rows[6], super::SecretsRow::RoleKeyRow { role, key } if role == "agent-a" && key == "KEY")
        );
        assert!(matches!(&rows[7], super::SecretsRow::SectionSpacer));
        assert!(matches!(&rows[8], super::SecretsRow::RoleAddSentinel(a) if a == "agent-a"));
        assert!(matches!(&rows[9], super::SecretsRow::SectionSpacer));
        assert!(
            matches!(&rows[10], super::SecretsRow::RoleHeader { role, expanded: false } if role == "agent-b")
        );
    }

    #[test]
    fn secrets_tab_empty_renders_only_sentinel() {
        let editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        let dump = render_to_dump(&editor);

        assert!(
            dump.contains("+ Add environment variable"),
            "the `+ Add environment variable` sentinel must render; dump:\n{dump}"
        );
        assert!(
            !dump.contains("Workspace env"),
            "the `Workspace env` preamble label must NOT render; dump:\n{dump}"
        );
        assert!(
            !dump.contains("(no env vars)"),
            "the `(no env vars)` placeholder must NOT render; dump:\n{dump}"
        );
        assert!(
            !dump.contains("env var"),
            "TUI text must say `environment variable`, not `env var`; dump:\n{dump}"
        );
    }

    #[test]
    fn op_row_breadcrumb_render_three_segment() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://Work/db/password".into(),
                path: "Work/db/password".into(),
                account: None,
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("Work"),
            "breadcrumb must render vault segment; dump:\n{dump}"
        );
        assert!(
            dump.contains("db"),
            "breadcrumb must render item segment; dump:\n{dump}"
        );
        assert!(
            dump.contains("password"),
            "breadcrumb must render field segment; dump:\n{dump}"
        );
        assert!(
            dump.contains("\u{2192}"),
            "breadcrumb must include the → glyph between item and field; dump:\n{dump}"
        );
        assert!(
            !dump.contains("op://"),
            "op:// scheme prefix must not appear in the breadcrumb; dump:\n{dump}"
        );
        // Mask glyph must not appear on OpRef rows even though
        // editor defaults to all-masked.
        assert!(
            editor.unmasked_rows.is_empty(),
            "default state is all-masked; OpRef rows must still bypass masking"
        );
        assert!(
            !dump.contains("●●●"),
            "OpRef rows must never render the mask glyph; dump:\n{dump}"
        );
    }

    /// 4-segment is `vault/item/section/field` per the 1Password CLI
    /// syntax — not the earlier `account/vault/item/field` reading.
    #[test]
    fn op_row_breadcrumb_render_four_segment_with_section() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "API_KEY".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://Personal/API Keys/auth/secret_key".into(),
                path: "Personal/API Keys/auth/secret_key".into(),
                account: None,
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        // All four components must appear, in order, with the arrow
        // glyph between the section and the field.
        assert!(
            dump.contains("Personal"),
            "vault must render; dump:\n{dump}"
        );
        assert!(dump.contains("API Keys"), "item must render; dump:\n{dump}");
        assert!(
            dump.contains("auth"),
            "section must render between item and field; dump:\n{dump}"
        );
        assert!(
            dump.contains("secret_key"),
            "field must render; dump:\n{dump}"
        );
        assert!(
            dump.contains("\u{2192}"),
            "arrow glyph must precede the field; dump:\n{dump}"
        );
        // The account-prefix branch is dead — no email-style rendering
        // for 4-segment refs.
        assert!(
            !dump.contains('@'),
            "4-segment refs must not render an account email prefix; dump:\n{dump}"
        );
    }

    /// Text marker (not glyph) — `⚿` rendered inconsistently across
    /// terminals; `[op]` reads as "1Password" at a glance.
    #[test]
    fn op_row_renders_with_op_text_marker() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://Work/db/password".into(),
                path: "Work/db/password".into(),
                account: None,
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("[op]"),
            "OpRef row must render the `[op]` text marker; dump:\n{dump}"
        );
        assert!(
            !dump.contains("\u{26BF}"),
            "the legacy `⚿` glyph must not appear after the marker swap; dump:\n{dump}"
        );
    }

    #[test]
    fn plain_row_renders_without_op_marker() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DEBUG".into(), "1".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        assert!(
            !dump.contains("[op]"),
            "plain-text row must not render the `[op]` marker; dump:\n{dump}"
        );
    }

    #[test]
    fn op_row_marker_column_is_5_chars_wide_with_brackets() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "DB_URL".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://Work/db/password".into(),
                path: "Work/db/password".into(),
                account: None,
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        assert!(
            dump.contains("[op] "),
            "OpRef row must render the marker as exactly `[op] ` (5 chars \
             including trailing space); dump:\n{dump}"
        );
    }

    #[test]
    fn plain_row_marker_column_is_5_blank_chars_for_alignment() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DEBUG".into(), "1".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // 7-char prefix region = cursor (1..3) + marker (3..8); on
        // a plain row, cells 3..8 are all blanks.
        let backend = TestBackend::new(80, 15);
        let mut term = Terminal::new(backend).unwrap();
        let config = AppConfig::default();
        term.draw(|f| {
            render_secrets_tab(f, Rect::new(0, 0, 80, 15), &editor, &config);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let mut cells = String::new();
        for x in 3..8 {
            cells.push_str(buf[(x, 1)].symbol());
        }
        assert_eq!(
            cells, "     ",
            "plain row marker column (cells 3..8 of row 1) must be 5 \
             blank spaces for alignment; got {cells:?}"
        );
    }

    #[test]
    fn secrets_tab_renders_keys_in_alphabetical_order() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("ZULU".into(), "z".into());
        env.insert("ALPHA".into(), "a".into());
        env.insert("MIKE".into(), "m".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        let alpha = dump.find("ALPHA").expect("ALPHA must appear");
        let mike = dump.find("MIKE").expect("MIKE must appear");
        let zulu = dump.find("ZULU").expect("ZULU must appear");
        assert!(
            alpha < mike && mike < zulu,
            "keys must render alphabetically (ALPHA < MIKE < ZULU); offsets {alpha}/{mike}/{zulu}\n{dump}"
        );
    }

    #[test]
    fn section_spacer_appears_between_workspace_and_first_agent_section() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DB_URL".into(), "postgres://localhost/db".into());
        let mut role_env = std::collections::BTreeMap::new();
        role_env.insert("LOG_LEVEL".into(), "debug".into());
        let mut roles = std::collections::BTreeMap::new();
        roles.insert(
            "agent-smith".into(),
            WorkspaceRoleOverride {
                env: role_env,
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
            },
        );
        let ws = WorkspaceConfig {
            env,
            roles,
            ..WorkspaceConfig::default()
        };
        let editor = EditorState::new_edit("ws".into(), ws);
        let rows = super::secrets_flat_rows(&editor);
        assert!(
            matches!(rows.get(3), Some(super::SecretsRow::SectionSpacer)),
            "row 3 must be a SectionSpacer between workspace add row \
             and first role header; got {:?}",
            rows.get(3)
        );
        assert!(
            matches!(rows.get(4), Some(super::SecretsRow::RoleHeader { .. })),
            "row 4 must be the role header right after the spacer; \
             got {:?}",
            rows.get(4)
        );
    }

    #[test]
    fn section_spacer_appears_between_consecutive_agent_sections() {
        let mut a_env = std::collections::BTreeMap::new();
        a_env.insert("LEVEL_A".into(), "1".into());
        let mut b_env = std::collections::BTreeMap::new();
        b_env.insert("LEVEL_B".into(), "2".into());
        let mut roles = std::collections::BTreeMap::new();
        roles.insert(
            "agent-architect".into(),
            WorkspaceRoleOverride {
                env: a_env,
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
            },
        );
        roles.insert(
            "agent-smith".into(),
            WorkspaceRoleOverride {
                env: b_env,
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                github: None,
            },
        );
        let ws = WorkspaceConfig {
            roles,
            ..WorkspaceConfig::default()
        };
        let editor = EditorState::new_edit("ws".into(), ws);
        let rows = super::secrets_flat_rows(&editor);
        assert!(
            matches!(rows.get(1), Some(super::SecretsRow::SectionSpacer)),
            "spacer expected before the first role header; rows={rows:?}"
        );
        assert!(
            matches!(rows.get(3), Some(super::SecretsRow::SectionSpacer)),
            "spacer expected between consecutive role sections; rows={rows:?}"
        );
        assert!(
            !matches!(rows.last(), Some(super::SecretsRow::SectionSpacer)),
            "no trailing spacer after the final section; rows={rows:?}"
        );
    }

    /// Helper that renders the Secrets tab to a wider (120-column) terminal
    /// so long breadcrumbs (subtitle + section + field) are not truncated.
    fn render_to_dump_wide(editor: &EditorState<'_>) -> String {
        let config = AppConfig::default();
        let backend = TestBackend::new(120, 15);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| {
            render_secrets_tab(f, Rect::new(0, 0, 120, 15), editor, &config);
        })
        .unwrap();
        let buf = term.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    /// `OpRef` whose `path` contains the `[subtitle]` disambiguation form.
    /// The subtitle must appear in the rendered output between the item
    /// name and the next " / " separator.
    #[test]
    fn renderer_op_ref_with_subtitle_renders_text() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "TOKEN".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/fld".into(),
                path: "Private/Claude[alexey@zhokhov.com]/security/auth token".into(),
                account: None,
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // Use the wide terminal so the subtitle and field are not truncated.
        let dump = render_to_dump_wide(&editor);
        // The row must carry the [op] marker (OpRef variant).
        assert!(
            dump.contains("[op]"),
            "OpRef row with subtitle must render `[op]` marker; dump:\n{dump}"
        );
        // Subtitle text must appear in the rendered output.
        assert!(
            dump.contains("alexey@zhokhov.com"),
            "subtitle text must appear in the breadcrumb; dump:\n{dump}"
        );
        // Vault, item, section, and field must all render.
        assert!(dump.contains("Private"), "vault must render; dump:\n{dump}");
        assert!(
            dump.contains("Claude"),
            "item name must render; dump:\n{dump}"
        );
        assert!(
            dump.contains("security"),
            "section must render; dump:\n{dump}"
        );
        assert!(
            dump.contains("auth token"),
            "field must render; dump:\n{dump}"
        );
    }

    /// `OpRef` whose `path` carries an `?attribute=otp` query suffix. The
    /// query must appear in the rendered output after the field name.
    #[test]
    fn renderer_op_ref_with_attribute_query_renders_text() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "OTP".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/fld?attribute=otp".into(),
                path: "Private/GitHub/one-time password?attribute=otp".into(),
                account: None,
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // Use the wide terminal so `?attribute=otp` is not truncated.
        let dump = render_to_dump_wide(&editor);
        // The row must carry the [op] marker.
        assert!(
            dump.contains("[op]"),
            "OpRef row with attribute query must render `[op]` marker; dump:\n{dump}"
        );
        // The query suffix must appear in the output.
        assert!(
            dump.contains("?attribute=otp"),
            "attribute query must appear in breadcrumb; dump:\n{dump}"
        );
        // Field name must also render.
        assert!(
            dump.contains("one-time password"),
            "field must render; dump:\n{dump}"
        );
    }

    /// `OpRef` with BOTH a subtitle disambiguation AND an `?attribute=otp`
    /// query suffix. Asserts that all six visible pieces appear in the
    /// expected left-to-right order: vault → item → subtitle → section →
    /// field → query.
    #[test]
    fn renderer_op_ref_with_subtitle_section_and_query_renders_all() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "TOKEN".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/sec/fld?attribute=otp".into(),
                path: "Private/Claude[alexey@zhokhov.com]/security/auth token?attribute=otp".into(),
                account: None,
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // Use the wide terminal so no piece is truncated.
        let dump = render_to_dump_wide(&editor);

        // All visible pieces must appear in order:
        // vault → item → subtitle → section → field → query.
        let v_pos = dump.find("Private").expect("vault present");
        let i_pos = dump.find("Claude").expect("item present");
        let s_pos = dump.find("alexey@zhokhov.com").expect("subtitle present");
        let sec_pos = dump.find("security").expect("section present");
        let f_pos = dump.find("auth token").expect("field present");
        let q_pos = dump.find("?attribute=otp").expect("query present");
        assert!(v_pos < i_pos, "vault before item");
        assert!(i_pos < s_pos, "item before subtitle");
        assert!(s_pos < sec_pos, "subtitle before section");
        assert!(sec_pos < f_pos, "section before field");
        assert!(f_pos < q_pos, "field before query");
    }

    /// A `Plain` row containing a bare `op://...` string gets NO `[op]`
    /// marker — it renders as a literal masked value, the visual signal
    /// that the operator needs to re-pick it.
    #[test]
    fn renderer_plain_with_bare_op_uri_renders_as_literal_no_breadcrumb() {
        let mut env = std::collections::BTreeMap::new();
        env.insert("DB_URL".into(), "op://Vault/Item/Field".into());
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        let dump = render_to_dump(&editor);
        // Plain rows carrying a legacy op:// string must NOT render the
        // [op] marker — the visual distinction signals the need to re-pick.
        assert!(
            !dump.contains("[op]"),
            "Plain rows must NOT carry [op] marker; dump:\n{dump}"
        );
        // The breadcrumb separators must not appear — this is a plain
        // masked/literal row, not a breadcrumb render.
        assert!(
            !dump.contains(" / Vault / "),
            "Plain op:// strings must not render vault breadcrumb; dump:\n{dump}"
        );
        // The mask glyph must appear (plain row, masked by default).
        assert!(
            dump.contains("●●●"),
            "Plain row must render masked by default; dump:\n{dump}"
        );
    }

    /// Single env var → `label_width` equals key length. Without the explicit
    /// two-space span, the screenshot bug (`CLAUDE_CODE_OAUTH_TOKENPrivate` / ...)
    /// recurs.
    #[test]
    fn renderer_key_value_separator_always_at_least_two_spaces() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/fld".into(),
                path: "Private/Claude/security/auth token".into(),
                account: None,
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);

        // Use the wide terminal so the breadcrumb is not truncated.
        let dump = render_to_dump_wide(&editor);
        assert!(
            dump.contains("CLAUDE_CODE_OAUTH_TOKEN  Private"),
            "expected at least 2 spaces between key and breadcrumb; dump:\n{dump}"
        );
        assert!(
            !dump.contains("CLAUDE_CODE_OAUTH_TOKENPrivate"),
            "no space is the bug; dump:\n{dump}"
        );
    }

    /// `OpRef` whose `path` doesn't parse as a 3- or 4-segment breadcrumb.
    /// The renderer must NOT panic; it shows a re-pick placeholder in the
    /// value column without the `[op]` marker, and must NOT leak the UUID URI.
    #[test]
    fn renderer_op_ref_with_malformed_path_renders_repick_placeholder_no_panic() {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "TOKEN".into(),
            crate::operator_env::EnvValue::OpRef(crate::operator_env::OpRef {
                op: "op://abc/def/fld".into(),
                path: "garbage-no-slashes".into(),
                account: None,
            }),
        );
        let ws = WorkspaceConfig {
            env,
            ..WorkspaceConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        // Unmask so the placeholder is rendered as text rather than ●●●.
        editor
            .unmasked_rows
            .insert((SecretsScopeTag::Workspace, "TOKEN".into()));

        let dump = render_to_dump_wide(&editor);
        // Malformed path → parse_path_breadcrumb returns None → no [op] marker.
        assert!(!dump.contains("[op]"), "no [op] marker; dump:\n{dump}");
        // Re-pick placeholder must be shown instead of the UUID URI.
        assert!(
            dump.contains("<unparseable path \u{2014} re-pick>"),
            "expected re-pick placeholder; dump:\n{dump}"
        );
        // UUID URI must NOT be visible to the operator.
        assert!(
            !dump.contains("op://abc/def/fld"),
            "UUID URI must NOT leak; dump:\n{dump}"
        );
    }
}

#[cfg(test)]
mod eligible_agents_for_override_tests {
    //! Roles already carrying an override are NOT filtered — the
    //! picker can add more keys to an existing override.
    use super::eligible_agents_for_override;
    use crate::config::{AppConfig, RoleSource};
    use crate::console::tui::state::{EditorState, EditorTab, FieldFocus};
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};

    fn config_with_agents(names: &[&str]) -> AppConfig {
        let mut config = AppConfig::default();
        for name in names {
            config.roles.insert((*name).into(), RoleSource::default());
        }
        config
    }

    fn ws_with_overrides(allowed: &[&str], override_agents: &[&str]) -> WorkspaceConfig {
        let mut roles = std::collections::BTreeMap::new();
        for a in override_agents {
            let mut env = std::collections::BTreeMap::new();
            env.insert("LOG_LEVEL".into(), "debug".into());
            roles.insert(
                (*a).into(),
                WorkspaceRoleOverride {
                    env,
                    claude: None,
                    codex: None,
                    amp: None,
                    kimi: None,
                    opencode: None,
                    github: None,
                },
            );
        }
        WorkspaceConfig {
            allowed_roles: allowed.iter().map(|s| (*s).into()).collect(),
            roles,
            ..WorkspaceConfig::default()
        }
    }

    fn editor_for(ws: WorkspaceConfig) -> EditorState<'static> {
        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.active_tab = EditorTab::Secrets;
        editor.active_field = FieldFocus::Row(0);
        editor
    }

    #[test]
    fn eligible_agents_returns_allowed_when_list_non_empty() {
        // Non-empty `allowed_roles` is taken at face value — the
        // result matches the workspace's allowed list verbatim.
        let cfg = config_with_agents(&["agent-smith", "agent-brown", "agent-architect"]);
        let editor = editor_for(ws_with_overrides(&["agent-smith"], &[]));
        let eligible = eligible_agents_for_override(&editor, &cfg);
        assert_eq!(eligible, vec!["agent-smith".to_string()]);
    }

    #[test]
    fn eligible_agents_returns_all_registered_when_allowed_empty() {
        // Empty `allowed_roles` is the "all roles allowed" shorthand —
        // every globally-registered role is eligible.
        let cfg = config_with_agents(&["agent-smith", "agent-brown"]);
        let editor = editor_for(ws_with_overrides(&[], &[]));
        let mut eligible = eligible_agents_for_override(&editor, &cfg);
        eligible.sort();
        assert_eq!(
            eligible,
            vec!["agent-brown".to_string(), "agent-smith".to_string()]
        );
    }

    #[test]
    fn eligible_agents_does_not_filter_by_existing_overrides() {
        // Operators may want to add additional keys to an role that
        // already carries some — the picker must therefore include
        // every allowed role regardless of whether `pending.roles`
        // already lists them.
        let cfg = config_with_agents(&["agent-smith", "agent-brown"]);
        let editor = editor_for(ws_with_overrides(
            &["agent-smith", "agent-brown"],
            &["agent-smith"],
        ));
        let mut eligible = eligible_agents_for_override(&editor, &cfg);
        eligible.sort();
        assert_eq!(
            eligible,
            vec!["agent-brown".to_string(), "agent-smith".to_string()],
            "agent-smith already has overrides but must still appear so the operator can add another key to it"
        );
    }

    #[test]
    fn eligible_agents_returns_empty_when_no_allowed_and_no_registered() {
        // Empty `allowed_roles` shorthand AND no registered roles:
        // the picker would be empty, so the caller is expected to
        // short-circuit and not open the modal.
        let cfg = config_with_agents(&[]);
        let editor = editor_for(ws_with_overrides(&[], &[]));
        let eligible = eligible_agents_for_override(&editor, &cfg);
        assert!(eligible.is_empty());
    }
}

#[cfg(test)]
mod auth_flat_rows_tests {
    use super::{AuthRow, auth_flat_rows, resolve_panel_mode};
    use crate::config::AppConfig;
    use crate::console::tui::state::EditorState;
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
    use jackin_console::tui::auth::{AuthKind, AuthMode};

    #[test]
    fn root_view_lists_auth_kinds_in_design_order() {
        let editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        let rows = auth_flat_rows(&editor, &AppConfig::default());
        assert_eq!(
            rows,
            vec![
                AuthRow::AuthKindRow {
                    kind: AuthKind::Claude,
                },
                AuthRow::AuthKindRow {
                    kind: AuthKind::Codex,
                },
                AuthRow::AuthKindRow {
                    kind: AuthKind::Amp,
                },
                AuthRow::AuthKindRow {
                    kind: AuthKind::Opencode,
                },
                AuthRow::AuthKindRow {
                    kind: AuthKind::Github,
                },
                AuthRow::AuthKindRow {
                    kind: AuthKind::Zai,
                },
            ],
            "root view must list Claude / Codex / Amp / Opencode / Github / Z.AI in this order"
        );
    }

    #[test]
    fn zai_panel_mode_uses_all_operator_env_layers() {
        let mut cfg = AppConfig::default();
        cfg.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("global-key".into()),
        );
        cfg.workspaces
            .insert("global-demo".into(), WorkspaceConfig::default());
        assert_eq!(
            resolve_panel_mode(&cfg, AuthKind::Zai, "global-demo", "the-architect"),
            AuthMode::ApiKey
        );
        cfg.env.clear();

        let mut workspace = WorkspaceConfig::default();
        workspace.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("workspace-key".into()),
        );
        cfg.workspaces.insert("workspace-demo".into(), workspace);
        assert_eq!(
            resolve_panel_mode(&cfg, AuthKind::Zai, "workspace-demo", "the-architect"),
            AuthMode::ApiKey
        );

        cfg.workspaces.remove("workspace-demo");
        let mut role = crate::config::RoleSource::default();
        role.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("role-key".into()),
        );
        cfg.roles.insert("the-architect".into(), role);
        cfg.workspaces
            .insert("role-demo".into(), WorkspaceConfig::default());
        assert_eq!(
            resolve_panel_mode(&cfg, AuthKind::Zai, "role-demo", "the-architect"),
            AuthMode::ApiKey
        );

        cfg.roles.clear();
        let mut workspace_role = WorkspaceConfig::default();
        let mut override_cfg = WorkspaceRoleOverride::default();
        override_cfg.env.insert(
            "ZAI_API_KEY".into(),
            crate::operator_env::EnvValue::Plain("workspace-role-key".into()),
        );
        workspace_role
            .roles
            .insert("the-architect".into(), override_cfg);
        cfg.workspaces
            .insert("workspace-role-demo".into(), workspace_role);
        assert_eq!(
            resolve_panel_mode(&cfg, AuthKind::Zai, "workspace-role-demo", "the-architect"),
            AuthMode::ApiKey
        );

        // No ZAI_API_KEY at any layer → Ignore. This is the branch that
        // suppresses the Source credential row; a regression to ApiKey here
        // would render a phantom row for every Z.AI panel without a key.
        assert_eq!(
            resolve_panel_mode(
                &AppConfig::default(),
                AuthKind::Zai,
                "absent",
                "the-architect"
            ),
            AuthMode::Ignore
        );
    }

    #[test]
    fn role_with_override_renders_collapsed_header_then_sentinel() {
        use crate::config::{AgentAuthConfig, AuthForwardMode};
        use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
        let mut ws = WorkspaceConfig {
            allowed_roles: vec!["the-architect".into(), "agent-smith".into()],
            ..Default::default()
        };
        let over = WorkspaceRoleOverride {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::Ignore,
            }),
            ..Default::default()
        };
        ws.roles.insert("the-architect".into(), over);

        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.auth_selected_kind = Some(AuthKind::Claude);
        let rows = auth_flat_rows(&editor, &AppConfig::default());

        let header_idx = rows
            .iter()
            .position(|r| {
                matches!(
                    r,
                    AuthRow::RoleHeader {
                        role,
                        expanded: false
                    } if role == "the-architect"
                )
            })
            .expect("role override header expected");
        assert!(matches!(
            rows[header_idx],
            AuthRow::RoleHeader { ref role, expanded: false } if role == "the-architect"
        ));
        assert!(matches!(rows[header_idx + 1], AuthRow::Spacer));
        assert!(matches!(
            rows[header_idx + 2],
            AuthRow::AddSentinel { eligible: 1 }
        ));
    }

    #[test]
    fn role_with_override_when_expanded_emits_kind_rows() {
        use crate::config::{AgentAuthConfig, AuthForwardMode, CodexAuthConfig};
        use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
        let mut ws = WorkspaceConfig {
            allowed_roles: vec!["the-architect".into()],
            ..Default::default()
        };
        let over = WorkspaceRoleOverride {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::Ignore,
            }),
            codex: Some(CodexAuthConfig(AgentAuthConfig {
                auth_forward: AuthForwardMode::ApiKey,
            })),
            ..Default::default()
        };
        ws.roles.insert("the-architect".into(), over);

        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.auth_selected_kind = Some(AuthKind::Claude);
        editor.auth_expanded.insert("the-architect".into());
        let rows = auth_flat_rows(&editor, &AppConfig::default());

        let header_pos = rows
            .iter()
            .position(|r| matches!(r, AuthRow::RoleHeader { expanded: true, .. }))
            .expect("expanded role header missing");
        assert!(matches!(
            rows[header_pos + 1],
            AuthRow::RoleMode { ref role, kind: AuthKind::Claude } if role == "the-architect"
        ));
    }

    #[test]
    fn resolve_auth_row_target_picks_workspace_default_for_workspacedefault_row() {
        use crate::console::tui::state::AuthFormTarget;
        use crate::workspace::WorkspaceConfig;

        let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        editor.auth_selected_kind = Some(AuthKind::Claude);
        let cfg = AppConfig::default();
        let rows = auth_flat_rows(&editor, &cfg);
        let workspace_claude_idx = rows
            .iter()
            .position(|r| {
                matches!(
                    r,
                    AuthRow::WorkspaceMode {
                        kind: AuthKind::Claude
                    }
                )
            })
            .unwrap();
        assert_eq!(
            super::resolve_auth_row_target(&editor, &cfg, workspace_claude_idx),
            Some(AuthFormTarget::Workspace {
                kind: AuthKind::Claude
            }),
        );
    }

    #[test]
    fn resolve_auth_row_target_returns_none_for_navigation_and_header_rows() {
        use crate::workspace::WorkspaceConfig;
        let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        editor.auth_selected_kind = Some(AuthKind::Claude);
        let cfg = AppConfig::default();
        let rows = auth_flat_rows(&editor, &cfg);
        for (idx, row) in rows.iter().enumerate() {
            match row {
                AuthRow::AuthKindRow { .. }
                | AuthRow::AddSentinel { .. }
                | AuthRow::Spacer
                | AuthRow::RoleHeader { .. } => assert!(
                    super::resolve_auth_row_target(&editor, &cfg, idx).is_none(),
                    "row {idx} ({row:?}) must not resolve to an editable target"
                ),
                _ => {}
            }
        }
    }

    /// Globally configured `api_key` mode (in `[claude].auth_forward`)
    /// must surface a `WorkspaceSource` row so the operator can set
    /// the credential — even when the workspace has no explicit
    /// `claude` block of its own.
    #[test]
    fn workspace_source_surfaces_when_global_requires_credential() {
        use crate::config::{AgentAuthConfig, AuthForwardMode};
        use crate::workspace::WorkspaceConfig;
        let config = AppConfig {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::ApiKey,
            }),
            ..AppConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        editor.auth_selected_kind = Some(AuthKind::Claude);

        let rows = auth_flat_rows(&editor, &config);
        assert!(
            rows.iter().any(|r| matches!(
                r,
                AuthRow::WorkspaceSource {
                    kind: AuthKind::Claude
                }
            )),
            "global claude.auth_forward = api_key must surface WorkspaceSource row; got {rows:?}"
        );
    }

    /// Selecting the GitHub kind opens a detail view that mirrors the
    /// Claude / Codex shape: workspace mode → spacer → add-sentinel.
    /// The agent dimension is intentionally absent (Github has no per-
    /// agent split).
    #[test]
    fn github_detail_view_emits_workspace_mode_then_sentinel() {
        let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        editor.auth_selected_kind = Some(AuthKind::Github);
        let rows = auth_flat_rows(&editor, &AppConfig::default());
        // Sync mode (the global default) requires no credential — no
        // WorkspaceSource row.
        assert!(
            matches!(
                rows.first(),
                Some(AuthRow::WorkspaceMode {
                    kind: AuthKind::Github
                })
            ),
            "first row must be the GitHub workspace mode; got {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|r| matches!(r, AuthRow::AddSentinel { .. })),
            "+ Override sentinel must be present; got {rows:?}"
        );
    }

    /// Globally configured `token` mode must surface a `WorkspaceSource`
    /// row for `GH_TOKEN` so the operator can set the credential without
    /// chasing an explicit workspace-level `[github]` block.
    #[test]
    fn github_workspace_source_surfaces_for_global_token_mode() {
        use crate::config::{GithubAuthConfig, GithubAuthMode};
        let config = AppConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Token,
                ..Default::default()
            }),
            ..AppConfig::default()
        };
        let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        editor.auth_selected_kind = Some(AuthKind::Github);

        let rows = auth_flat_rows(&editor, &config);
        assert!(
            rows.iter().any(|r| matches!(
                r,
                AuthRow::WorkspaceSource {
                    kind: AuthKind::Github
                }
            )),
            "global github.auth_forward = token must surface WorkspaceSource row; got {rows:?}"
        );
    }

    /// A workspace × role override on the Github kind shows up as a
    /// collapsed `RoleHeader` in the detail view, exactly like Claude /
    /// Codex overrides do.
    #[test]
    fn github_role_override_emits_role_header_when_override_present() {
        use crate::config::{GithubAuthConfig, GithubAuthMode};
        use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
        let mut ws = WorkspaceConfig {
            allowed_roles: vec!["the-architect".into()],
            ..Default::default()
        };
        let over = WorkspaceRoleOverride {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Ignore,
                ..Default::default()
            }),
            ..Default::default()
        };
        ws.roles.insert("the-architect".into(), over);

        let mut editor = EditorState::new_edit("ws".into(), ws);
        editor.auth_selected_kind = Some(AuthKind::Github);
        let rows = auth_flat_rows(&editor, &AppConfig::default());

        assert!(
            rows.iter().any(|r| {
                matches!(
                    r,
                    AuthRow::RoleHeader { role, .. } if role == "the-architect"
                )
            }),
            "github role override must surface a RoleHeader; got {rows:?}"
        );
    }
}
