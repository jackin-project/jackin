// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const EMPTY_STRINGS: &[&str] = &[];

fn valid_value(requirement: &schema::AttributeRequirement) -> Value<'static> {
    if let Some(value) = requirement.allowed_values.first() {
        return Value::Str(value);
    }
    match requirement.value_type {
        schema::ValueType::String => Value::Str("fixture"),
        schema::ValueType::Boolean => Value::Bool(true),
        schema::ValueType::Integer => Value::I64(1),
        schema::ValueType::Double => Value::F64(1.0),
        schema::ValueType::StringArray => Value::StrArray(EMPTY_STRINGS),
    }
}

fn wrong_type(expected: schema::ValueType) -> Value<'static> {
    match expected {
        schema::ValueType::String => Value::Bool(true),
        _ => Value::Str("wrong-type"),
    }
}

fn required_attrs(definition: &EventDef) -> Vec<Attr<'static>> {
    definition
        .metadata
        .attributes
        .iter()
        .filter(|requirement| requirement.requirement == schema::RequirementLevel::Required)
        .map(|requirement| Attr {
            key: requirement.name,
            value: valid_value(requirement),
        })
        .collect()
}

#[test]
fn every_event_enforces_generated_attribute_contract() {
    for definition in ALL {
        let required = required_attrs(definition);
        assert_eq!(
            validate(definition, &FieldSet::new(&required, None)),
            Ok(()),
            "valid required fields rejected for {}",
            definition.name
        );

        if let Some(missing) = required.first() {
            let absent = required
                .iter()
                .copied()
                .filter(|attr| attr.key != missing.key)
                .collect::<Vec<_>>();
            assert_eq!(
                validate(definition, &FieldSet::new(&absent, None)),
                Err(Rejection::InvalidValue),
                "missing required field accepted for {}",
                definition.name
            );
        }

        let unknown = schema::ALL_KEYS
            .iter()
            .copied()
            .find(|key| {
                schema::attrs::definition(key)
                    .is_some_and(|metadata| metadata.value_type == schema::ValueType::String)
                    && !definition
                        .metadata
                        .attributes
                        .iter()
                        .any(|requirement| requirement.name == *key)
            })
            .expect("event registry must not allow every attribute");
        let mut extra = required.clone();
        extra.push(Attr {
            key: unknown,
            value: Value::Str("fixture"),
        });
        assert_eq!(
            validate(definition, &FieldSet::new(&extra, None)),
            Err(Rejection::UnknownAttribute),
            "unknown field accepted for {}",
            definition.name
        );

        for requirement in definition.metadata.attributes {
            let mut invalid = required
                .iter()
                .copied()
                .filter(|attr| attr.key != requirement.name)
                .collect::<Vec<_>>();
            invalid.push(Attr {
                key: requirement.name,
                value: wrong_type(requirement.value_type),
            });
            assert_eq!(
                validate(definition, &FieldSet::new(&invalid, None)),
                Err(Rejection::InvalidValue),
                "wrong type accepted for {}:{}",
                definition.name,
                requirement.name
            );

            if !requirement.allowed_values.is_empty() {
                invalid.pop();
                invalid.push(Attr {
                    key: requirement.name,
                    value: Value::Str("invalid-enum-value"),
                });
                assert_eq!(
                    validate(definition, &FieldSet::new(&invalid, None)),
                    Err(Rejection::InvalidValue),
                    "invalid enum accepted for {}:{}",
                    definition.name,
                    requirement.name
                );
            }
        }
    }
}
