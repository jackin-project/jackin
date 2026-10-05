// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Privacy-safe process telemetry vocabulary.

use std::path::Path;

use crate::schema::enums::ProcessExecutableName;

/// Classify a program by basename into the closed process vocabulary.
#[must_use]
pub fn classify_executable(program: &Path) -> ProcessExecutableName {
    let executable = program
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    match executable {
        "jackin" => ProcessExecutableName::Jackin,
        "jackin-daemon" => ProcessExecutableName::JackinDaemon,
        "jackin-capsule" => ProcessExecutableName::JackinCapsule,
        "jackin-role" => ProcessExecutableName::JackinRole,
        "git" => ProcessExecutableName::Git,
        "gh" => ProcessExecutableName::Gh,
        "op" => ProcessExecutableName::Op,
        "docker" => ProcessExecutableName::Docker,
        "container" => ProcessExecutableName::Container,
        "mise" => ProcessExecutableName::Mise,
        "ps" => ProcessExecutableName::Ps,
        "osascript" => ProcessExecutableName::Osascript,
        "sh" => ProcessExecutableName::Sh,
        "caffeinate" => ProcessExecutableName::Caffeinate,
        "kill" => ProcessExecutableName::Kill,
        "less" => ProcessExecutableName::Less,
        "more" => ProcessExecutableName::More,
        "bat" => ProcessExecutableName::Bat,
        "claude" => ProcessExecutableName::Claude,
        "codex" => ProcessExecutableName::Codex,
        "amp" => ProcessExecutableName::Amp,
        "kimi" => ProcessExecutableName::Kimi,
        "opencode" => ProcessExecutableName::Opencode,
        "grok" => ProcessExecutableName::Grok,
        "agy" => ProcessExecutableName::Agy,
        "gemini" => ProcessExecutableName::Gemini,
        "cursor-agent" => ProcessExecutableName::CursorAgent,
        "muse" => ProcessExecutableName::Muse,
        "omp" => ProcessExecutableName::Omp,
        "hermes" => ProcessExecutableName::Hermes,
        // Bare `agent` stays Other: it is both the Grok Build alias and
        // the Cursor installer default, so the basename is ambiguous.
        _ => ProcessExecutableName::Other,
    }
}

#[cfg(test)]
mod tests;

/// Awaited subprocess span whose dropped future records honest cancellation.
/// Normal completion remains explicit; unwinding records a panic.
#[derive(Debug)]
#[must_use]
pub struct ProcessOperationGuard {
    operation: crate::OperationGuard,
    completed: bool,
}

impl ProcessOperationGuard {
    /// Own an operation across subprocess awaits.
    pub const fn new(operation: crate::OperationGuard) -> Self {
        Self {
            operation,
            completed: false,
        }
    }

    /// Record the normal terminal outcome exactly once.
    pub fn complete(
        mut self,
        outcome: crate::schema::enums::OutcomeValue,
        error_type: Option<crate::schema::enums::ErrorType>,
    ) {
        self.completed = true;
        self.operation.complete_borrowed(outcome, error_type);
    }
}

impl std::ops::Deref for ProcessOperationGuard {
    type Target = crate::OperationGuard;

    fn deref(&self) -> &Self::Target {
        &self.operation
    }
}

impl Drop for ProcessOperationGuard {
    fn drop(&mut self) {
        if !self.completed {
            self.completed = true;
            if std::thread::panicking() {
                self.operation.complete_borrowed(
                    crate::schema::enums::OutcomeValue::Error,
                    Some(crate::schema::enums::ErrorType::Panic),
                );
            } else {
                self.operation
                    .complete_borrowed(crate::schema::enums::OutcomeValue::Cancellation, None);
            }
        }
    }
}
