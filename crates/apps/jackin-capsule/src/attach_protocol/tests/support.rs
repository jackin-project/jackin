// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn launch_config_with_instances(instances: &[&str]) -> jackin_protocol::CapsuleConfig {
    jackin_protocol::CapsuleConfig {
        instances: instances.iter().map(ToString::to_string).collect(),
        ..jackin_protocol::CapsuleConfig::default()
    }
}
