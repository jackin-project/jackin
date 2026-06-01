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
