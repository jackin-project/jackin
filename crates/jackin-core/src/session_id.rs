// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Daemon session identity newtype (`u64` multiplexed session id).

use std::fmt;

/// Capsule daemon session id (opaque positive handle).
///
/// Wraps the daemon's `u64` session key so attach/protocol/daemon APIs cannot
/// confuse session ids with other integers. Construction is fallible only for
/// the zero value (reserved as "unset" in some control paths).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct SessionId(u64);

/// Why a session id is not legal.
#[derive(Debug, thiserror::Error)]
pub enum SessionIdError {
    /// Zero is reserved as an unset / invalid handle.
    #[error("session id cannot be zero")]
    Zero,
}

impl SessionId {
    /// Validate and wrap a raw session id.
    pub fn new(raw: u64) -> Result<Self, SessionIdError> {
        if raw == 0 {
            return Err(SessionIdError::Zero);
        }
        Ok(Self(raw))
    }

    /// Borrow the raw id.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
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

#[cfg(test)]
mod tests;
