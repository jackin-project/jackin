// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn preview_shows_unscoped_global_mounts_without_role_ambiguity_text() {
    let ws = ws_config_with_allowed(&["alpha", "beta"], None);
    let mut cfg = AppConfig::default();
    cfg.workspaces.insert("demo".into(), ws);
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());
    cfg.roles
        .insert("beta".into(), jackin_config::RoleSource::default());
    cfg.add_mount(
        "cargo",
        MountConfig {
            src: "/tmp/cargo".into(),
            dst: "/home/agent/.cargo".into(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        },
        None,
    );
    cfg.add_mount(
        "beta-only",
        MountConfig {
            src: "/tmp/beta".into(),
            dst: "/beta".into(),
            readonly: true,
            isolation: jackin_config::MountIsolation::Shared,
        },
        Some("beta"),
    );

    let backend = TestBackend::new(72, 24);
    let mut term = Terminal::new(backend).unwrap();
    let state = ManagerState::from_config(&cfg, std::path::Path::new("/tmp"));
    term.draw(|f| {
        render_details_pane(f, Rect::new(0, 0, 72, 24), &summary(), &cfg, &state);
    })
    .unwrap();

    let joined = buffer_text(term.backend().buffer());
    assert!(joined.contains("Global mounts"), "{joined}");
    assert!(joined.contains(".cargo"), "{joined}");
    assert!(!joined.contains("selected role affects"), "{joined}");
    assert!(!joined.contains("/beta"), "{joined}");
    assert!(joined.contains("+1 role mounts"), "{joined}");
}

#[test]
fn preview_includes_environments_block_when_only_workspace_env_set() {
    let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    ws.env.insert("API_KEY".into(), "literal".into());

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
        joined.contains("Environments"),
        "Environments block header must appear when the workspace env is non-empty; got {joined}"
    );
    assert!(
        joined.contains("API_KEY"),
        "the workspace env key must render; got {joined}"
    );
}

#[test]
fn preview_shows_compact_running_badge_for_active_instances() {
    let ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    let mut cfg = AppConfig::default();
    cfg.workspaces.insert("demo".into(), ws);
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());

    let mut state = ManagerState::from_config(&cfg, std::path::Path::new("/tmp"));
    state.instances = vec![
        jackin_core::InstanceIndexEntry {
            instance_id: "k7p9m2xq".into(),
            container_base: "jackin-demo-alpha-k7p9m2xq".into(),
            workspace_name: Some("demo".into()),
            workspace_label: "demo".into(),
            workdir: "/workspace/demo".into(),
            role_key: "alpha".into(),
            agent_runtime: "claude".into(),
            status: jackin_core::InstanceStatus::Active,
            updated_at: "2026-05-11T00:00:00Z".into(),
        },
        jackin_core::InstanceIndexEntry {
            instance_id: "done0001".into(),
            container_base: "jackin-demo-alpha-done0001".into(),
            workspace_name: Some("demo".into()),
            workspace_label: "demo".into(),
            workdir: "/workspace/demo".into(),
            role_key: "alpha".into(),
            agent_runtime: "claude".into(),
            status: jackin_core::InstanceStatus::CleanExited,
            updated_at: "2026-05-11T00:00:00Z".into(),
        },
    ];

    let summary = WorkspaceSummary {
        name: "demo".into(),
        workdir: "/workspace/demo".into(),
        mount_count: 0,
        readonly_mount_count: 0,
        allowed_role_count: 1,
        default_role: Some("alpha".into()),
        last_role: None,
    };

    let backend = TestBackend::new(72, 24);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        render_details_pane(f, Rect::new(0, 0, 72, 24), &summary, &cfg, &state);
    })
    .unwrap();

    let joined = buffer_text(term.backend().buffer());
    // Compact badge shows the "Running" block title and instance count.
    assert!(joined.contains("Running"), "{joined}");
    assert!(joined.contains("1 instance running"), "{joined}");
    // CleanExited instances are not shown in the compact summary.
    assert!(
        !joined.contains("done0001"),
        "cleanly exited instances must not appear: {joined}"
    );
}

#[test]
fn preview_includes_environments_block_when_only_per_agent_overrides_set() {
    let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    let mut alpha_overrides = jackin_config::WorkspaceRoleOverride::default();
    alpha_overrides
        .env
        .insert("LOG_LEVEL".into(), "debug".into());
    ws.roles.insert("alpha".into(), alpha_overrides);

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
        joined.contains("Environments"),
        "Environments block header must appear when only per-role overrides exist; got {joined}"
    );
    assert!(
        joined.contains("LOG_LEVEL"),
        "the per-role override key must render; got {joined}"
    );
}

#[test]
fn preview_block_order_is_general_mounts_environments_agents() {
    // Build a workspace with a mount, an env var, and an role so
    // every block has visible content.
    let mut ws = ws_config_with_allowed(&["alpha"], Some("alpha"));
    ws.workdir = "/workspace/demo".into();
    ws.mounts.push(MountConfig {
        src: "/tmp/demo".into(),
        dst: "/workspace/demo".into(),
        readonly: false,
        isolation: jackin_config::MountIsolation::Shared,
    });
    ws.env.insert("API_KEY".into(), "literal".into());

    let mut cfg = AppConfig::default();
    cfg.workspaces.insert("demo".into(), ws);
    cfg.roles
        .insert("alpha".into(), jackin_config::RoleSource::default());

    let summary = WorkspaceSummary {
        name: "demo".into(),
        workdir: "/workspace/demo".into(),
        mount_count: 1,
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
        if agents_y.is_none() && row.contains(" Roles ") {
            agents_y = Some(y);
        }
    }

    let g = general_y.expect("General block title must appear");
    let m = mounts_y.expect("Mounts block title must appear");
    let e = envs_y.expect("Environments block title must appear");
    let a = agents_y.expect("Roles block title must appear");
    assert!(
        g < m && m < e && e < a,
        "block order must be General < Mounts < Environments < Roles; got y=({g},{m},{e},{a})"
    );
}
