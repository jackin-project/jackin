//! Workspaces screen view helpers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disclosure {
    None,
    Collapsed,
    Expanded,
}

impl Disclosure {
    #[must_use]
    pub const fn for_instances(has_instances: bool, expanded: bool) -> Self {
        if !has_instances {
            Self::None
        } else if expanded {
            Self::Expanded
        } else {
            Self::Collapsed
        }
    }

    #[must_use]
    pub const fn glyph(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Collapsed => Some("▶"),
            Self::Expanded => Some("▼"),
        }
    }
}

#[must_use]
pub fn workspace_delete_confirm_state(name: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(format!("Delete \"{name}\"?"))
}

#[must_use]
pub fn instance_purge_confirm_state(label: &str) -> jackin_tui::components::ConfirmState {
    jackin_tui::components::ConfirmState::new(format!(
        "Purge \"{label}\"?\nThis removes the role container, DinD sidecar, volume, network, AND local recovery state. Cannot be undone."
    ))
}

#[must_use]
pub fn create_prelude_mount_destination_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Destination", current)
}

#[must_use]
pub fn create_prelude_workspace_name_input_state<'a>(
    current: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
    jackin_tui::components::TextInputState::new("Name this workspace", current)
}

#[must_use]
pub fn create_prelude_mount_dst_choice_state(
    src: impl Into<String>,
) -> crate::tui::components::mount_dst_choice::MountDstChoiceState {
    crate::tui::components::mount_dst_choice::MountDstChoiceState::new(src)
}

#[must_use]
pub fn create_prelude_workdir_pick_state<M: crate::tui::components::workdir_pick::WorkdirMount>(
    mounts: &[M],
) -> crate::tui::components::workdir_pick::WorkdirPickState {
    crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(mounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_delete_confirm_state_names_workspace() {
        let state = workspace_delete_confirm_state("alpha");

        let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind()
        else {
            panic!("expected default confirm");
        };
        assert_eq!(prompt, "Delete \"alpha\"?");
    }

    #[test]
    fn instance_purge_confirm_state_names_label_and_scope() {
        let state = instance_purge_confirm_state("role/dev");

        let jackin_tui::components::ConfirmKind::Default { prompt } = state.kind()
        else {
            panic!("expected default confirm");
        };
        assert!(prompt.starts_with("Purge \"role/dev\"?"));
        assert!(prompt.contains("local recovery state"));
    }

    #[test]
    fn create_prelude_input_helpers_name_fields() {
        let dst = create_prelude_mount_destination_input_state("/workspace");
        let name = create_prelude_workspace_name_input_state("project");

        assert_eq!(dst.label, "Destination");
        assert_eq!(dst.value(), "/workspace");
        assert_eq!(name.label, "Name this workspace");
        assert_eq!(name.value(), "project");
    }

    #[test]
    fn create_prelude_mount_dst_choice_uses_source() {
        let state = create_prelude_mount_dst_choice_state("/host/project");

        assert_eq!(state.src, "/host/project");
    }
}
