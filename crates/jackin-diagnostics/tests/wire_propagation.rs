// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};

#[test]
fn conformance_wire_preserves_remote_parent_and_detached_link() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let testbed = runtime.block_on(async { jackin_otlp_testbed::Testbed::start() })?;
    let runtime_guard = runtime.enter();
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::HOST_ONE_SHOT,
    )?;

    let trace_id = TraceId::from_hex("4bf92f3577b34da6a3ce929d0e0e4736")?;
    let parent_span_id = SpanId::from_hex("00f067aa0ba902b7")?;
    let remote = SpanContext::new(
        trace_id,
        parent_span_id,
        TraceFlags::SAMPLED,
        true,
        TraceState::default(),
    );
    jackin_telemetry::operation_with_remote_parent(
        &jackin_telemetry::operation::RPC_SERVER,
        &[],
        &remote,
    )
    .map_err(|error| anyhow::anyhow!("server operation rejected: {error:?}"))?
    .complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);

    let job = jackin_telemetry::spawn::spawn_prewarm_job(
        jackin_telemetry::schema::enums::JobType::ImagePrewarm,
        async {},
    );
    drop(runtime_guard);
    runtime.block_on(job)?;
    let runtime_guard = runtime.enter();
    jackin_diagnostics::flush_wire_test_export()?;
    drop(runtime_guard);

    let spans = testbed.spans();
    let server = spans
        .iter()
        .find(|span| span.name == "rpc.server")
        .ok_or_else(|| anyhow::anyhow!("rpc.server missing"))?;
    assert_eq!(server.trace_id, trace_id.to_bytes());
    assert_eq!(server.parent_span_id, parent_span_id.to_bytes());
    assert_eq!(server.kind, 2, "rpc.server must use SERVER span kind");

    let producer = spans
        .iter()
        .find(|span| span.name == "prewarm.schedule")
        .ok_or_else(|| anyhow::anyhow!("prewarm.schedule missing"))?;
    let consumer = spans
        .iter()
        .find(|span| span.name == "prewarm.attempt")
        .ok_or_else(|| anyhow::anyhow!("prewarm.attempt missing"))?;
    assert_eq!(
        string_attribute(producer, "job.id"),
        string_attribute(consumer, "job.id")
    );
    assert!(
        consumer
            .links
            .iter()
            .any(|link| { link.trace_id == producer.trace_id && link.span_id == producer.span_id })
    );

    jackin_diagnostics::shutdown_capsule_tracing();
    Ok(())
}

fn string_attribute<'a>(
    span: &'a opentelemetry_proto::tonic::trace::v1::Span,
    key: &str,
) -> Option<&'a str> {
    span.attributes
        .iter()
        .find(|attribute| attribute.key == key)
        .and_then(|attribute| attribute.value.as_ref())
        .and_then(|value| value.value.as_ref())
        .and_then(|value| match value {
            opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(value) => {
                Some(value.as_str())
            }
            _ => None,
        })
}
