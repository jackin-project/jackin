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

#[test]
fn upstream_error_enum_accepts_custom_instrumentation_values() {
    let requirements = schema::events::OPERATION_WARN_DEF.attributes;
    for value in ["timeout", "custom_domain_error", "_OTHER"] {
        assert_eq!(
            validation::attribute(
                requirements,
                Attr {
                    key: schema::attrs::std_attrs::ERROR_TYPE,
                    value: Value::Str(value)
                }
            ),
            Ok(())
        );
    }
    assert!(
        requirements
            .iter()
            .find(|field| field.name == "error.type")
            .unwrap()
            .allowed_values
            .is_empty()
    );
    assert_eq!(
        validation::attribute(
            requirements,
            Attr {
                key: schema::attrs::std_attrs::ERROR_TYPE,
                value: Value::Bool(true)
            }
        ),
        Err(Rejection::InvalidValue)
    );
}

#[test]
fn upstream_http_and_db_known_values_are_not_exhaustive() {
    for (requirements, attribute) in [
        (
            schema::spans::HTTP_CLIENT_DEF.attributes,
            Attr {
                key: schema::attrs::std_attrs::HTTP_REQUEST_METHOD,
                value: Value::Str("CUSTOM_METHOD"),
            },
        ),
        (
            schema::spans::DB_CLIENT_DEF.attributes,
            Attr {
                key: schema::attrs::std_attrs::DB_SYSTEM_NAME,
                value: Value::Str("custom_database"),
            },
        ),
    ] {
        assert!(
            requirements
                .iter()
                .find(|field| field.name == attribute.key)
                .unwrap()
                .allowed_values
                .is_empty()
        );
        assert_eq!(validation::attribute(requirements, attribute), Ok(()));
    }
}

#[test]
fn local_enum_closure_and_open_enum_privacy_limits_are_preserved() {
    for (metadata, attribute) in [
        (
            schema::attrs::APP_MODE_DEF,
            Attr {
                key: schema::attrs::APP_MODE,
                value: Value::Str("invalid-local-value"),
            },
        ),
        (
            schema::attrs::OUTCOME_DEF,
            Attr {
                key: schema::attrs::OUTCOME,
                value: Value::Str("invalid-local-value"),
            },
        ),
    ] {
        let requirement = schema::AttributeRequirement {
            name: metadata.name,
            value_type: metadata.value_type,
            requirement: schema::RequirementLevel::Recommended,
            allowed_values: metadata.allowed_values,
        };
        assert!(!requirement.allowed_values.is_empty());
        assert_eq!(
            validation::attribute(&[requirement], attribute),
            Err(Rejection::InvalidValue)
        );
    }
    let requirements = schema::events::OPERATION_WARN_DEF.attributes;
    let key = "error.type";
    assert_eq!(
        validation::attribute(
            requirements,
            Attr {
                key,
                value: Value::Str("password=secret")
            }
        ),
        Err(Rejection::Privacy)
    );
    let oversized = "x".repeat(4097);
    assert_eq!(
        validation::attribute(
            requirements,
            Attr {
                key,
                value: Value::Str(&oversized)
            }
        ),
        Err(Rejection::SizeLimit)
    );
    assert_eq!(
        validation::attribute(
            requirements,
            Attr {
                key: "unknown.attribute",
                value: Value::Str("safe")
            }
        ),
        Err(Rejection::UnknownAttribute)
    );
}
