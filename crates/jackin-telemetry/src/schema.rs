// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Generated semantic-convention constants and bounded values.

pub mod attrs;
pub mod enums;
pub mod events;
pub mod metrics;
pub mod spans;

pub use attrs::ALL_KEYS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueType {
    String,
    Boolean,
    Integer,
    Double,
    StringArray,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequirementLevel {
    Required,
    Recommended,
    ConditionallyRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttributeRequirement {
    pub name: &'static str,
    pub value_type: ValueType,
    pub requirement: RequirementLevel,
    pub allowed_values: &'static [&'static str],
}

#[derive(Clone, Copy, Debug)]
pub struct AttributeMetadata {
    pub name: &'static str,
    pub description: &'static str,
    pub value_type: ValueType,
    pub allowed_values: &'static [&'static str],
}

#[derive(Clone, Copy, Debug)]
pub struct EventMetadata {
    pub name: &'static str,
    pub description: &'static str,
    pub attributes: &'static [AttributeRequirement],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpanKind {
    Internal,
    Client,
    Server,
    Producer,
    Consumer,
}

#[derive(Clone, Copy, Debug)]
pub struct SpanMetadata {
    pub name: &'static str,
    pub description: &'static str,
    pub kind: SpanKind,
    pub attributes: &'static [AttributeRequirement],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricInstrument {
    Counter,
    Gauge,
    UpDownCounter,
    Histogram,
}

#[derive(Clone, Copy, Debug)]
pub struct MetricMetadata {
    pub name: &'static str,
    pub description: &'static str,
    pub instrument: MetricInstrument,
    pub unit: &'static str,
    pub boundaries: &'static [f64],
    pub attributes: &'static [AttributeRequirement],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigVersionDirection {
    From,
    To,
}

#[must_use]
pub fn valid_config_schema_version(
    scope: &str,
    direction: ConfigVersionDirection,
    value: &str,
) -> bool {
    use enums::{
        GlobalConfigSchemaVersionFrom, GlobalConfigSchemaVersionTo,
        WorkspaceConfigSchemaVersionFrom, WorkspaceConfigSchemaVersionTo,
    };
    match (scope, direction) {
        ("global", ConfigVersionDirection::From) => GlobalConfigSchemaVersionFrom::ALL
            .iter()
            .any(|version| version.as_str() == value),
        ("global", ConfigVersionDirection::To) => GlobalConfigSchemaVersionTo::ALL
            .iter()
            .any(|version| version.as_str() == value),
        ("workspace", ConfigVersionDirection::From) => WorkspaceConfigSchemaVersionFrom::ALL
            .iter()
            .any(|version| version.as_str() == value),
        ("workspace", ConfigVersionDirection::To) => WorkspaceConfigSchemaVersionTo::ALL
            .iter()
            .any(|version| version.as_str() == value),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
