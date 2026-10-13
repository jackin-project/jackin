// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const SUBPANEL_CONTENT_INDENT: usize = 2;

pub(super) fn config_with_long_workspace_name() -> AppConfig {
    let mut config = AppConfig::default();
    config.workspaces.insert(
        "chainargos-blockchain-nodes".into(),
        WorkspaceConfig::default(),
    );
    config
}

pub(super) fn config_with_sidebar_names_that_fit_wide_pane() -> AppConfig {
    let mut config = AppConfig::default();
    for name in [
        "chainargos",
        "chainargos-blockchain-nodes",
        "jackin",
        "parallax",
        "scentbird",
    ] {
        config
            .workspaces
            .insert(name.into(), WorkspaceConfig::default());
    }
    config
}

pub(super) fn config_with_many_workspaces() -> AppConfig {
    let mut config = AppConfig::default();
    for idx in 0..12 {
        config
            .workspaces
            .insert(format!("workspace-{idx:02}"), WorkspaceConfig::default());
    }
    config
}

pub(super) fn identity_test_instance_entry() -> jackin_core::InstanceIndexEntry {
    jackin_core::InstanceIndexEntry {
        instance_id: "instance-1".into(),
        container_base: "container-1".into(),
        workspace_name: Some("demo".into()),
        workspace_label: "demo".into(),
        workdir: "/workspace/demo".into(),
        role_key: "architect".into(),
        agent_runtime: "claude".into(),
        status: jackin_core::InstanceStatus::Active,
        updated_at: "2026-09-20T00:00:00Z".into(),
    }
}

pub(super) fn line_text(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>()
}

pub(super) fn mode_col_start(line: &ratatui::text::Line<'_>) -> usize {
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

pub(super) fn mount_row(
    destination: &str,
    mode: &'static str,
    isolation: &'static str,
    kind: &str,
) -> MountDisplayRow {
    MountDisplayRow {
        destination: destination.into(),
        host_source: None,
        mode,
        isolation,
        kind: kind.into(),
    }
}

pub(super) fn mount(path: &str) -> MountConfig {
    MountConfig {
        src: path.into(),
        dst: path.into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    }
}

pub(super) fn mount_block_height(mounts: &[MountConfig]) -> u16 {
    crate::tui::sidebar_layout::mount_block_height(
        mounts.iter().map(|mount| mount.src == mount.dst),
    )
}

pub(super) fn global_mounts_content_height(mounts: &[MountConfig]) -> usize {
    crate::tui::sidebar_layout::global_mounts_content_height(
        mounts.iter().map(|mount| mount.src == mount.dst),
    )
}

pub(super) fn render_agents_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    ws_config: Option<&WorkspaceConfig>,
    config: &AppConfig,
) {
    render_config_roles_subpanel(frame, area, ws_config, config, 0, 0, false);
}

pub(super) fn panel_inner(area: Rect) -> Rect {
    let theme = termrock::style::DesignSystem::default();
    termrock::widgets::Panel::new(&theme).inner(area)
}

pub(super) fn first_content_indent(terminal: &Terminal<TestBackend>) -> Option<usize> {
    let buf = terminal.backend().buffer();
    let inner = panel_inner(buf.area);
    for x in inner.x..inner.right() {
        let sym = buf[(x, inner.y)].symbol();
        if sym.is_empty() || sym == " " {
            continue;
        }
        return Some((x - inner.x) as usize);
    }
    None
}

pub(super) fn buffer_text(buf: &Buffer) -> String {
    let area = buf.area;
    let mut joined = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            joined.push_str(buf[(x, y)].symbol());
        }
        joined.push('\n');
    }
    joined
}

pub(super) fn summary() -> WorkspaceSummary {
    WorkspaceSummary {
        name: "demo".into(),
        workdir: "/tmp/demo".into(),
        mount_count: 1,
        readonly_mount_count: 0,
        allowed_role_count: 0,
        default_role: None,
        last_role: None,
    }
}

pub(super) fn ws_config_with_allowed(names: &[&str], default: Option<&str>) -> WorkspaceConfig {
    WorkspaceConfig {
        version: jackin_config::CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: "/tmp/demo".into(),
        mounts: vec![],
        allowed_roles: names.iter().map(|s| (*s).into()).collect(),
        default_role: default.map(String::from),
        default_agent: None,
        last_role: None,
        env: std::collections::BTreeMap::new(),
        roles: std::collections::BTreeMap::new(),
        keep_awake: jackin_config::KeepAwakeConfig::default(),
        accounts: Vec::new(),
        account_bindings: std::collections::BTreeMap::new(),
        github: None,
        git_pull_on_entry: false,
        runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        dirty_exit_policy: None,
        docker: None,
        default_launch: None,
    }
}

pub(super) fn find_symbol_indent(
    terminal: &Terminal<TestBackend>,
    y: u16,
    needle: &str,
) -> Option<usize> {
    let buf = terminal.backend().buffer();
    let inner = panel_inner(buf.area);
    for x in inner.x..inner.right() {
        if buf[(x, y)].symbol() == needle {
            return Some((x - inner.x) as usize);
        }
    }
    None
}

pub(super) fn last_printable_indent(terminal: &Terminal<TestBackend>, y: u16) -> Option<usize> {
    let buf = terminal.backend().buffer();
    let inner = panel_inner(buf.area);
    let mut last: Option<usize> = None;
    for x in inner.x..inner.right() {
        let sym = buf[(x, y)].symbol();
        if !sym.is_empty() && sym != " " {
            last = Some((x - inner.x) as usize);
        }
    }
    last
}

pub(super) fn render_agents_row(
    ws: Option<&WorkspaceConfig>,
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

pub(super) fn render_env_to_string(ws: &WorkspaceConfig, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_environments_subpanel(
            f,
            Rect::new(0, 0, width, height),
            workspace_env_rows(Some(ws)),
        );
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
    joined
}
