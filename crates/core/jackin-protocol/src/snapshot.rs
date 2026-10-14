// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::control::TabSnapshot;

#[derive(Debug, Clone)]
pub struct InstanceSnapshot {
    pub tabs: Vec<TabSnapshot>,
    pub active_tab: u32,
}
