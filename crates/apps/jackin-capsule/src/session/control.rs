// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Operator control: input, terminate, resize, and attributes.

use super::{Session, lock_or_record_poison};

use std::sync::atomic::Ordering;

use jackin_telemetry::ResultTelemetryExt as _;
use portable_pty::PtySize;

use crate::protocol::AgentState;

impl Session {
    #[must_use]
    pub fn send_input(&self, data: &[u8]) -> bool {
        // SendError fires when the writer task has exited (it owns the
        // receiver). The writer task emits SessionEvent::Exited before
        // dropping, so the daemon will reap this Session on the next
        // event tick. The writer boundary owns the originating failure.
        self.input_tx.send(data.to_vec()).is_ok()
    }

    /// Mark that the operator sent an explicit keyboard payload to this pane.
    /// Returns true when this clears a previously latched blocked state.
    pub fn mark_operator_input(&mut self) -> bool {
        let was_blocked = self.state == AgentState::Blocked;
        // Operator input updates recency evidence only. It never authors state
        // (that was the old flap bug: a keystroke in a blocked dialog flipped
        // Blocked→Working). State comes from evidence arbitration.
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

    /// Most recently announced working directory (OSC 7), if any.
    #[must_use]
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        // A pane collapsed below its border height yields a 0-row inner rect.
        // Never hand the agent PTY a 0×0 window size (programs expect ≥1) nor the
        // shadow grid a degenerate geometry. `DamageGrid::set_size` clamps too;
        // this keeps TIOCSWINSZ and the model in agreement on the floor.
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
        // Re-clamp through the grid: set_size may have shrunk the filled
        // scrollback the offset was clamped against.
        self.shadow_grid.set_scrollback(self.scrollback_offset());
    }
}
