// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[test]
fn decision_metric_family_has_only_bounded_cache_dimensions() {
    let name = crate::schema::attrs::CACHE_NAME;
    let result = crate::schema::attrs::CACHE_RESULT;
    assert_eq!(
        crate::metric::CACHE_DECISIONS
            .dimensions()
            .iter()
            .map(|attribute| attribute.name)
            .collect::<Vec<_>>(),
        [name, result]
    );
    assert_eq!(
        crate::metric::CACHE_DECISION_ACTIVE
            .dimensions()
            .iter()
            .map(|attribute| attribute.name)
            .collect::<Vec<_>>(),
        [name]
    );
    assert_eq!(
        crate::metric::CACHE_DECISION_DURATION
            .dimensions()
            .iter()
            .map(|attribute| attribute.name)
            .collect::<Vec<_>>(),
        [name, result]
    );
}
