// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.to_owned(),
        dst: dst.to_owned(),
        readonly: false,
        isolation: MountIsolation::Shared,
    }
}

pub(super) fn named_mount(name: &str, src: &str, dst: &str) -> (Option<String>, MountConfig) {
    (Some(name.to_owned()), mount(src, dst))
}

pub(super) fn launch_configurations() -> BTreeMap<String, AgentConfiguration> {
    BTreeMap::from([
        (
            "claude-a".to_owned(),
            AgentConfiguration {
                agent: Agent::Claude,
                account: "a-claude".into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        ),
        (
            "claude-z".to_owned(),
            AgentConfiguration {
                agent: Agent::Claude,
                account: "z-claude".into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        ),
        (
            "claude-out".to_owned(),
            AgentConfiguration {
                agent: Agent::Claude,
                account: "outside".into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        ),
    ])
}
