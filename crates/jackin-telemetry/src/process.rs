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
        _ => ProcessExecutableName::Other,
    }
}

#[cfg(test)]
mod tests;
