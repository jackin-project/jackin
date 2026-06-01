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

#[must_use]
pub fn mount_block_height(same_path_rows: impl IntoIterator<Item = bool>) -> u16 {
    let data_rows = mount_data_row_count(same_path_rows).unwrap_or(1);
    (data_rows + 2 + 1).min(12) as u16
}

#[must_use]
pub fn global_mount_rows_height(same_path_rows: impl IntoIterator<Item = bool>) -> u16 {
    let content_height = global_mounts_content_height(same_path_rows);
    (content_height + 2).min(12) as u16
}

#[must_use]
pub fn global_mounts_content_height(same_path_rows: impl IntoIterator<Item = bool>) -> usize {
    mount_data_row_count(same_path_rows).map_or(1, |data_rows| 1 + data_rows)
}

#[must_use]
pub fn env_block_height(workspace_keys: usize, role_keys: usize) -> u16 {
    let total_rows = workspace_keys + role_keys;
    (total_rows + 2).min(20) as u16
}

fn mount_data_row_count(same_path_rows: impl IntoIterator<Item = bool>) -> Option<usize> {
    let mut saw_row = false;
    let mut lines = 0;
    for same_path in same_path_rows {
        saw_row = true;
        lines += if same_path { 1 } else { 2 };
    }
    saw_row.then_some(lines)
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

    #[test]
    fn mount_heights_match_empty_and_host_source_rows() {
        assert_eq!(mount_block_height([]), 4);
        assert_eq!(mount_block_height([true, false]), 6);
        assert_eq!(global_mounts_content_height([]), 1);
        assert_eq!(global_mounts_content_height([true, false]), 4);
        assert_eq!(global_mount_rows_height([true, false]), 6);
    }
}
