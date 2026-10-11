// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
fn assert_action_frame_plan_avoids_full_diff_tier(plan: ActionFramePlan) {
    if let ActionFramePlan::Full(reason) = plan {
        assert_clear_tier_reason(reason);
    }
}

fn assert_dialog_frame_plan_avoids_full_diff_tier(plan: DialogActionFramePlan) {
    if let DialogActionFramePlan::Full(reason) = plan {
        assert_clear_tier_reason(reason);
    }
}

fn assert_clear_tier_reason(reason: FullRedrawReason) {
    assert!(
        matches!(
            reason,
            FullRedrawReason::FirstAttach
                | FullRedrawReason::Resize
                | FullRedrawReason::TabSwitch
                | FullRedrawReason::LayoutChange
                | FullRedrawReason::SplitClose
                | FullRedrawReason::ZoomChange
                | FullRedrawReason::SessionExit
                | FullRedrawReason::ExplicitRedraw
        ),
        "{reason:?} must not route through a full clear redraw"
    );
}

