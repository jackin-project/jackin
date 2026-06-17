//! Tests for `sidebar_layout`.
use super::*;

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
