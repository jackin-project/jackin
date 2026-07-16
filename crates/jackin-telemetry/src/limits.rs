// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{borrow::Cow, sync::OnceLock};

use crate::event::{Rejection, Value};

pub const MAX_LOG_ATTRIBUTES: usize = 32;
pub const MAX_SPAN_ATTRIBUTES: usize = 64;
pub const MAX_SPAN_LINKS: usize = 8;
pub const MAX_METRIC_ATTRIBUTES: usize = 32;
pub const MAX_BODY_BYTES: usize = 4 * 1024;
pub const MAX_STRING_ATTRIBUTE_BYTES: usize = 1024;
pub const MAX_ARRAY_ELEMENTS: usize = 32;
pub const MAX_NAME_BYTES: usize = 128;
pub const MAX_CARDINALITY: usize = 256;

type Redactor = for<'a> fn(&'a str) -> Cow<'a, str>;
static REDACTOR: OnceLock<Redactor> = OnceLock::new();

pub fn install_redactor(redactor: Redactor) {
    let _already_installed = REDACTOR.set(redactor);
}

#[must_use]
pub fn redact_and_clamp(body: &str) -> Cow<'_, str> {
    clamp_body(body, |value| {
        REDACTOR
            .get()
            .map_or(Cow::Borrowed(value), |redact| redact(value))
    })
}

#[must_use]
pub fn clamp_body<'a>(body: &'a str, redact: impl FnOnce(&'a str) -> Cow<'a, str>) -> Cow<'a, str> {
    let redacted = redact(body);
    if redacted.len() <= MAX_BODY_BYTES {
        return redacted;
    }
    let mut end = MAX_BODY_BYTES;
    while !redacted.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(redacted[..end].to_owned())
}

pub fn validate_name(name: &str) -> Result<(), Rejection> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES {
        Err(Rejection::SizeLimit)
    } else {
        Ok(())
    }
}

pub fn validate_value(value: &Value<'_>) -> Result<(), Rejection> {
    match value {
        Value::Str(value) if value.len() > MAX_STRING_ATTRIBUTE_BYTES => Err(Rejection::SizeLimit),
        Value::StrArray(values) if values.len() > MAX_ARRAY_ELEMENTS => Err(Rejection::SizeLimit),
        Value::StrArray(values)
            if values
                .iter()
                .any(|value| value.len() > MAX_STRING_ATTRIBUTE_BYTES) =>
        {
            Err(Rejection::SizeLimit)
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
