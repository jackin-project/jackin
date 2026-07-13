// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `sidebar_layout`.
use super::*;

use jackin_config::{
    AppConfig, EnvValue, GlobalMountRow, MountConfig, MountIsolation, RoleSource, WorkspaceConfig,
};

use crate::tui::screens::workspaces::model::ManagerListRow;

#[test]
fn selected_sidebar_target_routes_only_workspace_rows() {
    assert_eq!(
        selected_sidebar_target(ManagerListRow::CurrentDirectory),
        Some(SelectedSidebarTarget::CurrentDirectory)
    );
    assert_eq!(
        selected_sidebar_target(ManagerListRow::SavedWorkspace(2)),
        Some(SelectedSidebarTarget::SavedWorkspace(2))
    );
    assert_eq!(
        selected_sidebar_target(ManagerListRow::CurrentDirectoryInstance(0)),
        None
    );
    assert_eq!(
        selected_sidebar_target(ManagerListRow::WorkspaceInstance(0, 1)),
        None
    );
    assert_eq!(selected_sidebar_target(ManagerListRow::NewWorkspace), None);
}

#[test]
fn global_mount_rows_selection_routes_current_dir_and_existing_workspace() {
    assert_eq!(
        global_mount_rows_selection::<String>(ManagerListRow::CurrentDirectory, |_| false, None),
        GlobalMountRowsSelection::CurrentDirectory
    );
    assert_eq!(
        global_mount_rows_selection::<String>(
            ManagerListRow::CurrentDirectoryInstance(0),
            |_| false,
            Some("smith".into())
        ),
        GlobalMountRowsSelection::CurrentDirectory
    );
    assert_eq!(
        global_mount_rows_selection(
            ManagerListRow::SavedWorkspace(2),
            |idx| idx == 2,
            Some("smith".to_owned())
        ),
        GlobalMountRowsSelection::SavedWorkspace {
            picker_role: Some("smith".to_owned())
        }
    );
    assert_eq!(
        global_mount_rows_selection::<String>(ManagerListRow::SavedWorkspace(9), |_| false, None),
        GlobalMountRowsSelection::None
    );
    assert_eq!(
        global_mount_rows_selection::<String>(
            ManagerListRow::WorkspaceInstance(0, 1),
            |_| true,
            None
        ),
        GlobalMountRowsSelection::None
    );
}

#[test]
fn inline_picker_role_prefers_role_picker_selection() {
    assert_eq!(
        inline_picker_role(Some("role-picker"), Some("agent-picker")),
        Some("role-picker")
    );
    assert_eq!(
        inline_picker_role::<&str>(None, Some("agent-picker")),
        Some("agent-picker")
    );
    assert_eq!(inline_picker_role::<&str>(None, None), None);
}

#[test]
fn inline_picker_active_accepts_either_picker() {
    assert!(inline_picker_active(true, false));
    assert!(inline_picker_active(false, true));
    assert!(inline_picker_active(true, true));
    assert!(!inline_picker_active(false, false));
}

#[test]
fn omits_optional_blocks_without_consuming_slots() {
    let layout = compute_sidebar_layout(
        Rect::new(0, 0, 40, 30),
        SidebarLayoutMetrics {
            instance_count: 0,
            workspace_mount_height: 5,
            global_mount_height: None,
            role_global_mount_height: Some(4),
            env_height: None,
            show_roles: true,
            agent_count: 3,
        },
    );

    assert!(layout.instances.is_none());
    assert_eq!(layout.general.y, 0);
    assert_eq!(layout.mounts.y, 3);
    assert!(layout.global.is_none());
    assert_eq!(layout.role_global.expect("role global").y, 8);
    assert!(layout.env.is_none());
    assert_eq!(layout.roles.expect("roles").y, 12);
}

#[test]
fn mount_heights_match_empty_and_host_source_rows() {
    assert_eq!(mount_block_height([]), 4);
    assert_eq!(mount_block_height([true, false]), 6);
    assert_eq!(global_mounts_content_height([]), 1);
    assert_eq!(global_mounts_content_height([true, false]), 4);
    assert_eq!(global_mount_rows_height([true, false]), 6);
}

#[test]
fn agent_and_env_metrics_are_data_only() {
    assert!(!workspace_has_any_env(0, 0));
    assert!(workspace_has_any_env(1, 0));
    assert!(workspace_has_any_env(0, 1));
    assert_eq!(agents_block_agent_count(true, 5, 2), 5);
    assert_eq!(agents_block_agent_count(false, 5, 2), 2);
    assert_eq!(agents_block_content_width(["a", "long-role"]), 13);
}

#[test]
fn config_sidebar_layout_derives_optional_sections() {
    let workspace_mount = MountConfig {
        src: "/repo".into(),
        dst: "/repo".into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    };
    let global_mount = MountConfig {
        src: "/cache".into(),
        dst: "/jackin/cache".into(),
        readonly: true,
        isolation: MountIsolation::Shared,
    };
    let mut ws = WorkspaceConfig::default();
    ws.env
        .insert("LOCAL_ENV".into(), EnvValue::Plain("value".into()));
    let inputs = ConfigSidebarInputs {
        workdir: "/repo",
        mounts: std::slice::from_ref(&workspace_mount),
        mount_info_cache: MountInfoCache::default(),
        ws_config: Some(&ws),
        global_rows: vec![GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: global_mount,
        }],
        picker_role_label: String::new(),
        instance_count: 2,
        instance_expanded: true,
        inline_picker_active: false,
        show_envs: true,
        agent_count: 0,
    };

    let layout = compute_config_sidebar_layout(Rect::new(0, 0, 80, 40), &inputs);
    let areas = compute_config_sidebar_scroll_areas(
        Rect::new(0, 0, 80, 40),
        &inputs,
        &AppConfig::default(),
    );

    assert_eq!(layout.instances.expect("instances").height, 3);
    assert!(layout.global.is_some());
    assert!(layout.env.is_some());
    assert!(layout.roles.is_some());
    assert_eq!(areas.workspace.content_height, 2);
    assert_eq!(areas.global.content_height, 3);
    assert_eq!(areas.roles.expect("roles").content_height, 2);
}

#[test]
fn config_sidebar_selection_builder_suppresses_agents_for_inline_picker() {
    let mut config = AppConfig::default();
    config.roles.insert("smith".into(), RoleSource::default());
    let ws = WorkspaceConfig::default();

    let inputs = config_sidebar_inputs_for_selection(
        ConfigSidebarSelectionInputs {
            workdir: "/repo",
            mounts: &[],
            mount_info_cache: MountInfoCache::default(),
            ws_config: Some(&ws),
            global_rows: Vec::new(),
            picker_role_label: "smith".into(),
            instance_count: 1,
            instance_expanded: true,
            inline_picker_active: true,
            show_envs: false,
        },
        &config,
    );

    assert_eq!(inputs.agent_count, 0);
    assert_eq!(inputs.picker_role_label, "smith");
    assert_eq!(inputs.instance_count, 1);
    assert!(inputs.inline_picker_active);
}

#[test]
fn sidebar_active_instance_count_matches_active_workspace_facts() {
    let instances = [
        SidebarInstanceFacts {
            workspace_name: Some("saved"),
            workspace_label: "saved",
            workdir: "/repo",
            active: true,
        },
        SidebarInstanceFacts {
            workspace_name: Some("saved"),
            workspace_label: "saved",
            workdir: "/repo",
            active: false,
        },
        SidebarInstanceFacts {
            workspace_name: None,
            workspace_label: "/repo",
            workdir: "/repo",
            active: true,
        },
        SidebarInstanceFacts {
            workspace_name: Some("other"),
            workspace_label: "other",
            workdir: "/other",
            active: true,
        },
    ];

    assert_eq!(
        sidebar_active_instance_count(
            instances,
            SidebarInstanceQuery {
                workspace_name: Some("saved"),
                workspace_label: "saved",
                workdir: "/repo",
            },
        ),
        1
    );
    assert_eq!(
        sidebar_active_instance_count(
            instances,
            SidebarInstanceQuery {
                workspace_name: None,
                workspace_label: "/repo",
                workdir: "/repo",
            },
        ),
        1
    );
}

#[test]
fn scroll_area_detects_horizontal_and_vertical_overflow() {
    let area = Rect::new(0, 0, 10, 5);
    assert!(!scroll_area_scrollable(SidebarScrollArea {
        area,
        content_width: 8,
        content_height: 3,
    }));
    assert!(scroll_area_scrollable(SidebarScrollArea {
        area,
        content_width: 30,
        content_height: 3,
    }));
    assert!(scroll_area_scrollable(SidebarScrollArea {
        area,
        content_width: 8,
        content_height: 30,
    }));
}

#[test]
fn clamps_both_sidebar_scroll_axes() {
    let area = SidebarScrollArea {
        area: Rect::new(0, 0, 10, 5),
        content_width: 30,
        content_height: 20,
    };
    let mut scroll_x = u16::MAX;
    let mut scroll_y = u16::MAX;

    clamp_scroll_area(area, &mut scroll_x, &mut scroll_y);

    assert_eq!(scroll_x, 22);
    assert_eq!(scroll_y, 17);
}

#[test]
fn focused_scrollability_requires_area_and_overflow() {
    let area = Rect::new(0, 0, 10, 5);
    let scrollable = SidebarScrollArea {
        area,
        content_width: 30,
        content_height: 3,
    };
    let empty = SidebarScrollArea {
        area: Rect::new(0, 0, 0, 0),
        content_width: 30,
        content_height: 3,
    };
    let areas = SidebarScrollAreas {
        workspace: scrollable,
        global: empty,
        role_global: None,
        roles: None,
    };

    assert!(focused_scroll_area_still_scrollable(
        SidebarScrollFocus::Workspace,
        Some(&areas)
    ));
    assert_eq!(
        focused_scroll_area_axes(SidebarScrollFocus::Workspace, Some(&areas)),
        ScrollAxes {
            horizontal: true,
            vertical: false
        }
    );
    assert_eq!(
        focused_scroll_area_axes(SidebarScrollFocus::Global, Some(&areas)),
        ScrollAxes::none()
    );
    assert!(!focused_scroll_area_still_scrollable(
        SidebarScrollFocus::Global,
        Some(&areas)
    ));
    assert!(!focused_scroll_area_still_scrollable(
        SidebarScrollFocus::RoleGlobal,
        Some(&areas)
    ));
    assert!(!focused_scroll_area_still_scrollable(
        SidebarScrollFocus::Roles,
        None
    ));
    assert!(focused_mount_scroll_area_still_scrollable(
        crate::tui::focus::MountScrollFocus::Workspace,
        Some(&areas)
    ));
    assert!(!focused_mount_scroll_area_still_scrollable(
        crate::tui::focus::MountScrollFocus::Global,
        Some(&areas)
    ));
}
