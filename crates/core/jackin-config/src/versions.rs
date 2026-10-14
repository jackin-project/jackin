// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub const CURRENT_CONFIG_VERSION: &str = "v1alpha13";

pub const CURRENT_WORKSPACE_VERSION: &str = "v1alpha11";

pub const LEGACY_VERSION: &str = "legacy";

pub fn current_config_version() -> String {
    CURRENT_CONFIG_VERSION.to_owned()
}

pub fn current_workspace_version() -> String {
    CURRENT_WORKSPACE_VERSION.to_owned()
}
