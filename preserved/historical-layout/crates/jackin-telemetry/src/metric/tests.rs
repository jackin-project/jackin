use super::*;
use crate::{event::Value, schema::attrs};

static METER_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn every_registered_metric_stream_enforces_its_own_cardinality_cap() {
    let mut streams = SeriesByInstrument::new();
    for definition in ALL {
        for value in 0..limits::MAX_CARDINALITY {
            let attrs = [Attr {
                key: attrs::CLI_COMMAND_NAME,
                value: Value::U64(value as u64),
            }];
            assert!(accept_series_in(
                &mut streams,
                definition.name,
                &attrs,
                &[0]
            ));
        }
        let existing = [Attr {
            key: attrs::CLI_COMMAND_NAME,
            value: Value::U64(0),
        }];
        assert!(accept_series_in(
            &mut streams,
            definition.name,
            &existing,
            &[0]
        ));
        let overflow = [Attr {
            key: attrs::CLI_COMMAND_NAME,
            value: Value::U64(limits::MAX_CARDINALITY as u64),
        }];
        assert!(
            !accept_series_in(&mut streams, definition.name, &overflow, &[0]),
            "{} accepted its 257th series",
            definition.name
        );
    }
    assert_eq!(streams.len(), ALL.len());
    assert!(
        streams
            .values()
            .all(|series| series.len() == limits::MAX_CARDINALITY)
    );
}

#[test]
fn cardinality_rejects_the_257th_set_without_eviction() {
    let _lock = METER_TEST_LOCK.lock().expect("meter test lock");
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    let _installation =
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
    let exported = exporter.get_finished_metrics().expect("metric export");
    let point_count = exported
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

    let health_points = exported
        .iter()
        .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        .find(|metric| metric.name() == TELEMETRY_REJECTIONS.name())
        .and_then(|metric| match metric.data() {
            opentelemetry_sdk::metrics::data::AggregatedMetrics::U64(
                opentelemetry_sdk::metrics::data::MetricData::Sum(sum),
            ) => Some(
                sum.data_points()
                    .map(|point| {
                        point
                            .attributes()
                            .map(|attribute| {
                                (
                                    attribute.key.as_str(),
                                    attribute.value.as_str().into_owned(),
                                )
                            })
                            .collect::<HashMap<_, _>>()
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .expect("exported rejection health counter");
    let reasons = [
        "unknown_name",
        "unknown_attribute",
        "invalid_value",
        "privacy",
        "cardinality",
        "size_limit",
    ];
    assert_eq!(
        health_points.len(),
        health::Signal::ALL.len() * reasons.len()
    );
    for signal in health::Signal::ALL {
        for reason in reasons {
            assert!(health_points.iter().any(|attributes| {
                attributes
                    .get(attrs::TELEMETRY_SIGNAL)
                    .is_some_and(|value| value == signal.as_str())
                    && attributes
                        .get(attrs::TELEMETRY_REJECTION_REASON)
                        .is_some_and(|value| value == reason)
            }));
        }
    }
}

#[test]
fn meter_installation_drop_releases_provider_and_series_state() {
    let _lock = METER_TEST_LOCK.lock().expect("meter test lock");
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::SdkMeterProvider;

    let first_provider = SdkMeterProvider::builder().build();
    let first_installation =
        install(&first_provider.meter("first-lifecycle")).expect("first meter installation");
    for value in 0..limits::MAX_CARDINALITY {
        let attrs = [Attr {
            key: attrs::CLI_COMMAND_NAME,
            value: Value::U64(value as u64),
        }];
        assert!(accept_series(TELEMETRY_VALIDATE.name(), &attrs));
    }
    drop(first_installation);

    let second_provider = SdkMeterProvider::builder().build();
    let _second_installation = install(&second_provider.meter("second-lifecycle"))
        .expect("second meter installation after first shutdown");
    let attrs = [Attr {
        key: attrs::CLI_COMMAND_NAME,
        value: Value::U64(limits::MAX_CARDINALITY as u64),
    }];
    assert!(accept_series(TELEMETRY_VALIDATE.name(), &attrs));
}

#[test]
fn meter_detach_waits_for_in_flight_facade_read_guard() {
    let _lock = METER_TEST_LOCK.lock().expect("meter test lock");
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::SdkMeterProvider;

    let provider = SdkMeterProvider::builder().build();
    let installation = install(&provider.meter("concurrent-detach"))
        .expect("concurrent detach meter installation");
    let in_flight = INSTRUMENTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        let mut installation = installation;
        installation
            .detach_inner(Instant::now() + std::time::Duration::from_secs(1), || {
                ready_tx.send(()).expect("detach worker ready");
            })
            .expect("detach after read guard release");
        done_tx.send(()).expect("detach worker done");
    });

    ready_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("detach worker reached the facade write");
    assert!(done_rx.try_recv().is_err(), "detach ignored the read guard");
    drop(in_flight);
    done_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("detach worker completed after read guard release");
    worker.join().expect("detach worker join");
    assert!(
        INSTRUMENTS
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none()
    );
}

#[test]
fn detached_facade_drops_late_write_before_metric_flush() {
    let _lock = METER_TEST_LOCK.lock().expect("meter test lock");
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter.clone()).build())
        .build();
    let mut installation =
        install(&provider.meter("late-write-fence")).expect("late-write fence meter installation");
    let writer_gate = std::sync::Arc::new(std::sync::Barrier::new(2));
    let writer_gate_clone = std::sync::Arc::clone(&writer_gate);
    let writer = std::thread::spawn(move || {
        writer_gate_clone.wait();
        counter(&TELEMETRY_VALIDATE).add(1, &[])
    });

    installation
        .detach_before(Instant::now() + std::time::Duration::from_secs(1))
        .expect("detach before metric flush");
    writer_gate.wait();
    writer
        .join()
        .expect("late metric writer join")
        .expect("detached facade write is a no-op");
    provider.force_flush().expect("metric flush");
    let exported = exporter.get_finished_metrics().expect("metric export");
    assert!(
        !exported
            .iter()
            .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
            .any(|metric| metric.name() == TELEMETRY_VALIDATE.name())
    );
}

#[test]
fn detached_meter_installation_keeps_generation_until_drop() {
    let _lock = METER_TEST_LOCK.lock().expect("meter test lock");
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::SdkMeterProvider;

    let first_provider = SdkMeterProvider::builder().build();
    let mut first_installation =
        install(&first_provider.meter("detached-generation")).expect("first meter installation");
    first_installation
        .detach_before(Instant::now() + std::time::Duration::from_secs(1))
        .expect("detach first generation");

    let second_provider = SdkMeterProvider::builder().build();
    assert!(matches!(
        install(&second_provider.meter("blocked-generation")),
        Err(MeterInstallError)
    ));

    drop(first_installation);
    let _second_installation = install(&second_provider.meter("next-generation"))
        .expect("next meter installation after retired lease drop");
}

#[test]
fn meter_detach_deadline_bounds_reader_fence() {
    let _lock = METER_TEST_LOCK.lock().expect("meter test lock");
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::SdkMeterProvider;

    let provider = SdkMeterProvider::builder().build();
    let mut installation =
        install(&provider.meter("bounded-detach")).expect("bounded detach meter installation");
    let in_flight = INSTRUMENTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let started = Instant::now();
    assert_eq!(
        installation.detach_before(started + std::time::Duration::from_millis(20)),
        Err(MeterDetachError::DeadlineExceeded)
    );
    assert!(
        started.elapsed() < std::time::Duration::from_millis(250),
        "reader fence exceeded its deadline: {:?}",
        started.elapsed()
    );
    drop(in_flight);
    installation
        .detach_before(Instant::now() + std::time::Duration::from_secs(1))
        .expect("detach after bounded reader fence");
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
