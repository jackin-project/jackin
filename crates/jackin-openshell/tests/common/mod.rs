// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::unwrap_used,
    reason = "test helpers fail the test by panicking"
)]

use jackin_core::Agent;
use jackin_openshell::ProviderProfile;

const HEADER: &str = "version: 1
filesystem_policy:
  read_only:
    - /bin
landlock:
  compatibility: hard_requirement
";

pub(crate) fn with_header(tail: &str) -> String {
    format!("{HEADER}{tail}")
}

pub(crate) fn rule_doc(rules: &[(&str, &str, u16)]) -> String {
    let mut text = with_header("network_policies:\n");
    for (key, host, port) in rules {
        text.push_str(&format!(
            "  {key}:\n    name: {key}\n    endpoints:\n      - host: '{host}'\n        port: {port}\n    binaries:\n      - path: /usr/bin/curl\n"
        ));
    }
    text
}

pub(crate) fn profile() -> ProviderProfile {
    ProviderProfile::parse(&Agent::Codex.spec().openshell.chatgpt_profile).unwrap()
}
