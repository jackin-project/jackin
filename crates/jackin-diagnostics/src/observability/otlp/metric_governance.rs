// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MetricShape {
    GaugeF64,
    GaugeU64,
    SumI64,
    SumU64,
    HistogramF64,
}

pub(super) fn metric_contract_fields(
    name: &str,
    metric_description: &str,
    metric_unit: &str,
) -> Result<
    (
        MetricShape,
        &'static [jackin_telemetry::schema::AttributeRequirement],
    ),
    jackin_telemetry::Rejection,
> {
    use jackin_telemetry::schema::{MetricInstrument, metrics};
    let (shape, description, unit, attributes): (
        MetricShape,
        &'static str,
        &'static str,
        &'static [jackin_telemetry::schema::AttributeRequirement],
    ) = if let Some(definition) = metrics::definition(name) {
        let shape = match definition.instrument {
            MetricInstrument::Counter => MetricShape::SumU64,
            MetricInstrument::UpDownCounter => MetricShape::SumI64,
            MetricInstrument::Histogram => MetricShape::HistogramF64,
        };
        (
            shape,
            definition.description,
            definition.unit,
            definition.attributes,
        )
    } else {
        match name {
            opentelemetry_semantic_conventions::metric::PROCESS_CPU_UTILIZATION => (
                MetricShape::GaugeF64,
                "Fraction of total host CPU used by the jackin process",
                "1",
                &[],
            ),
            opentelemetry_semantic_conventions::metric::PROCESS_MEMORY_USAGE => (
                MetricShape::SumI64,
                "Resident set size of the jackin process",
                "By",
                &[],
            ),
            "tokio.runtime.workers" => (
                MetricShape::GaugeU64,
                "Worker threads driving the tokio runtime",
                "",
                &[],
            ),
            "tokio.runtime.alive_tasks" => (
                MetricShape::GaugeU64,
                "Tasks currently alive in the tokio runtime",
                "",
                &[],
            ),
            "tokio.runtime.global_queue.depth" => (
                MetricShape::GaugeU64,
                "Tasks waiting in the tokio runtime's global queue",
                "",
                &[],
            ),
            _ => return Err(jackin_telemetry::Rejection::UnknownName),
        }
    };
    if metric_description != description || metric_unit != unit {
        return Err(jackin_telemetry::Rejection::InvalidValue);
    }
    Ok((shape, attributes))
}

pub(super) fn validate_metric_attributes<'a>(
    requirements: &[jackin_telemetry::schema::AttributeRequirement],
    attributes: impl Iterator<Item = &'a opentelemetry::KeyValue>,
) -> Result<(), jackin_telemetry::Rejection> {
    use jackin_telemetry::schema::{RequirementLevel, ValueType};
    let attributes = attributes.collect::<Vec<_>>();
    if attributes.len() > jackin_telemetry::limits::MAX_METRIC_ATTRIBUTES {
        return Err(jackin_telemetry::Rejection::SizeLimit);
    }
    for (index, attribute) in attributes.iter().enumerate() {
        let key = attribute.key.as_str();
        jackin_telemetry::privacy::validate_key(key)?;
        if attributes[..index]
            .iter()
            .any(|prior| prior.key.as_str() == key)
        {
            return Err(jackin_telemetry::Rejection::InvalidValue);
        }
        let requirement = requirements
            .iter()
            .find(|requirement| requirement.name == key)
            .ok_or(jackin_telemetry::Rejection::UnknownAttribute)?;
        let valid_type = matches!(
            (&attribute.value, requirement.value_type),
            (opentelemetry::Value::String(_), ValueType::String)
                | (opentelemetry::Value::Bool(_), ValueType::Boolean)
                | (opentelemetry::Value::I64(_), ValueType::Integer)
                | (opentelemetry::Value::F64(_), ValueType::Double)
                | (
                    opentelemetry::Value::Array(opentelemetry::Array::String(_)),
                    ValueType::StringArray
                )
        );
        if !valid_type {
            return Err(jackin_telemetry::Rejection::InvalidValue);
        }
        match &attribute.value {
            opentelemetry::Value::String(value) => {
                jackin_telemetry::privacy::validate_string(value.as_str())?;
            }
            opentelemetry::Value::Array(opentelemetry::Array::String(values)) => {
                for value in values {
                    jackin_telemetry::privacy::validate_string(value.as_str())?;
                }
            }
            _ => {}
        }
        match &attribute.value {
            opentelemetry::Value::String(value)
                if value.as_str().len() > jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES =>
            {
                return Err(jackin_telemetry::Rejection::SizeLimit);
            }
            opentelemetry::Value::Array(opentelemetry::Array::String(values))
                if values.len() > jackin_telemetry::limits::MAX_ARRAY_ELEMENTS
                    || values.iter().any(|value| {
                        value.as_str().len() > jackin_telemetry::limits::MAX_STRING_ATTRIBUTE_BYTES
                    }) =>
            {
                return Err(jackin_telemetry::Rejection::SizeLimit);
            }
            _ => {}
        }
        if !requirement.allowed_values.is_empty()
            && !matches!(&attribute.value, opentelemetry::Value::String(value) if requirement.allowed_values.contains(&value.as_str()))
        {
            return Err(jackin_telemetry::Rejection::InvalidValue);
        }
    }
    if requirements
        .iter()
        .filter(|requirement| requirement.requirement == RequirementLevel::Required)
        .any(|requirement| {
            !attributes
                .iter()
                .any(|attribute| attribute.key.as_str() == requirement.name)
        })
    {
        return Err(jackin_telemetry::Rejection::InvalidValue);
    }
    Ok(())
}

pub(super) fn validate_metric_points<T>(
    points: impl Iterator<Item = T>,
    attributes: impl Fn(&T) -> Vec<&opentelemetry::KeyValue>,
    requirements: &[jackin_telemetry::schema::AttributeRequirement],
) -> Result<(), jackin_telemetry::Rejection> {
    let points = points.collect::<Vec<_>>();
    if points.len() > jackin_telemetry::limits::MAX_CARDINALITY {
        return Err(jackin_telemetry::Rejection::Cardinality);
    }
    for point in &points {
        validate_metric_attributes(requirements, attributes(point).into_iter())?;
    }
    Ok(())
}

fn validate_metric(
    metric: &opentelemetry_sdk::metrics::data::Metric,
) -> Result<(), jackin_telemetry::Rejection> {
    use opentelemetry_sdk::metrics::data::{AggregatedMetrics, MetricData};
    jackin_telemetry::limits::validate_name(metric.name())?;
    let (expected, requirements) =
        metric_contract_fields(metric.name(), metric.description(), metric.unit())?;
    match metric.data() {
        AggregatedMetrics::F64(MetricData::Gauge(data)) if expected == MetricShape::GaugeF64 => {
            validate_metric_points(
                data.data_points(),
                |point| point.attributes().collect(),
                requirements,
            )
        }
        AggregatedMetrics::U64(MetricData::Gauge(data)) if expected == MetricShape::GaugeU64 => {
            validate_metric_points(
                data.data_points(),
                |point| point.attributes().collect(),
                requirements,
            )
        }
        AggregatedMetrics::I64(MetricData::Sum(data)) if expected == MetricShape::SumI64 => {
            validate_metric_points(
                data.data_points(),
                |point| point.attributes().collect(),
                requirements,
            )
        }
        AggregatedMetrics::U64(MetricData::Sum(data)) if expected == MetricShape::SumU64 => {
            validate_metric_points(
                data.data_points(),
                |point| point.attributes().collect(),
                requirements,
            )
        }
        AggregatedMetrics::F64(MetricData::Histogram(data))
            if expected == MetricShape::HistogramF64 =>
        {
            validate_metric_points(
                data.data_points(),
                |point| point.attributes().collect(),
                requirements,
            )
        }
        _ => Err(jackin_telemetry::Rejection::InvalidValue),
    }
}

pub(super) fn validate_metric_export(
    metrics: &opentelemetry_sdk::metrics::data::ResourceMetrics,
) -> Result<(), jackin_telemetry::Rejection> {
    for scope in metrics.scope_metrics() {
        if scope.scope().name() != "jackin" {
            return Err(jackin_telemetry::Rejection::UnknownName);
        }
        for metric in scope.metrics() {
            validate_metric(metric)?;
        }
    }
    Ok(())
}
