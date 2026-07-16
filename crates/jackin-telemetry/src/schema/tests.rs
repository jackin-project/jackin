// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use super::{ALL_KEYS, attrs, enums, events, metrics, spans};

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
    assert_eq!(metrics::ALL.len(), 34);
}

#[test]
fn bounded_standard_values_drive_runtime_validation() {
    assert_eq!(
        attrs::APP_SCREEN_ID_DEF.allowed_values,
        enums::ScreenId::ALL
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        attrs::GEN_AI_PROVIDER_NAME_DEF.allowed_values,
        enums::ProviderName::ALL
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        attrs::DB_OPERATION_NAME_DEF.allowed_values,
        enums::DbOperationName::ALL
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn upstream_aliases_match_registry_wire_names() {
    assert!(
        attrs::std_attrs::UPSTREAM_ALIASES
            .iter()
            .all(|(constant, wire_name)| constant == wire_name)
    );
    assert_eq!(
        opentelemetry_semantic_conventions::SCHEMA_URL,
        attrs::std_attrs::RUST_CRATE_SCHEMA_URL
    );
    assert_eq!(
        attrs::std_attrs::ERROR_TYPE,
        opentelemetry_semantic_conventions::attribute::ERROR_TYPE
    );
    assert_eq!(
        attrs::std_attrs::HTTP_REQUEST_METHOD,
        opentelemetry_semantic_conventions::attribute::HTTP_REQUEST_METHOD
    );
    assert_eq!(
        attrs::std_attrs::SERVICE_NAME,
        opentelemetry_semantic_conventions::attribute::SERVICE_NAME
    );
}

#[test]
fn generated_definition_tables_are_bidirectionally_complete() {
    let attribute_names = attrs::ALL_DEFINITIONS
        .iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(attribute_names, ALL_KEYS.iter().copied().collect());
    assert!(
        ALL_KEYS
            .iter()
            .all(|name| attrs::definition(name).is_some())
    );

    let event_names = events::DEFINITIONS
        .iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(event_names, events::ALL.iter().copied().collect());
    assert!(
        events::ALL
            .iter()
            .all(|name| events::definition(name).is_some())
    );

    let span_names = spans::DEFINITIONS
        .iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(span_names, spans::ALL.iter().copied().collect());
    assert!(
        spans::ALL
            .iter()
            .all(|name| spans::definition(name).is_some())
    );

    let metric_names = metrics::DEFINITIONS
        .iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(metric_names, metrics::ALL.iter().copied().collect());
    assert!(
        metrics::ALL
            .iter()
            .all(|name| metrics::definition(name).is_some())
    );
}

#[test]
fn facade_definitions_exactly_cover_generated_events_and_metrics() {
    let facade_events = crate::event::ALL
        .iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(facade_events, events::ALL.iter().copied().collect());
    assert!(crate::event::ALL.iter().all(|definition| {
        let generated = events::definition(definition.name).unwrap();
        definition.metadata.name == generated.name
            && definition.metadata.description == generated.description
            && definition.metadata.attributes.len() == generated.attributes.len()
    }));

    let facade_metrics = crate::metric::ALL
        .iter()
        .map(|definition| definition.name)
        .collect::<BTreeSet<_>>();
    assert_eq!(facade_metrics, metrics::ALL.iter().copied().collect());
}

#[test]
fn config_version_sets_are_scope_and_direction_specific() {
    use super::{ConfigVersionDirection, valid_config_schema_version};

    assert!(valid_config_schema_version(
        "global",
        ConfigVersionDirection::From,
        "legacy"
    ));
    assert!(valid_config_schema_version(
        "global",
        ConfigVersionDirection::To,
        "v1alpha9"
    ));
    assert!(!valid_config_schema_version(
        "global",
        ConfigVersionDirection::To,
        "legacy"
    ));
    assert!(!valid_config_schema_version(
        "workspace",
        ConfigVersionDirection::From,
        "v1alpha9"
    ));
    assert!(!valid_config_schema_version(
        "workspace",
        ConfigVersionDirection::To,
        "v1alpha9"
    ));
}
