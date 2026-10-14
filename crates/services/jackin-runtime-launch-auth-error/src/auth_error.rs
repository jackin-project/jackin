// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn push_env_if_present(env_strings: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(v) = value
        && !v.is_empty()
    {
        env_strings.push(format!("{key}={v}"));
    }
}

pub const PROXY_VAR_NAMES: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];
pub const NO_PROXY_UPPER: &str = "NO_PROXY";
pub const NO_PROXY_LOWER: &str = "no_proxy";

pub fn is_proxy_env_name(key: &str) -> bool {
    PROXY_VAR_NAMES.contains(&key)
}

pub fn append_no_proxy_host(value: &str, host: &str) -> String {
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

pub fn auth_token_source_reference(env_var: &str, raw: Option<&str>) -> String {
    match raw {
        None | Some("") => env_var.to_owned(),
        Some(value) => format!("{env_var} \u{2190} {value}"),
    }
}
