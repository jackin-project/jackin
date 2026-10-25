use std::borrow::Borrow;
use std::fmt;

use crate::workspace_name::WorkspaceName;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceLabel(String);

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceLabelError {
    #[error("workspace label cannot be empty")]
    Empty,
}

impl WorkspaceLabel {
    pub fn parse(input: &str) -> Result<Self, WorkspaceLabelError> {
        if input.is_empty() {
            return Err(WorkspaceLabelError::Empty);
        }
        Ok(Self(input.to_owned()))
    }

    #[must_use]
    pub fn from_name(name: &WorkspaceName) -> Self {
        Self(name.as_str().to_owned())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for WorkspaceLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Borrow<str> for WorkspaceLabel {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for WorkspaceLabel {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for WorkspaceLabel {
    type Error = WorkspaceLabelError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<WorkspaceName> for WorkspaceLabel {
    fn from(name: WorkspaceName) -> Self {
        Self(name.into_inner())
    }
}
