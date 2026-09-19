// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn capsule_config_with(instances: &[(&str, &str)]) -> jackin_protocol::CapsuleConfig {
    let mut config = jackin_protocol::CapsuleConfig::default();
    for (id, agent) in instances {
        config.instances.push((*id).to_owned());
        config.agents.insert((*id).to_owned(), (*agent).to_owned());
    }
    config
}

#[test]
fn initial_argv_prefers_first_matching_instance() {
    let config = capsule_config_with(&[
        ("claude-work", "claude"),
        ("claude-personal", "claude"),
        ("codex-work", "codex"),
    ]);
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Claude, &config),
        "claude-work"
    );
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Codex, &config),
        "codex-work"
    );
}

#[test]
fn initial_argv_falls_back_to_first_instance_then_slug() {
    let config = capsule_config_with(&[("codex-work", "codex")]);
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Claude, &config),
        "codex-work"
    );
    let empty = jackin_protocol::CapsuleConfig::default();
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Claude, &empty),
        "claude"
    );
}
