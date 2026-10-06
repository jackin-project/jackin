// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn registered_retry_event_accepts_no_dynamic_fields() {
    record_retry_scheduled().expect("registered retry scheduling event");
}
