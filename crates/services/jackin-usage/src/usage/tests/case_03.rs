// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn codex_plan_display_name_matches_codexbar() {
    // ported from CodexBar's CodexPlanFormatting tests.
    assert_eq!(codex_plan_display_name("pro").as_deref(), Some("Pro 20x"));
    assert_eq!(codex_plan_display_name("Pro").as_deref(), Some("Pro 20x"));
    assert_eq!(
        codex_plan_display_name("Codex Pro").as_deref(),
        Some("Pro 20x")
    );
    assert_eq!(
        codex_plan_display_name("prolite").as_deref(),
        Some("Pro 5x")
    );
    assert_eq!(
        codex_plan_display_name("pro_lite").as_deref(),
        Some("Pro 5x")
    );
    assert_eq!(
        codex_plan_display_name("Pro Lite").as_deref(),
        Some("Pro 5x")
    );
    assert_eq!(
        codex_plan_display_name("Codex Pro Lite").as_deref(),
        Some("Pro 5x")
    );
    assert_eq!(codex_plan_display_name(""), None);
    assert_eq!(codex_plan_display_name("   "), None);
    assert_eq!(
        codex_plan_display_name("enterprise_cbp_usage_based").as_deref(),
        Some("Enterprise CBP Usage Based")
    );
    assert_eq!(codex_plan_display_name("k12").as_deref(), Some("K12"));
    assert_eq!(
        codex_plan_display_name("Enterprise").as_deref(),
        Some("Enterprise")
    );
}
