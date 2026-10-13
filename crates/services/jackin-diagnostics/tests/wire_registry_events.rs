// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_telemetry::schema::{RequirementLevel, ValueType};

static ARRAY_VALUE: &[&str] = &["proof"];
const REGISTRY_WIRE_CHILD: &str = "JACKIN_REGISTRY_WIRE_CHILD";
const REGISTRY_WIRE_TEST: &str =
    "conformance_wire_every_registered_event_arrives_once_with_native_shape";

fn dispatch_trace_level_child() -> anyhow::Result<bool> {
    if std::env::var_os(REGISTRY_WIRE_CHILD).is_some() {
        return Ok(false);
    }
    let status = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", REGISTRY_WIRE_TEST, "--nocapture"])
        .env(REGISTRY_WIRE_CHILD, "1")
        .env("JACKIN_TELEMETRY_LEVEL", "trace")
        .status()?;
    anyhow::ensure!(status.success(), "isolated registry wire test failed");
    Ok(true)
}

fn canonical_severity_number(name: &str) -> i32 {
    let Some(severity) = jackin_telemetry::event::canonical_severity(name) else {
        unreachable!("generated event {name} is missing its canonical severity");
    };
    match severity {
        jackin_telemetry::event::Severity::Trace => 1,
        jackin_telemetry::event::Severity::Debug => 5,
        jackin_telemetry::event::Severity::Info => 9,
        jackin_telemetry::event::Severity::Warn => 13,
        jackin_telemetry::event::Severity::Error => 17,
    }
}

fn required_fields(
    metadata: &jackin_telemetry::schema::EventMetadata,
) -> Vec<jackin_telemetry::Attr<'static>> {
    metadata
        .attributes
        .iter()
        .filter(|attribute| attribute.requirement == RequirementLevel::Required)
        .map(|attribute| jackin_telemetry::Attr {
            key: attribute.name,
            value: match attribute.value_type {
                ValueType::String => jackin_telemetry::Value::Str(
                    attribute.allowed_values.first().copied().unwrap_or("proof"),
                ),
                ValueType::Boolean => jackin_telemetry::Value::Bool(true),
                ValueType::Integer => jackin_telemetry::Value::I64(1),
                ValueType::Double => jackin_telemetry::Value::F64(1.0),
                ValueType::StringArray => jackin_telemetry::Value::StrArray(ARRAY_VALUE),
            },
        })
        .collect()
}

#[test]
fn conformance_wire_every_registered_event_arrives_once_with_native_shape() -> anyhow::Result<()> {
    if dispatch_trace_level_child()? {
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;

    let root =
        jackin_telemetry::root_operation(&jackin_telemetry::operation::TELEMETRY_VALIDATE, &[])
            .map_err(|reason| anyhow::anyhow!("validation root rejected: {reason:?}"))?;
    {
        let root_span = root.span().clone();
        let _entered = root_span.enter();
        for name in jackin_telemetry::schema::events::ALL {
            let definition = jackin_telemetry::event::definition(name)
                .expect("generated event has facade definition");
            let metadata = jackin_telemetry::schema::events::definition(name)
                .expect("generated event has schema metadata");
            let attrs = required_fields(metadata);
            jackin_telemetry::emit_event(definition, jackin_telemetry::FieldSet::new(&attrs, None))
                .unwrap_or_else(|reason| panic!("{name} fixture rejected: {reason:?}"));
        }
    }
    root.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    jackin_diagnostics::flush_wire_test_export()?;

    let expected_count = jackin_telemetry::schema::events::ALL.len();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let records = runtime.block_on(async {
        loop {
            let records = testbed.log_records();
            if records.len() == expected_count {
                break records;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "registry event wire count was {}, expected {expected_count}; received={:?}",
                records.len(),
                records
                    .iter()
                    .map(|record| record.event_name.as_str())
                    .collect::<Vec<_>>()
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    });
    for name in jackin_telemetry::schema::events::ALL {
        let matching = records
            .iter()
            .filter(|record| record.event_name == *name)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "{name} delivery count");
        assert_eq!(
            matching[0].severity_number,
            canonical_severity_number(name),
            "{name} severity"
        );
        assert_eq!(matching[0].trace_id.len(), 16, "{name} trace id");
        assert_eq!(matching[0].span_id.len(), 8, "{name} span id");
    }
    let spans = testbed.spans();
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == "telemetry.validate")
            .count(),
        1
    );
    assert!(spans.iter().all(|span| span.events.is_empty()));
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    assert_eq!(
        testbed.prohibited_value_violations(&[
            "wire-private-event-body",
            "wire-private-model-name",
            "wire-private-agent-codename",
            "wire-private-config-value",
        ]),
        Vec::<String>::new()
    );
    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}
