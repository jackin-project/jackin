// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::model::{AccountQuota, Meter, Note, UsageView};
use super::render::{Ctx, Line};

pub fn meter_lines(
    _account: &AccountQuota,
    _meter: &Meter,
    _view: &UsageView,
    _ctx: &Ctx,
) -> Vec<Line> {
    Vec::new()
}

pub fn note_lines(
    _account: &AccountQuota,
    _note: &Note,
    _view: &UsageView,
    _ctx: &Ctx,
) -> Vec<Line> {
    Vec::new()
}

pub fn footer(_account: &AccountQuota, _view: &UsageView, _ctx: &Ctx) -> Vec<Line> {
    Vec::new()
}
