// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn stage_prefetched_agent_binary_copies_codex_sibling() {
    let dir = tempdir().unwrap();
    let agent_bin_dir = dir.path().join("agent-binaries");
    std::fs::create_dir_all(&agent_bin_dir).unwrap();
    let host = dir.path().join("codex");
    std::fs::write(&host, b"codex-binary").unwrap();
    let sibling = dir.path().join(jackin_core::CODEX_CODE_MODE_HOST_BINARY);
    std::fs::write(&sibling, b"code-mode-host").unwrap();

    let ctx = stage_prefetched_agent_binary(&agent_bin_dir, Agent::Codex, &host).unwrap();

    assert_eq!(ctx, ".jackin-runtime/agent-binaries/codex");
    assert_eq!(
        std::fs::read(agent_bin_dir.join("codex")).unwrap(),
        b"codex-binary"
    );
    assert_eq!(
        std::fs::read(agent_bin_dir.join(jackin_core::CODEX_CODE_MODE_HOST_BINARY)).unwrap(),
        b"code-mode-host"
    );
}

#[test]
fn stage_prefetched_agent_binary_stages_primary_without_sibling() {
    let dir = tempdir().unwrap();
    let agent_bin_dir = dir.path().join("agent-binaries");
    std::fs::create_dir_all(&agent_bin_dir).unwrap();
    let host = dir.path().join("claude");
    std::fs::write(&host, b"claude-binary").unwrap();

    let ctx = stage_prefetched_agent_binary(&agent_bin_dir, Agent::Claude, &host).unwrap();

    assert_eq!(ctx, ".jackin-runtime/agent-binaries/claude");
    assert_eq!(
        std::fs::read(agent_bin_dir.join("claude")).unwrap(),
        b"claude-binary"
    );
}
