// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `HostUsageRuntime` event cursor reads.

use super::{HostEventBatch, HostUsageEvent, HostUsageRuntime, MAX_EVENT_BATCH};

impl HostUsageRuntime {
    /// Poll events after `cursor` (exclusive), up to `max`.
    pub fn next_events(&mut self, cursor: u64, max: u32) -> Result<HostEventBatch, String> {
        self.require_open()?;
        let max = max.clamp(1, MAX_EVENT_BATCH) as usize;
        if self.events.is_empty() {
            return Ok(HostEventBatch {
                next_cursor: self.next_seq,
                events: Vec::new(),
                resync_required: false,
            });
        }
        let first = self.events.front().map_or(0, |e| e.sequence);
        if cursor + 1 < first {
            return Ok(HostEventBatch {
                next_cursor: self.next_seq,
                events: Vec::new(),
                resync_required: true,
            });
        }
        let events: Vec<HostUsageEvent> = self
            .events
            .iter()
            .filter(|event| event.sequence > cursor)
            .take(max)
            .cloned()
            .collect();
        let next_cursor = events.last().map_or(cursor, |event| event.sequence);
        Ok(HostEventBatch {
            next_cursor,
            events,
            resync_required: false,
        })
    }
}
