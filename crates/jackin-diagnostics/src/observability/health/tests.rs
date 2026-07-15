// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{record_telemetry_rejection, telemetry_health_snapshot};

#[test]
fn facade_rejection_is_visible_in_snapshot() {
    let before = telemetry_health_snapshot().facade_rejections;
    record_telemetry_rejection();
    assert_eq!(telemetry_health_snapshot().facade_rejections, before + 1);
}
