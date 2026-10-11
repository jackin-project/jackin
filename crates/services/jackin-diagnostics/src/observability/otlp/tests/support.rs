// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn run_telemetry_shutdown_scenario() {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    // The diagnostics conformance suite keeps a process-global meter installed.
    // Exercise provider ownership in a fresh test process so this scenario can
    // prove replacement behavior without racing that shared test rig.
    let first_provider = SdkMeterProvider::builder().build();
    let first_installation = jackin_telemetry::install(&first_provider.meter("pending-lease"))
        .expect("first provider-bound meter installation");
    let meter_installation = std::sync::Arc::new(std::sync::Mutex::new(first_installation));
    meter_installation
        .lock()
        .expect("meter installation lock")
        .detach_before(std::time::Instant::now() + std::time::Duration::from_secs(1))
        .expect("detach pending lease");

    let second_provider = SdkMeterProvider::builder().build();
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
    let task = super::FlushTask::spawn(
        move || {
            release_rx.recv().expect("release pending worker");
            Ok(())
        },
        Some(std::sync::Arc::clone(&meter_installation)),
    );
    assert_eq!(
        task.finish_before(std::time::Instant::now() + std::time::Duration::from_millis(20)),
        Err("telemetry flush budget exhausted".to_owned())
    );
    drop(meter_installation);
    assert!(
        jackin_telemetry::install(&second_provider.meter("blocked-by-pending-lease")).is_err(),
        "timed-out worker released the meter generation"
    );
    release_tx.send(()).expect("release pending worker");
    let reap_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    let second_installation = loop {
        super::reap_flush_workers();
        if let Ok(installation) =
            jackin_telemetry::install(&second_provider.meter("after-pending-lease"))
        {
            break installation;
        }
        assert!(
            std::time::Instant::now() < reap_deadline,
            "pending worker did not release its meter lease"
        );
        std::thread::yield_now();
    };
    drop(second_installation);

    let tracer = opentelemetry_sdk::trace::SdkTracerProvider::builder().build();
    let logger = opentelemetry_sdk::logs::SdkLoggerProvider::builder().build();
    let meter_exporter = InMemoryMetricExporter::default();
    let meter = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(meter_exporter.clone()).build())
        .build();
    let meter_installation = jackin_telemetry::install(&meter.meter("shutdown-order"))
        .expect("provider-bound meter installation");
    let generation = super::super::health::set_active_signals();
    *super::PROVIDERS.lock().expect("provider lock") = Some(super::OtlpProviders {
        tracer,
        logger,
        meter,
        generation,
        meter_installation: std::sync::Arc::new(std::sync::Mutex::new(meter_installation)),
    });
    super::SHUTDOWN_ORDER.lock().expect("order lock").clear();

    let (late_writer_tx, late_writer_rx) = std::sync::mpsc::sync_channel(0);
    let late_writer = std::thread::spawn(move || {
        late_writer_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("late writer release");
        jackin_telemetry::counter(&jackin_telemetry::metric::TELEMETRY_VALIDATE)
            .add(1, &[])
            .expect("late write after detach is a no-op");
    });
    let shutdown = std::thread::spawn(super::super::super::shutdown_capsule_tracing);
    let order_deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while super::SHUTDOWN_ORDER.lock().expect("order lock").first() != Some(&"detach.meter") {
        assert!(
            std::time::Instant::now() < order_deadline,
            "shutdown did not detach the meter before its deadline"
        );
        std::thread::yield_now();
    }
    late_writer_tx.send(()).expect("release late writer");
    late_writer.join().expect("late writer join");
    shutdown.join().expect("provider shutdown join");
    assert_eq!(
        *super::SHUTDOWN_ORDER.lock().expect("order lock"),
        [
            "detach.meter",
            "flush.tracer",
            "flush.logger",
            "flush.meter",
            "tracer",
            "logger",
            "meter"
        ]
    );
    let exported = meter_exporter
        .get_finished_metrics()
        .expect("metric export");
    assert!(
        !exported
            .iter()
            .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
            .any(|metric| metric.name() == jackin_telemetry::metric::TELEMETRY_VALIDATE.name())
    );
}

pub(super) fn log_attribute<'a>(
    record: &'a opentelemetry_sdk::logs::SdkLogRecord,
    name: &str,
) -> Option<&'a opentelemetry::logs::AnyValue> {
    record
        .attributes_iter()
        .find_map(|(key, value)| (key.as_str() == name).then_some(value))
}

pub(super) fn span_attribute<'a>(
    span: &'a opentelemetry_sdk::trace::SpanData,
    name: &str,
) -> Option<std::borrow::Cow<'a, str>> {
    span.attributes
        .iter()
        .find(|attribute| attribute.key.as_str() == name)
        .map(|attribute| attribute.value.as_str())
}

pub(super) fn cli_command_test_attrs() -> [jackin_telemetry::Attr<'static>; 2] {
    [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CLI_COMMAND_NAME,
            value: jackin_telemetry::Value::Str("diagnostics"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
            value: jackin_telemetry::Value::Str("invocation-test"),
        },
    ]
}

pub(super) fn emit_severity_matrix() {
    let outcome = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::OUTCOME,
        value: jackin_telemetry::Value::Str("success"),
    }];
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::TIMING_STARTED,
        jackin_telemetry::FieldSet::new(&outcome, None),
    )
    .unwrap();
    let widget = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::APP_WIDGET_ID,
            value: jackin_telemetry::Value::Str("matrix.widget"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::APP_WIDGET_NAME,
            value: jackin_telemetry::Value::Str("matrix.widget"),
        },
    ];
    jackin_telemetry::emit_event(
        &jackin_telemetry::event::UI_WIDGET_FOCUSED,
        jackin_telemetry::FieldSet::new(&widget, None),
    )
    .unwrap();
    for def in [
        &jackin_telemetry::event::PTY_SPAWN,
        &jackin_telemetry::event::APP_JANK,
        &jackin_telemetry::event::APP_CRASH,
    ] {
        jackin_telemetry::emit_event(def, jackin_telemetry::FieldSet::default()).unwrap();
    }
}

pub(super) fn assert_raw_metric_batch_rejected(
    reason: jackin_telemetry::Rejection,
    record: impl FnOnce(&opentelemetry::metrics::Meter),
) {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader, SdkMeterProvider};

    let _lock = crate::DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let before = jackin_telemetry::facade_health().by_signal_reason
        [jackin_telemetry::Signal::Metric as usize][reason as usize];
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_reader(
            PeriodicReader::builder(super::GovernedMetricExporter(exporter.clone())).build(),
        )
        .build();
    record(&provider.meter("jackin"));

    assert!(
        provider.force_flush().is_err(),
        "raw metric batch unexpectedly passed governance for {reason:?}"
    );
    assert!(exporter.get_finished_metrics().unwrap().is_empty());
    assert_eq!(
        jackin_telemetry::facade_health().by_signal_reason
            [jackin_telemetry::Signal::Metric as usize][reason as usize],
        before + 1,
        "raw batch did not move the exact metric rejection cell"
    );
}
