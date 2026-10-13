// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{
    event::{Attr, Rejection, Value},
    limits, privacy, schema,
};

pub(crate) fn attributes(
    requirements: &[schema::AttributeRequirement],
    attrs: &[Attr<'_>],
    maximum: usize,
) -> Result<(), Rejection> {
    if attrs.len() > maximum {
        return Err(Rejection::SizeLimit);
    }
    for (index, attr) in attrs.iter().enumerate() {
        attribute(requirements, *attr)?;
        if attrs[..index].iter().any(|prior| prior.key == attr.key) {
            return Err(Rejection::InvalidValue);
        }
    }
    if requirements
        .iter()
        .filter(|requirement| requirement.requirement == schema::RequirementLevel::Required)
        .any(|requirement| !attrs.iter().any(|attr| attr.key == requirement.name))
    {
        return Err(Rejection::InvalidValue);
    }
    Ok(())
}

pub(crate) fn attribute(
    requirements: &[schema::AttributeRequirement],
    attr: Attr<'_>,
) -> Result<(), Rejection> {
    privacy::validate_key(attr.key)?;
    let exception = matches!(attr.key, "exception.message" | "exception.stacktrace");
    if !exception {
        match attr.value {
            Value::Str(value) => privacy::validate_attribute_string(attr.key, value)?,
            Value::StrArray(values) => {
                for value in values {
                    privacy::validate_attribute_string(attr.key, value)?;
                }
            }
            _ => {}
        }
    }
    limits::validate_attribute_value(attr.key, &attr.value)?;
    let Some(requirement) = requirements
        .iter()
        .find(|requirement| requirement.name == attr.key)
    else {
        return Err(Rejection::UnknownAttribute);
    };
    if !value_matches(attr.value, requirement.value_type)
        || !requirement.allowed_values.is_empty()
            && !matches!(attr.value, Value::Str(value) if requirement.allowed_values.contains(&value))
    {
        return Err(Rejection::InvalidValue);
    }
    Ok(())
}

const fn value_matches(value: Value<'_>, expected: schema::ValueType) -> bool {
    match (value, expected) {
        (Value::Str(_), schema::ValueType::String)
        | (Value::Bool(_), schema::ValueType::Boolean)
        | (Value::I64(_), schema::ValueType::Integer)
        | (Value::F64(_), schema::ValueType::Double)
        | (Value::StrArray(_), schema::ValueType::StringArray) => true,
        (Value::U64(value), schema::ValueType::Integer) => value <= i64::MAX as u64,
        _ => false,
    }
}
