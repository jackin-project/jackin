use super::*;
use crate::{event::Value, schema::attrs};

#[test]
fn cardinality_rejects_the_257th_set_without_eviction() {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    install(&provider.meter("cardinality-test")).expect("test meter installation");
    let before = crate::facade_health().cardinality;
    let mut series = Vec::new();
    for command in schema::enums::CliCommandName::ALL {
        for outcome in schema::enums::OutcomeValue::ALL {
            for error in schema::enums::ErrorType::ALL {
                series.push((command.as_str(), outcome.as_str(), error.as_str()));
                if series.len() > limits::MAX_CARDINALITY {
                    break;
                }
            }
            if series.len() > limits::MAX_CARDINALITY {
                break;
            }
        }
        if series.len() > limits::MAX_CARDINALITY {
            break;
        }
    }
    for (command, outcome, error) in &series[..limits::MAX_CARDINALITY] {
        let attrs = [
            Attr {
                key: attrs::CLI_COMMAND_NAME,
                value: Value::Str(command),
            },
            Attr {
                key: attrs::OUTCOME,
                value: Value::Str(outcome),
            },
            Attr {
                key: attrs::std_attrs::ERROR_TYPE,
                value: Value::Str(error),
            },
        ];
        histogram(&CLI_DURATION).record(1.0, &attrs).unwrap();
    }
    let existing = series[0];
    let existing_attrs = [
        Attr {
            key: attrs::CLI_COMMAND_NAME,
            value: Value::Str(existing.0),
        },
        Attr {
            key: attrs::OUTCOME,
            value: Value::Str(existing.1),
        },
        Attr {
            key: attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(existing.2),
        },
    ];
    histogram(&CLI_DURATION)
        .record(2.0, &existing_attrs)
        .expect("an existing exact series remains accepted at the cap");
    let overflow = series[limits::MAX_CARDINALITY];
    let overflow_attrs = [
        Attr {
            key: attrs::CLI_COMMAND_NAME,
            value: Value::Str(overflow.0),
        },
        Attr {
            key: attrs::OUTCOME,
            value: Value::Str(overflow.1),
        },
        Attr {
            key: attrs::std_attrs::ERROR_TYPE,
            value: Value::Str(overflow.2),
        },
    ];
    assert_eq!(
        histogram(&CLI_DURATION).record(1.0, &overflow_attrs),
        Err(Rejection::Cardinality)
    );
    assert_eq!(crate::facade_health().cardinality, before + 1);
    provider.force_flush().expect("metric flush");
    let point_count = exporter
        .get_finished_metrics()
        .expect("metric export")
        .iter()
        .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        .find(|metric| metric.name() == CLI_DURATION.name())
        .and_then(|metric| match metric.data() {
            opentelemetry_sdk::metrics::data::AggregatedMetrics::F64(
                opentelemetry_sdk::metrics::data::MetricData::Histogram(histogram),
            ) => Some(histogram.data_points().count()),
            _ => None,
        })
        .expect("exported governed histogram");
    assert_eq!(point_count, limits::MAX_CARDINALITY);
}

#[test]
fn series_identity_is_order_independent_and_duplicates_reject() {
    let first = [
        Attr {
            key: attrs::LAUNCH_STAGE_NAME,
            value: Value::Str("network"),
        },
        Attr {
            key: attrs::OUTCOME,
            value: Value::Str("success"),
        },
    ];
    let reversed = [first[1], first[0]];
    assert_eq!(series_identity(&first), series_identity(&reversed));
    assert_eq!(
        validate_attributes(&LAUNCH_STAGE_DURATION, &[first[0], first[0]]),
        Err(Rejection::InvalidValue)
    );
}

#[test]
fn prewarm_metrics_use_only_bounded_job_and_outcome_dimensions() {
    let job = Attr {
        key: attrs::JOB_TYPE,
        value: Value::Str(schema::enums::JobType::ImagePrewarm.as_str()),
    };
    let outcome = Attr {
        key: attrs::OUTCOME,
        value: Value::Str(schema::enums::OutcomeValue::Failure.as_str()),
    };
    let error = Attr {
        key: attrs::std_attrs::ERROR_TYPE,
        value: Value::Str(schema::enums::ErrorType::LaunchFailed.as_str()),
    };

    validate_attributes(&PREWARM_JOBS, &[job]).unwrap();
    validate_attributes(&PREWARM_ACTIVE, &[job]).unwrap();
    validate_attributes(&PREWARM_DURATION, &[job, outcome, error]).unwrap();
    assert_eq!(
        validate_attributes(&PREWARM_DURATION, &[job]),
        Err(Rejection::InvalidValue)
    );
}

#[test]
fn standard_token_usage_requires_only_bounded_semantic_dimensions() {
    let dimensions = GEN_AI_CLIENT_TOKEN_USAGE.dimensions();
    assert_eq!(
        dimensions
            .iter()
            .map(|requirement| requirement.name)
            .collect::<Vec<_>>(),
        [
            attrs::GEN_AI_OPERATION_NAME,
            attrs::GEN_AI_PROVIDER_NAME,
            attrs::GEN_AI_TOKEN_TYPE,
        ]
    );
    assert!(
        dimensions
            .iter()
            .all(|requirement| requirement.requirement == schema::RequirementLevel::Required)
    );
    assert_eq!(GEN_AI_CLIENT_TOKEN_USAGE.unit(), "{token}");
    assert_eq!(
        GEN_AI_CLIENT_TOKEN_USAGE.boundaries(),
        [
            1.0, 4.0, 16.0, 64.0, 256.0, 1024.0, 4096.0, 16384.0, 65536.0, 262144.0
        ]
    );
}

#[test]
fn correlation_identities_are_never_metric_dimensions() {
    for key in [
        attrs::CLI_INVOCATION_ID,
        attrs::std_attrs::SESSION_ID,
        attrs::JOB_ID,
        attrs::UI_SCREEN_VISIT_ID,
        attrs::std_attrs::GEN_AI_CONVERSATION_ID,
    ] {
        assert_eq!(
            counter(&TELEMETRY_VALIDATE).add(
                1,
                &[Attr {
                    key,
                    value: Value::Str("opaque-correlation"),
                }],
            ),
            Err(Rejection::Cardinality),
            "identity key {key} must fail before disabled-meter short circuit"
        );
    }
}

#[test]
fn agent_state_metrics_require_the_governed_dimensions() {
    let attrs = [
        Attr {
            key: attrs::std_attrs::GEN_AI_AGENT_NAME,
            value: Value::Str("codex"),
        },
        Attr {
            key: attrs::AGENT_STATE,
            value: Value::Str("working"),
        },
        Attr {
            key: attrs::AGENT_STATUS_SOURCE,
            value: Value::Str("shell_integration"),
        },
        Attr {
            key: attrs::AGENT_STATUS_CONFIDENCE,
            value: Value::Str("strong"),
        },
    ];
    assert_eq!(
        validate_attributes(&AGENT_STATE_TRANSITIONS, &attrs),
        Ok(())
    );
    assert_eq!(
        validate_attributes(&AGENT_STATE_STUCK, &attrs[..3]),
        Err(Rejection::InvalidValue)
    );

    let mut unknown_agent = attrs;
    unknown_agent[0].value = Value::Str("unknown-agent");
    assert_eq!(
        validate_attributes(&AGENT_STATE_FLAPS, &unknown_agent),
        Err(Rejection::InvalidValue)
    );
}
