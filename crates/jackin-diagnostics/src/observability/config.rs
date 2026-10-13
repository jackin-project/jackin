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

const AUTH_VARS: [&str; 12] = [
    "OTEL_EXPORTER_OTLP_HEADERS",
    "OTEL_EXPORTER_OTLP_TRACES_HEADERS",
    "OTEL_EXPORTER_OTLP_LOGS_HEADERS",
    "OTEL_EXPORTER_OTLP_METRICS_HEADERS",
    "OTEL_EXPORTER_OTLP_CLIENT_KEY",
    "OTEL_EXPORTER_OTLP_CLIENT_CERTIFICATE",
    "OTEL_EXPORTER_OTLP_TRACES_CLIENT_KEY",
    "OTEL_EXPORTER_OTLP_TRACES_CLIENT_CERTIFICATE",
    "OTEL_EXPORTER_OTLP_LOGS_CLIENT_KEY",
    "OTEL_EXPORTER_OTLP_LOGS_CLIENT_CERTIFICATE",
    "OTEL_EXPORTER_OTLP_METRICS_CLIENT_KEY",
    "OTEL_EXPORTER_OTLP_METRICS_CLIENT_CERTIFICATE",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OtlpConfig {
    pub traces_endpoint: String,
    pub logs_endpoint: String,
    pub metrics_endpoint: String,
    pub traces_timeout: Duration,
    pub logs_timeout: Duration,
    pub metrics_timeout: Duration,
    pub traces_tls: TlsConfig,
    pub logs_tls: TlsConfig,
    pub metrics_tls: TlsConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct TlsConfig {
    pub certificate: Option<String>,
    pub client_key: Option<String>,
    pub client_certificate: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OtlpConfigError {
    MissingSignalEndpoint(&'static str),
    UnsupportedProtocol { variable: &'static str },
    ConflictingSampler,
    UnsupportedCompression { variable: &'static str },
    InvalidTimeout { variable: &'static str },
    InvalidHeaders { variable: &'static str },
    InvalidResourceAttribute,
    InvalidEndpoint(&'static str),
    EmptyValue(&'static str),
    IncompleteClientIdentity(&'static str),
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
            Self::UnsupportedProtocol { variable } => write!(
                f,
                "{variable} is unsupported; jackin exports OTLP over grpc only"
            ),
            Self::ConflictingSampler => write!(
                f,
                "OTEL_TRACES_SAMPLER conflicts with required parentbased_always_on"
            ),
            Self::UnsupportedCompression { variable } => {
                write!(f, "{variable} is unsupported; expected gzip")
            }
            Self::InvalidTimeout { variable } => {
                write!(
                    f,
                    "{variable} must be a positive integer millisecond timeout"
                )
            }
            Self::InvalidHeaders { variable } => {
                write!(f, "{variable} contains an invalid OTLP header")
            }
            Self::InvalidResourceAttribute => {
                f.write_str("OTEL_RESOURCE_ATTRIBUTES contains an invalid entry")
            }
            Self::InvalidEndpoint(signal) => {
                write!(
                    f,
                    "OTLP {signal} endpoint must be an http(s) authority without credentials"
                )
            }
            Self::EmptyValue(variable) => write!(f, "{variable} must not be empty when set"),
            Self::IncompleteClientIdentity(signal) => write!(
                f,
                "OTLP {signal} client certificate and client key must be configured together"
            ),
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
        "OTEL_EXPORTER_OTLP_TRACES_CLIENT_KEY",
        "OTEL_EXPORTER_OTLP_TRACES_CLIENT_CERTIFICATE",
        "OTEL_EXPORTER_OTLP_LOGS_CLIENT_KEY",
        "OTEL_EXPORTER_OTLP_LOGS_CLIENT_CERTIFICATE",
        "OTEL_EXPORTER_OTLP_METRICS_CLIENT_KEY",
        "OTEL_EXPORTER_OTLP_METRICS_CLIENT_CERTIFICATE",
    ] {
        validate_nonempty_optional(env, variable)?;
    }

    if let Some(attributes) = env("OTEL_RESOURCE_ATTRIBUTES") {
        for entry in attributes
            .split(',')
            .filter(|entry| !entry.trim().is_empty())
        {
            let Some((key, value)) = entry.split_once('=') else {
                return Err(OtlpConfigError::InvalidResourceAttribute);
            };
            if key.trim().is_empty() || value.trim().is_empty() {
                return Err(OtlpConfigError::InvalidResourceAttribute);
            }
        }
    }

    build_otlp_config(env, traces, logs, metrics).map(Some)
}

fn build_otlp_config(
    env: &impl Fn(&str) -> Option<String>,
    traces: Option<String>,
    logs: Option<String>,
    metrics: Option<String>,
) -> Result<OtlpConfig, OtlpConfigError> {
    // The pinned tonic exporter consumes the standard generic/per-signal header
    // variables directly. Validate them here before any provider is built.
    parse_headers(env, "OTEL_EXPORTER_OTLP_TRACES_HEADERS")?;
    parse_headers(env, "OTEL_EXPORTER_OTLP_LOGS_HEADERS")?;
    parse_headers(env, "OTEL_EXPORTER_OTLP_METRICS_HEADERS")?;
    Ok(OtlpConfig {
        traces_endpoint: normalize_endpoint(
            traces.ok_or(OtlpConfigError::MissingSignalEndpoint("traces"))?,
            "traces",
        )?,
        logs_endpoint: normalize_endpoint(
            logs.ok_or(OtlpConfigError::MissingSignalEndpoint("logs"))?,
            "logs",
        )?,
        metrics_endpoint: normalize_endpoint(
            metrics.ok_or(OtlpConfigError::MissingSignalEndpoint("metrics"))?,
            "metrics",
        )?,
        traces_timeout: parse_timeout(env, "OTEL_EXPORTER_OTLP_TRACES_TIMEOUT")?,
        logs_timeout: parse_timeout(env, "OTEL_EXPORTER_OTLP_LOGS_TIMEOUT")?,
        metrics_timeout: parse_timeout(env, "OTEL_EXPORTER_OTLP_METRICS_TIMEOUT")?,
        traces_tls: tls_config(env, "TRACES", "traces")?,
        logs_tls: tls_config(env, "LOGS", "logs")?,
        metrics_tls: tls_config(env, "METRICS", "metrics")?,
    })
}

fn tls_config(
    env: &impl Fn(&str) -> Option<String>,
    signal_variable: &str,
    signal_name: &'static str,
) -> Result<TlsConfig, OtlpConfigError> {
    let signal = |suffix: &str| {
        nonempty(env(&format!(
            "OTEL_EXPORTER_OTLP_{signal_variable}_{suffix}"
        )))
    };
    let generic = |suffix: &str| nonempty(env(&format!("OTEL_EXPORTER_OTLP_{suffix}")));
    let config = TlsConfig {
        certificate: signal("CERTIFICATE").or_else(|| generic("CERTIFICATE")),
        client_key: signal("CLIENT_KEY").or_else(|| generic("CLIENT_KEY")),
        client_certificate: signal("CLIENT_CERTIFICATE").or_else(|| generic("CLIENT_CERTIFICATE")),
    };
    if config.client_key.is_some() != config.client_certificate.is_some() {
        return Err(OtlpConfigError::IncompleteClientIdentity(signal_name));
    }
    Ok(config)
}

pub(super) fn any_endpoint_configured(env: &impl Fn(&str) -> Option<String>) -> bool {
    ENDPOINT_VARS
        .iter()
        .any(|variable| nonempty(env(variable)).is_some())
}

pub(super) fn any_auth_configured(env: &impl Fn(&str) -> Option<String>) -> bool {
    AUTH_VARS
        .iter()
        .any(|variable| nonempty(env(variable)).is_some())
}

fn validate_protocols(env: &impl Fn(&str) -> Option<String>) -> Result<(), OtlpConfigError> {
    for variable in PROTOCOL_VARS {
        if let Some(value) = nonempty(env(variable))
            && value.trim() != "grpc"
        {
            return Err(OtlpConfigError::UnsupportedProtocol { variable });
        }
    }
    Ok(())
}

fn validate_sampler(env: &impl Fn(&str) -> Option<String>) -> Result<(), OtlpConfigError> {
    if let Some(value) = nonempty(env("OTEL_TRACES_SAMPLER"))
        && value.trim() != "parentbased_always_on"
    {
        return Err(OtlpConfigError::ConflictingSampler);
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
            return Err(OtlpConfigError::UnsupportedCompression { variable });
        }
    }
    Ok(())
}

fn parse_timeout(
    env: &impl Fn(&str) -> Option<String>,
    signal_variable: &'static str,
) -> Result<Duration, OtlpConfigError> {
    let (variable, value) = if let Some(value) = nonempty(env(signal_variable)) {
        (signal_variable, value)
    } else if let Some(value) = nonempty(env("OTEL_EXPORTER_OTLP_TIMEOUT")) {
        ("OTEL_EXPORTER_OTLP_TIMEOUT", value)
    } else {
        return Ok(Duration::from_secs(5));
    };
    let millis = value
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|millis| *millis > 0)
        .ok_or(OtlpConfigError::InvalidTimeout { variable })?;
    Ok(Duration::from_millis(millis.min(5_000)))
}

fn parse_headers(
    env: &impl Fn(&str) -> Option<String>,
    signal_variable: &'static str,
) -> Result<(), OtlpConfigError> {
    let (variable, raw) = if let Some(raw) = nonempty(env(signal_variable)) {
        (signal_variable, raw)
    } else if let Some(raw) = nonempty(env("OTEL_EXPORTER_OTLP_HEADERS")) {
        ("OTEL_EXPORTER_OTLP_HEADERS", raw)
    } else {
        return Ok(());
    };
    raw.split(',').try_for_each(|entry| {
        let Some((key, value)) = entry.split_once('=') else {
            return Err(OtlpConfigError::InvalidHeaders { variable });
        };
        if key.trim().is_empty() || value.trim().is_empty() {
            return Err(OtlpConfigError::InvalidHeaders { variable });
        }
        let decoded = decode_header_value(value.trim())
            .ok_or(OtlpConfigError::InvalidHeaders { variable })?;
        key.trim()
            .parse::<tonic::metadata::MetadataKey<tonic::metadata::Ascii>>()
            .map_err(|_| OtlpConfigError::InvalidHeaders { variable })?;
        decoded
            .parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>()
            .map_err(|_| OtlpConfigError::InvalidHeaders { variable })?;
        Ok(())
    })?;
    Ok(())
}

fn decode_header_value(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let text = std::str::from_utf8(hex).ok()?;
            decoded.push(u8::from_str_radix(text, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
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

pub(super) fn normalize_endpoint(
    value: String,
    signal: &'static str,
) -> Result<String, OtlpConfigError> {
    let endpoint =
        url::Url::parse(value.trim()).map_err(|_| OtlpConfigError::InvalidEndpoint(signal))?;
    if !matches!(endpoint.scheme(), "http" | "https")
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(OtlpConfigError::InvalidEndpoint(signal));
    }
    Ok(endpoint.as_str().trim_end_matches('/').to_owned())
}

#[cfg(test)]
mod tests;
