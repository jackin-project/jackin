// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential source display and proxy helpers.

/// non-empty. Centralizes the "skip the env push when the value is
/// missing or blank" check used by every optional env injection.
pub(super) fn push_env_if_present(env_strings: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(v) = value
        && !v.is_empty()
    {
        env_strings.push(format!("{key}={v}"));
    }
}

/// Canonical CLI proxy env vars `curl`, `wget`, and Go's HTTP client read.
/// `FTP_PROXY` / `RSYNC_PROXY` are intentionally out of scope: they don't
/// reach `DinD`'s daemon socket, so adding them here would only widen the
/// detection surface without changing bypass behavior.
pub(super) const PROXY_VAR_NAMES: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];
pub(super) const NO_PROXY_UPPER: &str = "NO_PROXY";
pub(super) const NO_PROXY_LOWER: &str = "no_proxy";

pub(super) fn is_proxy_env_name(key: &str) -> bool {
    PROXY_VAR_NAMES.contains(&key)
}

pub(crate) fn append_no_proxy_host(value: &str, host: &str) -> String {
    if value
        .split(',')
        .map(str::trim)
        .any(|entry| entry.eq_ignore_ascii_case(host))
    {
        return value.to_owned();
    }

    if value.trim().is_empty() {
        host.to_owned()
    } else {
        format!("{value},{host}")
    }
}

/// Printable source reference for the credential env var `env_var` (e.g.
/// `"CLAUDE_CODE_OAUTH_TOKEN"`, `"ANTHROPIC_API_KEY"`) given the raw
/// (unresolved) declaration value from the operator env config (e.g.
/// `"Private/Claude/security/auth token"` or `"$CLAUDE_CODE_OAUTH_TOKEN"`).
/// Produces the `"KEY ← value"` form; falls back to the bare env-var name
/// when `raw` is `None` or empty.
pub(super) fn auth_token_source_reference(env_var: &str, raw: Option<&str>) -> String {
    match raw {
        None | Some("") => env_var.to_owned(),
        Some(value) => format!("{env_var} \u{2190} {value}"),
    }
}
