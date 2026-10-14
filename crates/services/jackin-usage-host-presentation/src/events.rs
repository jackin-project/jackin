// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostUsageEvent {
    pub sequence: u64,
    pub kind: String,
    pub surface_id: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEventBatch {
    pub next_cursor: u64,
    pub events: Vec<HostUsageEvent>,
    pub resync_required: bool,
}

pub const MAX_EVENT_LOG: usize = 4_096;
pub const MAX_EVENT_BATCH: u32 = 256;
