// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(any(test, feature = "test-support"))]

use crate::AppConfig;
use crate::schema::RoleSource;

#[cfg(any(test, feature = "test-support"))]
pub fn config_with_agents(names: &[&str]) -> AppConfig {
    let mut config = AppConfig::default();
    for name in names {
        config.roles.insert((*name).into(), RoleSource::default());
    }
    config
}
