// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::Instant;

mod host_open;
mod hover;
mod selection;

pub(crate) use host_open::{host_url_opening_allowed, is_double_click};

#[derive(Clone, Copy, Debug)]
pub(crate) struct PanePress {
    pub(super) session_id: u64,
    pub(super) content_row: usize,
    pub(super) col: u16,
    pub(super) at: Instant,
}
