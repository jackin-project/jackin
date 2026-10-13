// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn row_text(buf: &Buffer, row: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, row)].symbol().to_owned())
        .collect()
}

pub(super) fn hint_row(area: Rect) -> u16 {
    area.height.saturating_sub(3)
}
