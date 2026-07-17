// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::logs::LogRecord as _;
use opentelemetry_sdk::Resource;

use super::EXPORT_TARGETS;

#[derive(Debug)]
pub(super) struct GovernedLogProcessor<P>(pub(super) P);

impl<P: opentelemetry_sdk::logs::LogProcessor> opentelemetry_sdk::logs::LogProcessor
    for GovernedLogProcessor<P>
{
    fn emit(
        &self,
        record: &mut opentelemetry_sdk::logs::SdkLogRecord,
        instrumentation: &opentelemetry::InstrumentationScope,
    ) {
        let governed = instrumentation.name() == jackin_telemetry::TELEMETRY_TARGET
            || record
                .target()
                .is_some_and(|target| EXPORT_TARGETS.contains(&target.as_ref()));
        if governed {
            let Some(event_name) = record.event_name() else {
                reject(
                    jackin_telemetry::Signal::Log,
                    jackin_telemetry::Rejection::UnknownName,
                );
                return;
            };
            if let Err(reason) = jackin_telemetry::limits::validate_name(event_name) {
                reject(jackin_telemetry::Signal::Log, reason);
                return;
            }
            let Some(canonical_severity) = jackin_telemetry::event::canonical_severity(event_name)
            else {
                reject(
                    jackin_telemetry::Signal::Log,
                    jackin_telemetry::Rejection::UnknownName,
                );
                return;
            };
            if record.severity_number() != Some(otel_log_severity(canonical_severity)) {
                reject(
                    jackin_telemetry::Signal::Log,
                    jackin_telemetry::Rejection::InvalidValue,
                );
                return;
            }
            for (key, values) in jackin_telemetry::event::take_pending_event_arrays() {
                record.add_attribute(
                    key,
                    opentelemetry::logs::AnyValue::ListAny(Box::new(
                        values
                            .into_iter()
                            .map(|value| opentelemetry::logs::AnyValue::String(value.into()))
                            .collect(),
                    )),
                );
            }
            let Some(definition) = jackin_telemetry::schema::events::definition(event_name) else {
                reject(
                    jackin_telemetry::Signal::Log,
                    jackin_telemetry::Rejection::UnknownName,
                );
                return;
            };
            if let Err(reason) = validate_log_record(record, definition) {
                reject(jackin_telemetry::Signal::Log, reason);
                return;
            }
        }
        self.0.emit(record, instrumentation);
    }

    fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.0.force_flush()
    }

    fn shutdown_with_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> opentelemetry_sdk::error::OTelSdkResult {
        self.0.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.0.set_resource(resource);
    }
}

#[derive(Debug)]
pub(super) struct GovernedSpanProcessor<P>(pub(super) P);

impl<P: opentelemetry_sdk::trace::SpanProcessor> opentelemetry_sdk::trace::SpanProcessor
    for GovernedSpanProcessor<P>
{
    fn on_start(
        &self,
        span: &mut opentelemetry_sdk::trace::Span,
        context: &opentelemetry::Context,
    ) {
        self.0.on_start(span, context);
    }

    fn on_end(&self, span: opentelemetry_sdk::trace::SpanData) {
        if let Err(reason) = jackin_telemetry::limits::validate_name(span.name.as_ref()) {
            reject(jackin_telemetry::Signal::Trace, reason);
            return;
        }
        let Some(definition) = jackin_telemetry::schema::spans::definition(span.name.as_ref())
        else {
            reject(
                jackin_telemetry::Signal::Trace,
                jackin_telemetry::Rejection::UnknownName,
            );
            return;
        };
        if let Err(reason) = validate_span(&span, definition) {
            reject(jackin_telemetry::Signal::Trace, reason);
            return;
        }
        self.0.on_end(span);
    }

    fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.0.force_flush()
    }

    fn shutdown_with_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> opentelemetry_sdk::error::OTelSdkResult {
        self.0.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.0.set_resource(resource);
    }
}

fn validate_log_record(
    record: &opentelemetry_sdk::logs::SdkLogRecord,
    definition: &jackin_telemetry::schema::EventMetadata,
) -> Result<(), jackin_telemetry::Rejection> {
    use opentelemetry::logs::AnyValue;

    if record.attributes_iter().count() > jackin_telemetry::limits::MAX_LOG_ATTRIBUTES {
        return Err(jackin_telemetry::Rejection::SizeLimit);
    }
    if matches!(record.body(), Some(AnyValue::String(body)) if body.as_str().len() > jackin_telemetry::limits::MAX_BODY_BYTES)
    {
        return Err(jackin_telemetry::Rejection::SizeLimit);
    }
    if let Some(AnyValue::String(body)) = record.body() {
        jackin_telemetry::privacy::validate_string(body.as_str())?;
    }
    for (index, (key, value)) in record.attributes_iter().enumerate() {
        if record
            .attributes_iter()
            .take(index)
            .any(|(prior, _)| prior == key)
        {
            return Err(jackin_telemetry::Rejection::InvalidValue);
        }
        jackin_telemetry::privacy::validate_key(key.as_str())?;
        let Some(requirement) = definition
            .attributes
            .iter()
            .find(|requirement| requirement.name == key.as_str())
        else {
            return Err(jackin_telemetry::Rejection::UnknownAttribute);
        };
        validate_log_value(key.as_str(), value, requirement)?;
    }
    validate_required(definition.attributes, |required| {
        record
            .attributes_iter()
            .any(|(key, _)| key.as_str() == required)
    })
}

fn validate_log_value(
    key: &str,
    value: &opentelemetry::logs::AnyValue,
    requirement: &jackin_telemetry::schema::AttributeRequirement,
) -> Result<(), jackin_telemetry::Rejection> {
    use jackin_telemetry::schema::ValueType;
    use opentelemetry::logs::AnyValue;

    let valid_type = matches!(
        (value, requirement.value_type),
        (AnyValue::String(_), ValueType::String)
            | (AnyValue::Boolean(_), ValueType::Boolean)
            | (AnyValue::Int(_), ValueType::Integer)
            | (AnyValue::Double(_), ValueType::Double)
            | (AnyValue::ListAny(_), ValueType::StringArray)
    );
    if !valid_type {
        return Err(jackin_telemetry::Rejection::InvalidValue);
    }
    jackin_telemetry::privacy::validate_key(key)?;
    let maximum = if matches!(key, "exception.message" | "exception.stacktrace") {
        jackin_telemetry::limits::MAX_BODY_BYTES
    } else {
        jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES
    };
    match value {
        AnyValue::String(value) => {
            jackin_telemetry::privacy::validate_attribute_string(key, value.as_str())?;
            if !requirement.allowed_values.is_empty()
                && !requirement.allowed_values.contains(&value.as_str())
            {
                return Err(jackin_telemetry::Rejection::InvalidValue);
            }
            if value.as_str().len() > maximum {
                return Err(jackin_telemetry::Rejection::SizeLimit);
            }
            Ok(())
        }
        AnyValue::ListAny(values) => {
            if values.len() > jackin_telemetry::limits::MAX_ARRAY_ELEMENTS {
                return Err(jackin_telemetry::Rejection::SizeLimit);
            }
            for value in values.iter() {
                let AnyValue::String(item) = value else {
                    return Err(jackin_telemetry::Rejection::InvalidValue);
                };
                if item.as_str().len() > maximum {
                    return Err(jackin_telemetry::Rejection::SizeLimit);
                }
                jackin_telemetry::privacy::validate_attribute_string(key, item.as_str())?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn validate_span(
    span: &opentelemetry_sdk::trace::SpanData,
    definition: &jackin_telemetry::schema::SpanMetadata,
) -> Result<(), jackin_telemetry::Rejection> {
    validate_span_limits(
        span.attributes.len(),
        span.links.iter().map(|link| link.attributes.len()),
    )?;
    if let opentelemetry::trace::Status::Error { description } = &span.status {
        if description.len() > jackin_telemetry::limits::MAX_BODY_BYTES {
            return Err(jackin_telemetry::Rejection::SizeLimit);
        }
        jackin_telemetry::privacy::validate_string(description)?;
    }
    for link in span.links.iter() {
        if link.attributes.len() > jackin_telemetry::limits::MAX_SPAN_ATTRIBUTES {
            return Err(jackin_telemetry::Rejection::SizeLimit);
        }
        for (index, attribute) in link.attributes.iter().enumerate() {
            if link.attributes[..index]
                .iter()
                .any(|prior| prior.key == attribute.key)
            {
                return Err(jackin_telemetry::Rejection::InvalidValue);
            }
            jackin_telemetry::privacy::validate_key(attribute.key.as_str())?;
            validate_untyped_span_value(&attribute.value)?;
        }
    }
    for (index, attribute) in span.attributes.iter().enumerate() {
        if span.attributes[..index]
            .iter()
            .any(|prior| prior.key == attribute.key)
        {
            return Err(jackin_telemetry::Rejection::InvalidValue);
        }
        if attribute.key.as_str() == jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE {
            let opentelemetry::Value::String(value) = &attribute.value else {
                return Err(jackin_telemetry::Rejection::InvalidValue);
            };
            if !jackin_telemetry::schema::enums::ErrorType::ALL
                .iter()
                .any(|candidate| candidate.as_str() == value.as_str())
            {
                return Err(jackin_telemetry::Rejection::InvalidValue);
            }
            continue;
        }
        jackin_telemetry::privacy::validate_key(attribute.key.as_str())?;
        let Some(requirement) = definition
            .attributes
            .iter()
            .find(|requirement| requirement.name == attribute.key.as_str())
        else {
            return Err(jackin_telemetry::Rejection::UnknownAttribute);
        };
        validate_span_value(attribute.key.as_str(), &attribute.value, requirement)?;
    }
    validate_required(definition.attributes, |required| {
        span.attributes
            .iter()
            .any(|attribute| attribute.key.as_str() == required)
    })?;
    Ok(())
}

fn validate_span_limits(
    span_attributes: usize,
    mut link_attributes: impl ExactSizeIterator<Item = usize>,
) -> Result<(), jackin_telemetry::Rejection> {
    if span_attributes > jackin_telemetry::limits::MAX_SPAN_ATTRIBUTES
        || link_attributes.len() > jackin_telemetry::limits::MAX_SPAN_LINKS
        || link_attributes
            .try_fold(span_attributes, usize::checked_add)
            .is_none_or(|count| count > jackin_telemetry::limits::MAX_SPAN_ATTRIBUTES)
    {
        return Err(jackin_telemetry::Rejection::SizeLimit);
    }
    Ok(())
}

fn validate_span_value(
    key: &str,
    value: &opentelemetry::Value,
    requirement: &jackin_telemetry::schema::AttributeRequirement,
) -> Result<(), jackin_telemetry::Rejection> {
    use jackin_telemetry::schema::ValueType;
    use opentelemetry::{Array, Value};
    let valid = matches!(
        (value, requirement.value_type),
        (Value::String(_), ValueType::String)
            | (Value::Bool(_), ValueType::Boolean)
            | (Value::I64(_), ValueType::Integer)
            | (Value::F64(_), ValueType::Double)
            | (Value::Array(Array::String(_)), ValueType::StringArray)
    );
    if !valid {
        return Err(jackin_telemetry::Rejection::InvalidValue);
    }
    validate_span_attribute_value(key, value)?;
    if matches!(value, Value::String(value) if !requirement.allowed_values.is_empty() && !requirement.allowed_values.contains(&value.as_str()))
    {
        return Err(jackin_telemetry::Rejection::InvalidValue);
    }
    Ok(())
}

fn validate_span_attribute_value(
    key: &str,
    value: &opentelemetry::Value,
) -> Result<(), jackin_telemetry::Rejection> {
    use opentelemetry::{Array, Value};
    match value {
        Value::String(value) => {
            if value.as_str().len() > jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES {
                return Err(jackin_telemetry::Rejection::SizeLimit);
            }
            jackin_telemetry::privacy::validate_attribute_string(key, value.as_str())
        }
        Value::Array(Array::String(values)) => {
            if values.len() > jackin_telemetry::limits::MAX_ARRAY_ELEMENTS {
                return Err(jackin_telemetry::Rejection::SizeLimit);
            }
            for value in values {
                if value.as_str().len() > jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES {
                    return Err(jackin_telemetry::Rejection::SizeLimit);
                }
                jackin_telemetry::privacy::validate_attribute_string(key, value.as_str())?;
            }
            Ok(())
        }
        Value::Array(_) => Err(jackin_telemetry::Rejection::InvalidValue),
        _ => Ok(()),
    }
}

fn validate_untyped_span_value(
    value: &opentelemetry::Value,
) -> Result<(), jackin_telemetry::Rejection> {
    use opentelemetry::{Array, Value};
    match value {
        Value::String(value) => {
            if value.as_str().len() > jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES {
                return Err(jackin_telemetry::Rejection::SizeLimit);
            }
            jackin_telemetry::privacy::validate_string(value.as_str())
        }
        Value::Array(Array::String(values)) => {
            if values.len() > jackin_telemetry::limits::MAX_ARRAY_ELEMENTS {
                return Err(jackin_telemetry::Rejection::SizeLimit);
            }
            for value in values {
                if value.as_str().len() > jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES {
                    return Err(jackin_telemetry::Rejection::SizeLimit);
                }
                jackin_telemetry::privacy::validate_string(value.as_str())?;
            }
            Ok(())
        }
        Value::Array(_) => Err(jackin_telemetry::Rejection::InvalidValue),
        _ => Ok(()),
    }
}

fn validate_required(
    requirements: &[jackin_telemetry::schema::AttributeRequirement],
    mut contains: impl FnMut(&str) -> bool,
) -> Result<(), jackin_telemetry::Rejection> {
    if requirements
        .iter()
        .filter(|requirement| {
            requirement.requirement == jackin_telemetry::schema::RequirementLevel::Required
        })
        .any(|requirement| !contains(requirement.name))
    {
        Err(jackin_telemetry::Rejection::InvalidValue)
    } else {
        Ok(())
    }
}

fn reject(signal: jackin_telemetry::Signal, rejection: jackin_telemetry::Rejection) {
    jackin_telemetry::record_export_rejection(signal, rejection);
}

const fn otel_log_severity(
    severity: jackin_telemetry::event::Severity,
) -> opentelemetry::logs::Severity {
    match severity {
        jackin_telemetry::event::Severity::Trace => opentelemetry::logs::Severity::Trace,
        jackin_telemetry::event::Severity::Debug => opentelemetry::logs::Severity::Debug,
        jackin_telemetry::event::Severity::Info => opentelemetry::logs::Severity::Info,
        jackin_telemetry::event::Severity::Warn => opentelemetry::logs::Severity::Warn,
        jackin_telemetry::event::Severity::Error => opentelemetry::logs::Severity::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        validate_log_value, validate_span_attribute_value, validate_span_limits,
        validate_span_value, validate_untyped_span_value,
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
}
