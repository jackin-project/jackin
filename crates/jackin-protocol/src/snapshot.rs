// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side snapshot types shared between the console UI and the
//! runtime fetch layer.

use crate::control::TabSnapshot;

/// Current tab/pane state fetched from a running container's daemon.
#[derive(Debug, Clone)]
pub struct InstanceSnapshot {
    pub tabs: Vec<TabSnapshot>,
    pub active_tab: u32,
}
