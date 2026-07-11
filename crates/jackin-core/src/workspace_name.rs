//! Validated workspace name (config-file stem).

use std::borrow::Borrow;
use std::fmt;

/// Workspace name used as the config-file stem and map key entrance.
///
/// Construct only via [`WorkspaceName::parse`] / [`TryFrom`]. The rules match
/// the former `validate_workspace_file_stem` invariant (empty, reserved dots,
/// path separators, and Windows device names).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkspaceName(String);

/// Why a workspace name string is not legal as a config-file stem.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceNameError {
    #[error("workspace name cannot be empty")]
    Empty,
    #[error("workspace name {0:?} is reserved")]
    Reserved(String),
    #[error("workspace name {0:?} cannot contain path separators")]
    PathSeparator(String),
    #[cfg(windows)]
    #[error("workspace name {0:?} is reserved on Windows")]
    WindowsReserved(String),
    #[cfg(windows)]
    #[error("workspace name {0:?} cannot end with a dot or space on Windows")]
    WindowsTrailing(String),
}

impl WorkspaceName {
    /// Parse and validate a workspace name (config-file stem rules).
    pub fn parse(input: &str) -> Result<Self, WorkspaceNameError> {
        if input.is_empty() {
            return Err(WorkspaceNameError::Empty);
        }
        if input == "." || input == ".." {
            return Err(WorkspaceNameError::Reserved(input.to_owned()));
        }
        if input.contains('/') || input.contains('\\') {
            return Err(WorkspaceNameError::PathSeparator(input.to_owned()));
        }
        #[cfg(windows)]
        {
            const RESERVED: &[&str] = &[
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
            ];
            if RESERVED
                .iter()
                .any(|reserved| input.eq_ignore_ascii_case(reserved))
            {
                return Err(WorkspaceNameError::WindowsReserved(input.to_owned()));
            }
            if input.ends_with('.') || input.ends_with(' ') {
                return Err(WorkspaceNameError::WindowsTrailing(input.to_owned()));
            }
        }
        Ok(Self(input.to_owned()))
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

impl fmt::Display for WorkspaceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Borrow<str> for WorkspaceName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for WorkspaceName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for WorkspaceName {
    type Error = WorkspaceNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

#[cfg(test)]
#[path = "workspace_name/tests.rs"]
mod tests;
