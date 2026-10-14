// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(test)]

pub fn install_all_test_stubs(paths: &jackin_core::JackinPaths) {
    use jackin_core::Agent;
    for agent in &[
        Agent::Claude,
        Agent::Codex,
        Agent::Amp,
        Agent::Kimi,
        Agent::Opencode,
    ] {
        jackin_image::agent_binary::install_test_stub(paths, *agent)
            .expect("install agent binary test stub");
    }
    jackin_image::capsule_binary::install_test_stub(paths).expect("install capsule test stub");
}
