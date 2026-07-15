// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use super::*;

fn resolve(values: &[(&str, &str)]) -> Result<Option<OtlpConfig>, OtlpConfigError> {
    let values: HashMap<&str, &str> = values.iter().copied().collect();
    resolve_otlp_config(&|key| values.get(key).map(|value| (*value).to_owned()))
}

#[test]
fn absent_endpoint_is_disabled_without_runtime_configuration() {
    assert_eq!(resolve(&[]), Ok(None));
}

#[test]
fn sdk_disabled_wins_over_invalid_configuration() {
    assert_eq!(
        resolve(&[
            ("OTEL_SDK_DISABLED", "TRUE"),
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4317"),
            ("OTEL_EXPORTER_OTLP_PROTOCOL", "http/protobuf"),
        ]),
        Ok(None)
    );
}

#[test]
fn signal_endpoints_override_base_and_all_are_required() {
    let config = resolve(&[
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://base:4317/"),
        ("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT", "http://logs:4317"),
    ])
    .expect("valid")
    .expect("enabled");
    assert_eq!(config.traces_endpoint, "http://base:4317");
    assert_eq!(config.logs_endpoint, "http://logs:4317");
    assert_eq!(config.metrics_endpoint, "http://base:4317");

    assert_eq!(
        resolve(&[("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT", "http://traces:4317",)]),
        Err(OtlpConfigError::MissingSignalEndpoint("logs"))
    );
}

#[test]
fn protocol_sampler_compression_and_timeout_are_typed() {
    let endpoint = ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4317");
    assert!(matches!(
        resolve(&[endpoint, ("OTEL_EXPORTER_OTLP_PROTOCOL", "http/protobuf")]),
        Err(OtlpConfigError::UnsupportedProtocol { .. })
    ));
    assert_eq!(
        resolve(&[endpoint, ("OTEL_TRACES_SAMPLER", "always_off")]),
        Err(OtlpConfigError::ConflictingSampler("always_off".to_owned()))
    );
    assert!(matches!(
        resolve(&[endpoint, ("OTEL_EXPORTER_OTLP_COMPRESSION", "none")]),
        Err(OtlpConfigError::UnsupportedCompression { .. })
    ));
    assert!(matches!(
        resolve(&[endpoint, ("OTEL_EXPORTER_OTLP_TIMEOUT", "zero")]),
        Err(OtlpConfigError::InvalidTimeout { .. })
    ));
}

#[test]
fn headers_are_parsed_before_provider_start() {
    resolve(&[
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4317"),
        (
            "OTEL_EXPORTER_OTLP_HEADERS",
            "authorization=Bearer redacted,x-tenant=dev",
        ),
    ])
    .expect("valid")
    .expect("enabled");
    assert!(matches!(
        resolve(&[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4317"),
            ("OTEL_EXPORTER_OTLP_HEADERS", "missing-value"),
        ]),
        Err(OtlpConfigError::InvalidHeaders { .. })
    ));
}
