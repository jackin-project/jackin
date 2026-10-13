// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn preview_environments_block_lists_envs_alphabetically_with_agent_on_right() {
    let mut ws = ws_config_with_allowed(&["beta", "alpha"], Some("alpha"));
    ws.env.insert("API_KEY".into(), "literal".into());
    ws.env.insert("DB_URL".into(), "postgres://...".into());

    let mut alpha_overrides = jackin_config::WorkspaceRoleOverride::default();
    alpha_overrides
        .env
        .insert("LOG_LEVEL".into(), "debug".into());
    ws.roles.insert("alpha".into(), alpha_overrides);

    let mut beta_overrides = jackin_config::WorkspaceRoleOverride::default();
    beta_overrides.env.insert("DEBUG".into(), "1".into());
    ws.roles.insert("beta".into(), beta_overrides);

    let joined = render_env_to_string(&ws, 60, 14);
    // No sub-headers in the flat layout.
    assert!(
        !joined.contains("All roles:"),
        "flat layout must not render `All roles:`; got {joined}"
    );
    assert!(
        !joined.contains("alpha:"),
        "flat layout must not render `<role>:` sub-headers; got {joined}"
    );
    assert!(
        !joined.contains("beta:"),
        "flat layout must not render `<role>:` sub-headers; got {joined}"
    );

    // Find each name's y-row to pin alphabetical ordering across scopes.
    let mut api_y: Option<u16> = None;
    let mut db_y: Option<u16> = None;
    let mut debug_y: Option<u16> = None;
    let mut log_y: Option<u16> = None;
    for (y, row) in joined.lines().enumerate() {
        if api_y.is_none() && row.contains("API_KEY") {
            api_y = Some(y as u16);
        }
        if db_y.is_none() && row.contains("DB_URL") {
            db_y = Some(y as u16);
        }
        if debug_y.is_none() && row.contains("DEBUG") {
            debug_y = Some(y as u16);
        }
        if log_y.is_none() && row.contains("LOG_LEVEL") {
            log_y = Some(y as u16);
        }
    }
    let api = api_y.expect("API_KEY row must appear");
    let db = db_y.expect("DB_URL row must appear");
    let debug = debug_y.expect("DEBUG row must appear");
    let log = log_y.expect("LOG_LEVEL row must appear");
    assert!(
        api < db && db < debug && debug < log,
        "rows must be alphabetical: API_KEY < DB_URL < DEBUG < LOG_LEVEL; \
         got y=({api},{db},{debug},{log})"
    );

    // Role labels live on the right edge of their row.
    for row in joined.lines() {
        if row.contains("DEBUG") {
            assert!(
                row.contains("beta"),
                "DEBUG row must show `beta` on the right; got {row}"
            );
        }
        if row.contains("LOG_LEVEL") {
            assert!(
                row.contains("alpha"),
                "LOG_LEVEL row must show `alpha` on the right; got {row}"
            );
        }
    }
}

#[test]
fn preview_environments_block_omits_agents_without_overrides() {
    let mut ws = ws_config_with_allowed(&["alpha", "beta"], Some("alpha"));
    ws.env.insert("API_KEY".into(), "literal".into());
    // Only alpha has overrides; beta is in the allowed list but
    // has no overrides.
    let mut alpha_overrides = jackin_config::WorkspaceRoleOverride::default();
    alpha_overrides
        .env
        .insert("LOG_LEVEL".into(), "debug".into());
    ws.roles.insert("alpha".into(), alpha_overrides);

    let joined = render_env_to_string(&ws, 60, 10);
    assert!(
        joined.contains("alpha"),
        "alpha has overrides — its name must appear on its row; got {joined}"
    );
    assert!(
        !joined.contains("beta"),
        "beta has no overrides — its name must NOT appear in the Environments block; got {joined}"
    );
}

#[test]
fn preview_environments_flat_row_workspace_level_has_no_agent_label() {
    let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    ws.env.insert("API_KEY".into(), "literal".into());

    let joined = render_env_to_string(&ws, 60, 4);
    // The row containing API_KEY must not also contain "alpha".
    let api_row = joined
        .lines()
        .find(|r| r.contains("API_KEY"))
        .expect("API_KEY row must appear");
    assert!(
        !api_row.contains("alpha"),
        "workspace-level row must not show an role label; got `{api_row}`"
    );
}

#[test]
fn preview_environments_flat_row_per_agent_has_agent_label_on_right() {
    let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    let mut alpha_overrides = jackin_config::WorkspaceRoleOverride::default();
    alpha_overrides
        .env
        .insert("LOG_LEVEL".into(), "debug".into());
    ws.roles.insert("alpha".into(), alpha_overrides);

    let joined = render_env_to_string(&ws, 60, 4);
    let log_row = joined
        .lines()
        .find(|r| r.contains("LOG_LEVEL"))
        .expect("LOG_LEVEL row must appear");
    assert!(
        log_row.contains("alpha"),
        "per-role row must show the role name; got `{log_row}`"
    );
    // Role name sits to the right of the key name on the same row.
    let key_pos = log_row.find("LOG_LEVEL").unwrap();
    let agent_pos = log_row.find("alpha").unwrap();
    assert!(
        agent_pos > key_pos,
        "role label must come AFTER the key name on the row; got key@{key_pos}, role@{agent_pos}"
    );
}

#[test]
fn preview_environments_agent_label_has_one_cell_right_padding() {
    let mut ws = ws_config_with_allowed(&["agent-brown"], Some("agent-brown"));
    let mut brown = jackin_config::WorkspaceRoleOverride::default();
    brown.env.insert("TEST5".into(), "v".into());
    ws.roles.insert("agent-brown".into(), brown);

    let width: u16 = 60;
    let backend = TestBackend::new(width, 4);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_environments_subpanel(f, Rect::new(0, 0, width, 4), workspace_env_rows(Some(&ws)));
    })
    .unwrap();
    let buf = term.backend().buffer();

    // Find the row containing TEST5; role label `agent-brown`
    // must end one cell before the right border so the cell at
    // x = width - 2 (i.e. the one just inside the right border
    // at x = width - 1) is a space, and the label's last char
    // sits at x = width - 3.
    let mut found_row: Option<u16> = None;
    for y in 0..buf.area.height {
        let row: String = (0..width).map(|x| buf[(x, y)].symbol()).collect();
        if row.contains("TEST5") {
            found_row = Some(y);
            break;
        }
    }
    let y = found_row.expect("TEST5 row must render");

    // Right border is at x = width - 1 (the `│` glyph).
    // The cell immediately inside (x = width - 2) must be blank
    // — that's the 1-cell padding the operator asked for.
    let cell_inside_border = buf[(width - 2, y)].symbol();
    assert_eq!(
        cell_inside_border,
        " ",
        "cell at x={} (one inside right border) must be a space — \
         role label should have 1-cell right padding; got {:?}",
        width - 2,
        cell_inside_border
    );

    // And the role label's last char (`n` of `agent-brown`)
    // must sit at x = width - 3 — the cell just before the pad.
    let label_last = buf[(width - 3, y)].symbol();
    assert_eq!(
        label_last,
        "n",
        "last char of `agent-brown` must sit at x={} (one cell \
         before the right border); got {:?}",
        width - 3,
        label_last
    );
}

#[test]
fn preview_environments_same_key_in_workspace_and_agent_renders_two_rows() {
    let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    ws.env.insert("API_KEY".into(), "workspace-value".into());
    let mut alpha_overrides = jackin_config::WorkspaceRoleOverride::default();
    alpha_overrides
        .env
        .insert("API_KEY".into(), "role-value".into());
    ws.roles.insert("alpha".into(), alpha_overrides);

    let joined = render_env_to_string(&ws, 60, 6);
    let api_rows: Vec<&str> = joined.lines().filter(|r| r.contains("API_KEY")).collect();
    assert_eq!(
        api_rows.len(),
        2,
        "API_KEY must appear in TWO rows (workspace + alpha); got rows={api_rows:?}"
    );
    // Workspace row first (no role label), role row second.
    assert!(
        !api_rows[0].contains("alpha"),
        "first API_KEY row must be workspace-level (no role label); got `{}`",
        api_rows[0]
    );
    assert!(
        api_rows[1].contains("alpha"),
        "second API_KEY row must be the role override (alpha label); got `{}`",
        api_rows[1]
    );
}

#[test]
fn preview_environments_sorts_alphabetically_across_scopes() {
    let mut ws = ws_config_with_allowed(&["agent-smith", "agent-brown"], Some("agent-smith"));
    ws.env.insert("DB_URL".into(), "postgres://...".into());
    ws.env.insert("API_KEY".into(), "literal".into());

    let mut smith = jackin_config::WorkspaceRoleOverride::default();
    smith.env.insert("DEBUG".into(), "1".into());
    ws.roles.insert("agent-smith".into(), smith);

    let mut brown = jackin_config::WorkspaceRoleOverride::default();
    brown.env.insert("LOG_LEVEL".into(), "debug".into());
    ws.roles.insert("agent-brown".into(), brown);

    let joined = render_env_to_string(&ws, 60, 8);
    // Capture the y-row of each env-key name and assert ordering.
    let mut order: Vec<(&str, usize)> = Vec::new();
    for (y, row) in joined.lines().enumerate() {
        for key in ["API_KEY", "DB_URL", "DEBUG", "LOG_LEVEL"] {
            if row.contains(key) && !order.iter().any(|(k, _)| *k == key) {
                order.push((key, y));
            }
        }
    }
    let names: Vec<&str> = order.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        names,
        vec!["API_KEY", "DB_URL", "DEBUG", "LOG_LEVEL"],
        "rows must be sorted alphabetically across workspace and role scopes; got {order:?}"
    );
}

#[test]
fn preview_environments_marks_op_references_with_op_marker() {
    let mut ws = ws_config_with_allowed(&[], None);
    ws.env.insert(
        "STRIPE_KEY".into(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://abc-vault/abc-item/field".into(),
            path: "Vault/Item/field".into(),
            account: None,
            on_demand: false,
        }),
    );

    let backend = TestBackend::new(60, 4);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_environments_subpanel(f, Rect::new(0, 0, 60, 4), workspace_env_rows(Some(&ws)));
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

#[test]
fn preview_omits_environments_block_when_workspace_has_no_env_vars() {
    // Empty workspace env, no role overrides.
    let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));

    let mut cfg = AppConfig::default();
    cfg.workspaces.insert("demo".into(), ws);
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());

    let summary = WorkspaceSummary {
        name: "demo".into(),
        workdir: "/workspace/demo".into(),
        mount_count: 0,
        readonly_mount_count: 0,
        allowed_role_count: 1,
        default_role: Some("alpha".into()),
        last_role: None,
    };

    let backend = TestBackend::new(60, 24);
    let mut term = Terminal::new(backend).unwrap();
    let state = ManagerState::from_config(&cfg, std::path::Path::new("/tmp"));
    term.draw(|f| {
        render_details_pane(f, Rect::new(0, 0, 60, 24), &summary, &cfg, &state);
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
        !joined.contains("Environments"),
        "Environments block must NOT render when the workspace has no env vars; got {joined}"
    );
    assert!(
        !joined.contains("(no environment variables)"),
        "the placeholder line must NOT appear (block is omitted entirely); got {joined}"
    );
}
