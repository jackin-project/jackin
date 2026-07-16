// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use super::{ALL_KEYS, enums, events, metrics, spans};

#[test]
fn extension_namespaces_are_neutral_and_unique() {
    let keys = ALL_KEYS.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(keys.len(), ALL_KEYS.len());
    for name in ALL_KEYS
        .iter()
        .chain(events::ALL)
        .chain(spans::ALL)
        .chain(metrics::ALL)
    {
        assert!(!name.starts_with("jackin.") && !name.starts_with("parallax."));
    }
}

#[test]
fn contract_closed_set_sizes_are_stable() {
    assert_eq!(enums::OutcomeValue::ALL.len(), 6);
    assert_eq!(enums::LaunchStageName::ALL.len(), 11);
    assert_eq!(enums::AgentName::ALL.len(), 6);
    assert_eq!(enums::ScreenId::ALL.len(), 6);
    assert_eq!(metrics::ALL.len(), 32);
}
