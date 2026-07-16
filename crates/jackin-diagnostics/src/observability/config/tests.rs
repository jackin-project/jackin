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
        Err(OtlpConfigError::ConflictingSampler)
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
fn invalid_scalar_configuration_never_echoes_values() {
    let endpoint = ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4317");
    for (variable, secret) in [
        ("OTEL_EXPORTER_OTLP_PROTOCOL", "secret-protocol"),
        ("OTEL_TRACES_SAMPLER", "secret-sampler"),
        ("OTEL_EXPORTER_OTLP_COMPRESSION", "secret-compression"),
        ("OTEL_EXPORTER_OTLP_TIMEOUT", "secret-timeout"),
    ] {
        let oversized = format!("{secret}-{}", "x".repeat(16_384));
        let values = HashMap::from([
            (endpoint.0, endpoint.1.to_owned()),
            (variable, oversized.clone()),
        ]);
        let error = resolve_otlp_config(&|key| values.get(key).cloned())
            .expect_err("invalid configuration must fail");
        let rendered = error.to_string();
        assert!(rendered.contains(variable), "{rendered}");
        assert!(!rendered.contains(secret), "{rendered}");
        assert!(
            rendered.len() < 256,
            "unbounded error: {} bytes",
            rendered.len()
        );
    }
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
    let error = resolve(&[
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4317"),
        ("OTEL_EXPORTER_OTLP_HEADERS", "authorization=secret%0Avalue"),
    ])
    .expect_err("decoded newline must be rejected");
    assert!(!error.to_string().contains("secret"));
}

#[test]
fn signal_timeout_and_standard_header_tls_values_validate() {
    let config = resolve(&[
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://collector:4317"),
        ("OTEL_EXPORTER_OTLP_TIMEOUT", "4000"),
        ("OTEL_EXPORTER_OTLP_LOGS_TIMEOUT", "250"),
        ("OTEL_EXPORTER_OTLP_HEADERS", "authorization=generic"),
        (
            "OTEL_EXPORTER_OTLP_METRICS_HEADERS",
            "authorization=metrics",
        ),
        ("OTEL_EXPORTER_OTLP_CERTIFICATE", "/generic-ca.pem"),
        ("OTEL_EXPORTER_OTLP_TRACES_CERTIFICATE", "/trace-ca.pem"),
    ])
    .expect("valid")
    .expect("enabled");
    assert_eq!(config.traces_timeout, Duration::from_secs(4));
    assert_eq!(config.logs_timeout, Duration::from_millis(250));
    assert_eq!(
        config.traces_tls.certificate.as_deref(),
        Some("/trace-ca.pem")
    );
    assert_eq!(
        config.logs_tls.certificate.as_deref(),
        Some("/generic-ca.pem")
    );
}

#[test]
fn incomplete_client_identity_is_rejected_without_echoing_secret_paths() {
    let error = resolve(&[
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://collector:4317"),
        ("OTEL_EXPORTER_OTLP_CLIENT_KEY", "/secret/client.key"),
    ])
    .expect_err("client identity must be paired");
    assert_eq!(error, OtlpConfigError::IncompleteClientIdentity("traces"));
    assert!(!error.to_string().contains("/secret/client.key"));
}

#[test]
fn endpoints_reject_credentials_without_echoing_them() {
    let error = resolve(&[(
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "https://operator:super-secret@collector:4317/private",
    )])
    .expect_err("embedded endpoint credentials must fail");
    assert_eq!(error, OtlpConfigError::InvalidEndpoint("traces"));
    assert!(!error.to_string().contains("operator"));
    assert!(!error.to_string().contains("super-secret"));
}

#[test]
fn invalid_resource_attributes_do_not_echo_values() {
    let error = resolve(&[
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "https://collector:4317"),
        (
            "OTEL_RESOURCE_ATTRIBUTES",
            "authorization=super-secret,broken",
        ),
    ])
    .expect_err("invalid resource attributes must fail");
    assert_eq!(error, OtlpConfigError::InvalidResourceAttribute);
    assert!(!error.to_string().contains("super-secret"));
}
