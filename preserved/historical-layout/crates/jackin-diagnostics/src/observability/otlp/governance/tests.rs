// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    validate_log_value, validate_span_attribute_value, validate_span_limits, validate_span_value,
    validate_untyped_span_value,
};
use jackin_telemetry::{Rejection, limits};

const STRING: jackin_telemetry::schema::AttributeRequirement =
    jackin_telemetry::schema::AttributeRequirement {
        name: "app.screen.id",
        value_type: jackin_telemetry::schema::ValueType::String,
        requirement: jackin_telemetry::schema::RequirementLevel::Recommended,
        allowed_values: &[],
    };

#[test]
fn span_limits_include_all_link_attributes() {
    assert_eq!(
        validate_span_limits(limits::MAX_SPAN_ATTRIBUTES - 1, [1].into_iter()),
        Ok(())
    );
    assert_eq!(
        validate_span_limits(limits::MAX_SPAN_ATTRIBUTES, [1].into_iter()),
        Err(Rejection::SizeLimit)
    );
    assert_eq!(
        validate_span_limits(0, [0; limits::MAX_SPAN_LINKS + 1].into_iter()),
        Err(Rejection::SizeLimit)
    );
}

#[test]
fn log_second_line_rejects_type_privacy_and_size_violations() {
    use opentelemetry::logs::AnyValue;

    assert_eq!(
        validate_log_value("app.screen.id", &AnyValue::Boolean(true), &STRING),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(
        validate_log_value(
            "app.screen.id",
            &AnyValue::String("/private/path".into()),
            &STRING,
        ),
        Err(Rejection::Privacy)
    );
    assert_eq!(
        validate_log_value(
            "app.screen.id",
            &AnyValue::String("x".repeat(limits::MAX_STRING_ATTRIBUTE_BYTES + 1).into(),),
            &STRING,
        ),
        Err(Rejection::SizeLimit)
    );

    let values = vec![AnyValue::String("bounded".into()); limits::MAX_ARRAY_ELEMENTS + 1];
    let array = jackin_telemetry::schema::AttributeRequirement {
        value_type: jackin_telemetry::schema::ValueType::StringArray,
        ..STRING
    };
    assert_eq!(
        validate_log_value("app.screen.id", &AnyValue::ListAny(values.into()), &array),
        Err(Rejection::SizeLimit)
    );
}

#[test]
fn span_second_line_rejects_type_privacy_size_and_link_violations() {
    use opentelemetry::{Array, Value};

    assert_eq!(
        validate_span_value("app.screen.id", &Value::Bool(true), &STRING),
        Err(Rejection::InvalidValue)
    );
    assert_eq!(
        validate_span_attribute_value("app.screen.id", &Value::String("token=secret".into())),
        Err(Rejection::Privacy)
    );
    assert_eq!(
        validate_span_attribute_value(
            "app.screen.id",
            &Value::String("x".repeat(limits::MAX_STRING_ATTRIBUTE_BYTES + 1).into()),
        ),
        Err(Rejection::SizeLimit)
    );
    assert_eq!(
        validate_span_attribute_value(
            "app.screen.id",
            &Value::Array(Array::String(vec![
                "bounded".into();
                limits::MAX_ARRAY_ELEMENTS + 1
            ],)),
        ),
        Err(Rejection::SizeLimit)
    );
    assert_eq!(
        validate_untyped_span_value(&Value::String("https://secret.invalid".into())),
        Err(Rejection::Privacy)
    );
}
