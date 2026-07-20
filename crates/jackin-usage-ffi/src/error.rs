// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{error::Error, fmt, panic::AssertUnwindSafe};

/// Typed bridge failures. Messages never include credentials or tokens.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
pub enum UsageBridgeError {
    /// Recoverable rejection (validation, unknown surface, closed runtime).
    Rejected { code: String, message: String },
    /// A Rust panic was caught at the facade boundary.
    ContainedPanic { message: String },
    /// Runtime not opened or already shut down.
    RuntimeUnavailable,
    /// Event cursor requires resync.
    ResyncRequired,
}

impl UsageBridgeError {
    #[must_use]
    pub fn rejected(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Rejected {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for UsageBridgeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected { code, message } => write!(formatter, "{code}: {message}"),
            Self::ContainedPanic { message } => {
                write!(formatter, "contained panic: {message}")
            }
            Self::RuntimeUnavailable => formatter.write_str("runtime unavailable"),
            Self::ResyncRequired => formatter.write_str("event cursor requires resync"),
        }
    }
}

impl Error for UsageBridgeError {}

/// Run `body` and convert panics into [`UsageBridgeError::ContainedPanic`].
pub(crate) fn catch_entry<T>(
    body: impl FnOnce() -> Result<T, UsageBridgeError>,
) -> Result<T, UsageBridgeError> {
    match std::panic::catch_unwind(AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(payload) => Err(UsageBridgeError::ContainedPanic {
            message: panic_message(payload),
        }),
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "non-string panic payload".to_owned()
}
