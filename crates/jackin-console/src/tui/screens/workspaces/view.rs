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
}
