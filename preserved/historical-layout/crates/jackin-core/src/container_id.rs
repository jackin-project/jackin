// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Docker / role container name newtype.

use std::fmt;

/// Validated container name (role instance base or Docker name).
///
/// Schema-preserving transparent string. Rejects empty names and names with
/// whitespace or path separators so host APIs cannot smuggle path fragments.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct ContainerId(String);

/// Why a container id string is not legal.
#[derive(Debug, thiserror::Error)]
pub enum ContainerIdError {
    /// Empty string.
    #[error("container id cannot be empty")]
    Empty,
    /// Contained whitespace or path separators.
    #[error("container id {0:?} contains forbidden characters")]
    ForbiddenChars(String),
}

impl ContainerId {
    /// Parse and validate a container id / name.
    pub fn parse(input: &str) -> Result<Self, ContainerIdError> {
        if input.is_empty() {
            return Err(ContainerIdError::Empty);
        }
        if input
            .chars()
            .any(|c| c.is_whitespace() || c == '/' || c == '\\')
        {
            return Err(ContainerIdError::ForbiddenChars(input.to_owned()));
        }
        Ok(Self(input.to_owned()))
    }

    /// Borrow the validated name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the inner string.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl<'de> serde::Deserialize<'de> for ContainerId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for ContainerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ContainerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests;
