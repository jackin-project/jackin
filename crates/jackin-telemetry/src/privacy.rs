// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{
    event::{Rejection, Value},
    schema,
};

pub fn validate_key(key: &str) -> Result<(), Rejection> {
    let extension = schema::ALL_KEYS.contains(&key);
    let standard = schema::attrs::std_attrs::ALL_KEYS.contains(&key);
    if extension || standard {
        Ok(())
    } else {
        Err(Rejection::UnknownAttribute)
    }
}

/// Reject values which are payload material rather than bounded telemetry
/// dimensions. Redaction is reserved for governed human-readable bodies and
/// exception fields; attributes fail closed.
pub fn validate_value(value: &Value<'_>) -> Result<(), Rejection> {
    match value {
        Value::Str(value) => validate_string(value),
        Value::StrArray(values) => values.iter().try_for_each(|value| validate_string(value)),
        _ => Ok(()),
    }
}

pub fn validate_string(value: &str) -> Result<(), Rejection> {
    let lower = value.to_ascii_lowercase();
    let looks_sensitive = value.contains('\u{1b}')
        || value.contains('\n')
        || value.contains('\r')
        || value.starts_with('/')
        || value.starts_with("~/")
        || value.contains("://")
        || lower.contains("password=")
        || lower.contains("token=")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("authorization:")
        || lower.contains("bearer ");
    if looks_sensitive {
        Err(Rejection::Privacy)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
