// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pure, pre-provider validation of standard OTLP environment configuration.

use std::fmt;
use std::time::Duration;

const ENDPOINT_VARS: [&str; 4] = [
    "OTEL_EXPORTER_OTLP_ENDPOINT",
    "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
    "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
    "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
];

const PROTOCOL_VARS: [&str; 4] = [
    "OTEL_EXPORTER_OTLP_PROTOCOL",
    "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL",
    "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL",
    "OTEL_EXPORTER_OTLP_METRICS_PROTOCOL",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OtlpConfig {
    pub traces_endpoint: String,
    pub logs_endpoint: String,
    pub metrics_endpoint: String,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OtlpConfigError {
    MissingSignalEndpoint(&'static str),
    UnsupportedProtocol {
        variable: &'static str,
        value: String,
    },
    ConflictingSampler(String),
    UnsupportedCompression {
        variable: &'static str,
        value: String,
    },
    InvalidTimeout {
        variable: &'static str,
        value: String,
    },
    InvalidHeaders {
        variable: &'static str,
        value: String,
    },
    InvalidResourceAttribute(String),
    EmptyValue(&'static str),
}

impl fmt::Display for OtlpConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSignalEndpoint(signal) => {
                write!(
                    f,
                    "OTLP {signal} endpoint is required when telemetry is enabled"
                )
            }
            Self::UnsupportedProtocol { variable, value } => write!(
                f,
                "{variable}={value} is unsupported; jackin exports OTLP over grpc only"
            ),
            Self::ConflictingSampler(value) => write!(
                f,
                "OTEL_TRACES_SAMPLER={value} conflicts with required parentbased_always_on"
            ),
            Self::UnsupportedCompression { variable, value } => {
                write!(f, "{variable}={value} is unsupported; expected gzip")
            }
            Self::InvalidTimeout { variable, value } => {
                write!(
                    f,
                    "{variable}={value} must be a positive integer millisecond timeout"
                )
            }
            Self::InvalidHeaders { variable, value } => {
                write!(f, "{variable} contains an invalid OTLP header: {value}")
            }
            Self::InvalidResourceAttribute(value) => {
                write!(
                    f,
                    "OTEL_RESOURCE_ATTRIBUTES contains an invalid entry: {value}"
                )
            }
            Self::EmptyValue(variable) => write!(f, "{variable} must not be empty when set"),
        }
    }
}

impl std::error::Error for OtlpConfigError {}

pub(super) fn resolve_otlp_config(
    env: &impl Fn(&str) -> Option<String>,
) -> Result<Option<OtlpConfig>, OtlpConfigError> {
    if env("OTEL_SDK_DISABLED").is_some_and(|value| value.trim().eq_ignore_ascii_case("true")) {
        return Ok(None);
    }

    let base = nonempty(env("OTEL_EXPORTER_OTLP_ENDPOINT"));
    let traces = nonempty(env("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")).or_else(|| base.clone());
    let logs = nonempty(env("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT")).or_else(|| base.clone());
    let metrics = nonempty(env("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT")).or_else(|| base.clone());
    if base.is_none() && traces.is_none() && logs.is_none() && metrics.is_none() {
        return Ok(None);
    }

    validate_protocols(env)?;
    validate_sampler(env)?;
    validate_compression(env)?;
    validate_nonempty_optional(env, "OTEL_SERVICE_NAME")?;
    for variable in [
        "OTEL_EXPORTER_OTLP_CERTIFICATE",
        "OTEL_EXPORTER_OTLP_TRACES_CERTIFICATE",
        "OTEL_EXPORTER_OTLP_LOGS_CERTIFICATE",
        "OTEL_EXPORTER_OTLP_METRICS_CERTIFICATE",
        "OTEL_EXPORTER_OTLP_CLIENT_KEY",
        "OTEL_EXPORTER_OTLP_CLIENT_CERTIFICATE",
    ] {
        validate_nonempty_optional(env, variable)?;
    }

    if let Some(attributes) = env("OTEL_RESOURCE_ATTRIBUTES") {
        for entry in attributes
            .split(',')
            .filter(|entry| !entry.trim().is_empty())
        {
            let Some((key, value)) = entry.split_once('=') else {
                return Err(OtlpConfigError::InvalidResourceAttribute(entry.to_owned()));
            };
            if key.trim().is_empty() || value.trim().is_empty() {
                return Err(OtlpConfigError::InvalidResourceAttribute(entry.to_owned()));
            }
        }
    }

    let timeout = parse_timeout(env)?;
    parse_headers(env)?;
    Ok(Some(OtlpConfig {
        traces_endpoint: normalize_endpoint(
            traces.ok_or(OtlpConfigError::MissingSignalEndpoint("traces"))?,
        ),
        logs_endpoint: normalize_endpoint(
            logs.ok_or(OtlpConfigError::MissingSignalEndpoint("logs"))?,
        ),
        metrics_endpoint: normalize_endpoint(
            metrics.ok_or(OtlpConfigError::MissingSignalEndpoint("metrics"))?,
        ),
        timeout,
    }))
}

pub(super) fn any_endpoint_configured(env: &impl Fn(&str) -> Option<String>) -> bool {
    ENDPOINT_VARS
        .iter()
        .any(|variable| nonempty(env(variable)).is_some())
}

fn validate_protocols(env: &impl Fn(&str) -> Option<String>) -> Result<(), OtlpConfigError> {
    for variable in PROTOCOL_VARS {
        if let Some(value) = nonempty(env(variable))
            && value.trim() != "grpc"
        {
            return Err(OtlpConfigError::UnsupportedProtocol { variable, value });
        }
    }
    Ok(())
}

fn validate_sampler(env: &impl Fn(&str) -> Option<String>) -> Result<(), OtlpConfigError> {
    if let Some(value) = nonempty(env("OTEL_TRACES_SAMPLER"))
        && value.trim() != "parentbased_always_on"
    {
        return Err(OtlpConfigError::ConflictingSampler(value));
    }
    Ok(())
}

fn validate_compression(env: &impl Fn(&str) -> Option<String>) -> Result<(), OtlpConfigError> {
    for variable in [
        "OTEL_EXPORTER_OTLP_COMPRESSION",
        "OTEL_EXPORTER_OTLP_TRACES_COMPRESSION",
        "OTEL_EXPORTER_OTLP_LOGS_COMPRESSION",
        "OTEL_EXPORTER_OTLP_METRICS_COMPRESSION",
    ] {
        if let Some(value) = nonempty(env(variable))
            && value.trim() != "gzip"
        {
            return Err(OtlpConfigError::UnsupportedCompression { variable, value });
        }
    }
    Ok(())
}

fn parse_timeout(env: &impl Fn(&str) -> Option<String>) -> Result<Duration, OtlpConfigError> {
    let variable = "OTEL_EXPORTER_OTLP_TIMEOUT";
    let Some(value) = nonempty(env(variable)) else {
        return Ok(Duration::from_secs(5));
    };
    let millis = value
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|millis| *millis > 0)
        .ok_or_else(|| OtlpConfigError::InvalidTimeout {
            variable,
            value: value.clone(),
        })?;
    Ok(Duration::from_millis(millis.min(5_000)))
}

fn parse_headers(
    env: &impl Fn(&str) -> Option<String>,
) -> Result<Vec<(String, String)>, OtlpConfigError> {
    let variable = "OTEL_EXPORTER_OTLP_HEADERS";
    let Some(raw) = nonempty(env(variable)) else {
        return Ok(Vec::new());
    };
    raw.split(',')
        .map(|entry| {
            let Some((key, value)) = entry.split_once('=') else {
                return Err(OtlpConfigError::InvalidHeaders {
                    variable,
                    value: entry.to_owned(),
                });
            };
            if key.trim().is_empty() || value.trim().is_empty() {
                return Err(OtlpConfigError::InvalidHeaders {
                    variable,
                    value: entry.to_owned(),
                });
            }
            Ok((key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

fn validate_nonempty_optional(
    env: &impl Fn(&str) -> Option<String>,
    variable: &'static str,
) -> Result<(), OtlpConfigError> {
    if env(variable).is_some_and(|value| value.trim().is_empty()) {
        return Err(OtlpConfigError::EmptyValue(variable));
    }
    Ok(())
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn normalize_endpoint(value: String) -> String {
    value.trim().trim_end_matches('/').to_owned()
}

#[cfg(test)]
mod tests;
