// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for protocol types.
use super::*;

#[test]
fn label_round_trips_through_from_label() {
    for provider in Provider::ALL {
        assert_eq!(Provider::from_label(provider.label()), Some(provider));
    }
    assert_eq!(Provider::from_label("Gemini"), None);
}
