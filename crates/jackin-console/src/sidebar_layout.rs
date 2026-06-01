//! Pure sidebar rectangle allocation for the workspace list preview pane.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Fixed height of the compact running-instances badge (borders + 1 text line).
pub const COMPACT_INSTANCES_HEIGHT: u16 = 3;

/// Root-derived heights and visibility flags for sidebar layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarLayoutMetrics {
    pub instance_count: usize,
    pub workspace_mount_height: u16,
    pub global_mount_height: Option<u16>,
    pub role_global_mount_height: Option<u16>,
    pub env_height: Option<u16>,
    pub show_roles: bool,
    pub agent_count: usize,
}

/// Rect for each rendered block. `None` panels are skipped in both render
/// and hit-test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarLayout {
    pub instances: Option<Rect>,
    pub general: Rect,
    pub mounts: Rect,
    pub global: Option<Rect>,
    pub role_global: Option<Rect>,
    pub env: Option<Rect>,
    pub roles: Option<Rect>,
}

#[must_use]
pub fn compute_sidebar_layout(area: Rect, metrics: SidebarLayoutMetrics) -> SidebarLayout {
    let mut constraints = Vec::new();
    if metrics.instance_count > 0 {
        constraints.push(Constraint::Length(COMPACT_INSTANCES_HEIGHT));
    }
    constraints.push(Constraint::Length(3));
    constraints.push(Constraint::Length(metrics.workspace_mount_height));
    if let Some(height) = metrics.global_mount_height {
        constraints.push(Constraint::Length(height));
    }
    if let Some(height) = metrics.role_global_mount_height {
        constraints.push(Constraint::Length(height));
    }
    if let Some(height) = metrics.env_height {
        constraints.push(Constraint::Length(height));
    }
    if metrics.show_roles {
        constraints.push(Constraint::Length(agents_block_height(metrics.agent_count)));
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let mut iter = rows.iter().copied();

    SidebarLayout {
        instances: (metrics.instance_count > 0).then(|| iter.next().expect("instances slot")),
        general: iter.next().expect("general slot"),
        mounts: iter.next().expect("mounts slot"),
        global: metrics
            .global_mount_height
            .is_some()
            .then(|| iter.next().expect("global slot")),
        role_global: metrics
            .role_global_mount_height
            .is_some()
            .then(|| iter.next().expect("role-global slot")),
        env: metrics
            .env_height
            .is_some()
            .then(|| iter.next().expect("env slot")),
        roles: metrics.show_roles.then(|| iter.next().expect("roles slot")),
    }
}

#[must_use]
pub fn agents_block_height(agent_count: usize) -> u16 {
    let agent_rows = agent_count.max(1);
    (2 + 1 + 1 + agent_rows).min(14) as u16
}

#[cfg(test)]
mod tests {
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
}
