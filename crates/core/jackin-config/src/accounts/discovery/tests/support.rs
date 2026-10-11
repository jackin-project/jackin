// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn write_codex_fixture(directory: &Path, token: &str) {
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(
        directory.join("auth.json"),
        format!(r#"{{"tokens":{{"access_token":"{token}"}}}}"#),
    )
    .unwrap();
}

pub(super) fn codex_accounts(report: &DiscoveryReport) -> Vec<&DiscoveredAccount> {
    report
        .accounts
        .iter()
        .filter(|account| account.agent == Agent::Codex)
        .collect()
}
