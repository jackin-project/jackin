//! Operator-facing workspace label (path/display scope).

use std::borrow::Borrow;
use std::fmt;

use crate::workspace_name::WorkspaceName;

/// Operator-facing / path-scoped workspace label.
///
/// Used for isolation records, instance index matching, materialization, and
/// display. Distinct from [`WorkspaceName`] (config-file stem / identity key):
/// ad-hoc workspaces may use a workdir path as the label, which can contain
/// path separators and would be rejected by [`WorkspaceName::parse`].
///
/// Construct via [`WorkspaceLabel::parse`], [`WorkspaceLabel::from_name`], or
/// [`TryFrom`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceLabel(String);

/// Why a workspace label string is not legal.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceLabelError {
    /// Label was empty.
    #[error("workspace label cannot be empty")]
    Empty,
}

impl WorkspaceLabel {
    /// Parse a non-empty workspace label (path or display string allowed).
    pub fn parse(input: &str) -> Result<Self, WorkspaceLabelError> {
        if input.is_empty() {
            return Err(WorkspaceLabelError::Empty);
        }
        Ok(Self(input.to_owned()))
    }

    /// Build a label from a validated config-stem name (common saved-workspace path).
    #[must_use]
    pub fn from_name(name: &WorkspaceName) -> Self {
        Self(name.as_str().to_owned())
    }

    /// Borrow the label as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner `String`.
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

#[cfg(test)]
mod tests;
