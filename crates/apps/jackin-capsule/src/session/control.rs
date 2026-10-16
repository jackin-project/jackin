// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{InputOwnership, Session, lock_or_record_poison};

use std::sync::atomic::Ordering;

use jackin_telemetry::ResultTelemetryExt as _;
use portable_pty::PtySize;

use crate::protocol::AgentState;

impl Session {
    #[must_use]
    pub fn send_input(&self, data: &[u8]) -> bool {
        self.input_tx.send(data.to_vec()).is_ok()
    }

    #[must_use]
    pub const fn operator_owned(&self) -> bool {
        matches!(self.input_ownership, InputOwnership::Operator)
    }

    pub fn take_operator_control(&mut self) {
        self.input_ownership = InputOwnership::Operator;
    }

    pub fn release_operator_control(&mut self) {
        self.input_ownership = InputOwnership::Automatic;
    }

    pub fn mark_operator_input(&mut self) -> bool {
        let was_blocked = self.state == AgentState::Blocked;
        self.last_input_at = std::time::Instant::now();
        was_blocked
    }

    pub fn terminate(&self) {
        self.termination_requested.store(true, Ordering::Release);
        if let Some(mut killer) = lock_or_record_poison(&self.child_killer) {
            drop(
                killer
                    .kill()
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError),
            );
        }
    }

    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    #[must_use]
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        if let Some(master) = lock_or_record_poison(&self.pty_master) {
            drop(
                master
                    .resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError),
            );
        }
        self.shadow_grid.set_size(rows, cols);
        self.shadow_grid.set_scrollback(self.scrollback_offset());
    }
}
