// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn seed_latest(paths: &JackinPaths, agent: Agent, version: &str) {
    let release = crate::agent_binary::AgentRelease {
        agent,
        version: version.to_owned(),
        url: "https://example.invalid/agent".to_owned(),
        checksum: None,
        archive_member: None,
    };
    let path = paths
        .cache_dir
        .join("agent-binaries")
        .join(agent.slug())
        .join("latest.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string(&release).unwrap()).unwrap();
}
