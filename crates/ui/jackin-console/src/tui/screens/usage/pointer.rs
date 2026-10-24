// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    UsageFrameLayout, UsageListGeometry, UsageRowTarget, UsageScreenState, UsageWheelAction,
    WheelDirection, usage_wheel_action,
};
use ratatui::layout::Rect;

/// Opaque evidence that a Usage list was rendered for one area and state.
///
/// Only the Usage renderer can construct this receipt. It carries the actual
/// list geometry consumed by the frame and a structural state snapshot; it is
/// not a provider snapshot or a persisted Usage value.
#[derive(Clone)]
pub struct UsagePaintReceipt {
    area: Rect,
    _captured_epoch: i64,
    screen_snapshot: UsageScreenState,
    refresh_subscription_present: bool,
    layout: UsageFrameLayout,
    list_geometry: UsageListGeometry,
}

impl std::fmt::Debug for UsagePaintReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("UsagePaintReceipt")
    }
}

impl UsagePaintReceipt {
    pub(super) fn new(
        area: Rect,
        captured_epoch: i64,
        screen: &UsageScreenState,
        layout: UsageFrameLayout,
        list_geometry: UsageListGeometry,
    ) -> Self {
        Self {
            area,
            _captured_epoch: captured_epoch,
            screen_snapshot: screen.clone(),
            refresh_subscription_present: screen.refresh_rx.is_some(),
            layout,
            list_geometry,
        }
    }

    pub(crate) fn matches(&self, area: Rect, screen: &UsageScreenState) -> bool {
        self.area == area
            && self.screen_snapshot == *screen
            && self.refresh_subscription_present == screen.refresh_rx.is_some()
    }

    pub(crate) fn target_at(&self, x: u16, y: u16) -> Option<UsageRowTarget> {
        self.list_geometry
            .target_at(x, y, self.screen_snapshot.scroll)
    }

    pub(crate) fn wheel_action(
        &self,
        screen: &UsageScreenState,
        area: Rect,
        x: u16,
        y: u16,
        direction: WheelDirection,
    ) -> Option<UsageWheelAction> {
        self.matches(area, screen).then_some(())?;
        usage_wheel_action(self.layout, &self.screen_snapshot, x, y, direction)
    }
}
