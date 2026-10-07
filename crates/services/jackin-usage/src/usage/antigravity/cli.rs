// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Antigravity` CLI version probe and usage fetch.

use jackin_usage_provider_core::{PROVIDER_CLI_TIMEOUT, run_cli_with_timeout};

use super::{
    AntigravityCredits, AntigravityUsage, parse_antigravity_credits_output,
    parse_antigravity_usage_output,
};

/// Minimum `agy` version with read-only `/usage|/credits --output-format json`.
pub(crate) const ANTIGRAVITY_MIN_JSON_VERSION: (u64, u64, u64) = (1, 1, 11);

/// macOS Keychain service holding the Antigravity OAuth grant singleton.
/// Discovery probes its *presence* only (never the payload): the CLI owns
/// the secret, jackin only shells out to it.
pub(crate) const ANTIGRAVITY_KEYCHAIN_SERVICE: &str = "gemini";

/// Parse `agy --version` output into `(major, minor, patch)`. Accepts a bare
/// `1.2.5` or a decorated line (`agy version 1.2.5 (build …)`); `None` when no
/// `N.N.N` triple is present.
pub(crate) fn parse_agy_version(text: &str) -> Option<(u64, u64, u64)> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-'))
        .find_map(|part| {
            let mut segments = part.split('.');
            match (segments.next(), segments.next(), segments.next()) {
                (Some(major), Some(minor), Some(patch))
                    if major.chars().all(|ch| ch.is_ascii_digit())
                        && minor.chars().all(|ch| ch.is_ascii_digit())
                        && patch.chars().all(|ch| ch.is_ascii_digit()) =>
                {
                    Some((
                        major.parse().ok()?,
                        minor.parse().ok()?,
                        patch.parse().ok()?,
                    ))
                }
                _ => None,
            }
        })
}

/// True when `version` supports the official JSON usage commands.
pub(crate) fn agy_version_supports_json(version: (u64, u64, u64)) -> bool {
    version >= ANTIGRAVITY_MIN_JSON_VERSION
}

/// Probe `agy --version` and enforce the JSON gate. `Err` carries the exact
/// non-secret reason (binary missing, unparseable version, too old).
pub(crate) fn antigravity_cli_version() -> Result<(u64, u64, u64), String> {
    let text = run_cli_with_timeout("agy", &["--version"], PROVIDER_CLI_TIMEOUT)
        .map_err(|error| format!("Antigravity CLI unavailable: {error}"))?;
    let version = parse_agy_version(&text)
        .ok_or_else(|| "Antigravity CLI version was not recognized".to_owned())?;
    if !agy_version_supports_json(version) {
        return Err(format!(
            "Antigravity CLI {}.{}.{} predates JSON usage (needs >= 1.1.11)",
            version.0, version.1, version.2
        ));
    }
    Ok(version)
}

pub(crate) fn fetch_antigravity_cli_usage() -> Result<AntigravityUsage, String> {
    antigravity_cli_version()?;
    let output = run_cli_with_timeout(
        "agy",
        &["-p", "/usage", "--output-format", "json"],
        PROVIDER_CLI_TIMEOUT,
    )
    .map_err(|error| format!("Antigravity /usage request failed: {error}"))?;
    parse_antigravity_usage_output(&output)
}

pub(crate) fn fetch_antigravity_cli_credits() -> Result<AntigravityCredits, String> {
    antigravity_cli_version()?;
    let output = run_cli_with_timeout(
        "agy",
        &["-p", "/credits", "--output-format", "json"],
        PROVIDER_CLI_TIMEOUT,
    )
    .map_err(|error| format!("Antigravity /credits request failed: {error}"))?;
    parse_antigravity_credits_output(&output)
}
