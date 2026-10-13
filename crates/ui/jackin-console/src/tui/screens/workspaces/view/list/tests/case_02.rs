// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn agents_subpanel_non_default_agent_name_starts_at_col_2() {
    let ws = ws_config_with_allowed(&["alpha", "beta"], Some("alpha"));
    let mut cfg = AppConfig::default();
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());
    cfg.roles
        .insert("beta".into(), jackin_config::RoleSource::default());

    let backend = TestBackend::new(40, 7);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_agents_subpanel(f, Rect::new(0, 0, 40, 7), Some(&ws), &cfg);
    })
    .unwrap();

    // Locate the first printable char on the beta row (y=4).
    let buf = term.backend().buffer();
    let inner = panel_inner(buf.area);
    let name_col = (inner.x..inner.right())
        .find(|x| {
            let sym = buf[(*x, 4)].symbol();
            !sym.is_empty() && sym != " "
        })
        .map(|x| (x - inner.x) as usize)
        .expect("beta row has content");
    assert_eq!(
        name_col, SUBPANEL_CONTENT_INDENT,
        "non-default role name should start at col {SUBPANEL_CONTENT_INDENT}, got {name_col}"
    );

    // And there must be no trailing star on the non-default row.
    let last_col = last_printable_indent(&term, 4).expect("beta row has content");
    // `beta` is 4 chars starting at col 2 ⇒ last printable at col 5.
    // A trailing star would push last_col to col 7 (space + star).
    assert_eq!(
        last_col,
        SUBPANEL_CONTENT_INDENT + "beta".len() - 1,
        "non-default role row must have no trailing suffix past the name",
    );
}

#[test]
fn agents_subpanel_default_agent_has_trailing_star() {
    let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    let mut cfg = AppConfig::default();
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());

    let backend = TestBackend::new(40, 6);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
    })
    .unwrap();

    let star_col = find_symbol_indent(&term, 3, "\u{2605}")
        .expect("default role row should contain a star glyph");
    let expected = SUBPANEL_CONTENT_INDENT + "alpha".len() + 1;
    assert_eq!(
        star_col, expected,
        "default role star should trail the name at col {expected}, got {star_col}"
    );
}

#[test]
fn agents_subpanel_default_agent_name_starts_at_col_2_regardless_of_star() {
    let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    let mut cfg = AppConfig::default();
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());

    let backend = TestBackend::new(40, 6);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_agents_subpanel(f, Rect::new(0, 0, 40, 6), Some(&ws), &cfg);
    })
    .unwrap();

    // Locate the first printable char on the alpha row (y=3).
    let buf = term.backend().buffer();
    let inner = panel_inner(buf.area);
    let name_col = (inner.x..inner.right())
        .find(|x| {
            let sym = buf[(*x, 3)].symbol();
            !sym.is_empty() && sym != " "
        })
        .map(|x| (x - inner.x) as usize)
        .expect("alpha row has content");
    assert_eq!(
        name_col, SUBPANEL_CONTENT_INDENT,
        "default role name should start at col {SUBPANEL_CONTENT_INDENT} even with the trailing star, got {name_col}"
    );
}

#[test]
fn general_subpanel_no_longer_shows_last_used() {
    let mut s = summary();
    s.last_role = Some("alpha".into());

    let backend = TestBackend::new(60, 4);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_general_subpanel(f, Rect::new(0, 0, 60, 4), &s.workdir);
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

#[test]
fn agents_subpanel_shows_default_at_top() {
    let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    let mut cfg = AppConfig::default();
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());

    let row = render_agents_row(Some(&ws), &cfg, 60, 6, 1);
    assert!(
        row.contains("Default"),
        "Roles row 1 must hold `Default`; got {row:?}"
    );
    assert!(
        row.contains("alpha"),
        "Roles row 1 must hold the default role name; got {row:?}"
    );
}

#[test]
fn agents_subpanel_default_none_renders_placeholder() {
    let ws = ws_config_with_allowed(&[], None);
    let cfg = AppConfig::default();

    let row = render_agents_row(Some(&ws), &cfg, 60, 6, 1);
    assert!(
        row.contains("Default") && row.contains("(none)"),
        "Default row should show `(none)` when no default role is set; got {row:?}"
    );
}

#[test]
fn agents_subpanel_no_longer_shows_last_used() {
    let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    ws.last_role = Some("beta".into());
    let mut cfg = AppConfig::default();
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());

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
            "Roles sub-panel must not render `Last used`; got row {y}: {row:?}"
        );
    }
}

#[test]
fn preview_agents_block_no_longer_lists_overrides() {
    let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    let mut overrides = jackin_config::WorkspaceRoleOverride::default();
    overrides.env.insert("API_KEY".into(), "literal".into());
    overrides
        .env
        .insert("LOG_LEVEL".into(), "op://Vault/Item/field".into());
    ws.roles.insert("alpha".into(), overrides);

    let mut cfg = AppConfig::default();
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());

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
    // Per-role override keys must NOT appear in the Roles block —
    // they live in the Environments block now.
    assert!(
        !joined.contains("API_KEY"),
        "override key API_KEY must NOT appear in the Roles block; got {joined}"
    );
    assert!(
        !joined.contains("LOG_LEVEL"),
        "override key LOG_LEVEL must NOT appear in the Roles block; got {joined}"
    );
    assert!(
        !joined.contains("[op]"),
        "`[op]` marker must NOT appear in the Roles block; got {joined}"
    );
    assert!(
        !joined.contains("(no overrides)"),
        "`(no overrides)` placeholder must NOT appear in the Roles block; got {joined}"
    );
    // Default + role name still render.
    assert!(
        joined.contains("Default") && joined.contains("alpha"),
        "Roles block must still show default + role name; got {joined}"
    );
}

#[test]
fn preview_agents_block_lists_all_global_agents_when_allowed_empty() {
    let ws = ws_config_with_allowed(&[], None);
    let mut cfg = AppConfig::default();
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());
    cfg.roles
        .insert("beta".into(), jackin_config::RoleSource::default());

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
        !joined.contains("any role"),
        "old `any role` placeholder should be gone; got {joined}"
    );
}

#[test]
fn preview_includes_environments_block_with_workspace_env_keys() {
    let mut ws = ws_config_with_allowed(&[], None);
    ws.env.insert("DB_URL".into(), "postgres://...".into());
    ws.env.insert("API_KEY".into(), "literal-secret".into());
    ws.env.insert(
        "ANTHROPIC_API_KEY".into(),
        "workspace-preview-sentinel".into(),
    );

    let joined = render_env_to_string(&ws, 60, 6);
    assert!(
        joined.contains("Environments"),
        "block title `Environments` must appear; got {joined}"
    );
    assert!(
        joined.contains("API_KEY"),
        "API_KEY env key must appear; got {joined}"
    );
    assert!(
        joined.contains("DB_URL"),
        "DB_URL env key must appear; got {joined}"
    );
    // Sub-section header from the previous layout must NOT appear in
    // the flat list.
    assert!(
        !joined.contains("All roles:"),
        "flat layout must not render the `All roles:` sub-header; got {joined}"
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
    assert!(
        !joined.contains("ANTHROPIC_API_KEY"),
        "account-owned key leaked: {joined}"
    );
    assert!(
        !joined.contains("workspace-preview-sentinel"),
        "account-owned value leaked: {joined}"
    );
}
