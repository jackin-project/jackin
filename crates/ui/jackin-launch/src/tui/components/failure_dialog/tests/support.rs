// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn row_text(buf: &Buffer, row: u16, width: u16) -> String {
    (0..width)
        .map(|x| buf[(x, row)].symbol().to_owned())
        .collect()
}

pub(super) fn screen_text(buf: &Buffer, area: Rect) -> String {
    (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| buf[(x, y)].symbol().to_owned()))
        .collect()
}

pub(super) fn failure_with_summary(summary: &str) -> LaunchFailure {
    LaunchFailure {
        title: "Build failed".to_owned(),
        summary: summary.to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::DerivedImage,
    }
}
