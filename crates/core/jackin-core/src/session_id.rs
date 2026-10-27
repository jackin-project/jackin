// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct SessionId(u64);

#[derive(Debug, thiserror::Error)]
pub enum SessionIdError {
    #[error("session id cannot be zero")]
    Zero,
}

impl SessionId {
    pub fn new(raw: u64) -> Result<Self, SessionIdError> {
        if raw == 0 {
            return Err(SessionIdError::Zero);
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl<'de> serde::Deserialize<'de> for SessionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<SessionId> for u64 {
    fn from(value: SessionId) -> Self {
        value.0
    }
}
