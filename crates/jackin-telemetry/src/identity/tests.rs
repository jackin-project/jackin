// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Barrier};

use opentelemetry::trace::TracerProvider as _;
use tracing_subscriber::prelude::*;

use super::*;

static TEST_SESSION_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn identity_values_are_uuid_unique_and_parseable() {
    let first = InvocationId::mint();
    let second = InvocationId::mint();
    assert_ne!(first, second);
    assert_eq!(InvocationId::parse(&first.to_string()).unwrap(), first);
}

#[test]
fn session_rejects_concurrent_owner_and_reattach_uses_last_ended() {
    let _serial = TEST_SESSION_LOCK.lock().unwrap();
    let first = SessionGuard::begin(SessionKind::Console).unwrap();
    let first_id = first.context().current;
    let barrier = Arc::new(Barrier::new(2));
    let worker_barrier = Arc::clone(&barrier);
    let worker = std::thread::spawn(move || {
        worker_barrier.wait();
        SessionGuard::begin(SessionKind::Attachment).unwrap_err()
    });
    barrier.wait();
    let conflict = worker.join().unwrap();
    assert_eq!(conflict.active.current, first_id);
    assert_eq!(conflict.active.kind, SessionKind::Console);
    assert_eq!(current_session(), Some(first.context()));
    drop(first);

    let reattach = SessionGuard::begin(SessionKind::Attachment).unwrap();
    assert_ne!(reattach.context().current, first_id);
    assert_eq!(reattach.context().previous, Some(first_id));
    assert_eq!(reattach.context().kind, SessionKind::Attachment);
    drop(reattach);
    assert_eq!(current_session(), None);
}

#[test]
fn interleaved_non_owner_drop_cannot_clear_active_session() {
    let _serial = TEST_SESSION_LOCK.lock().unwrap();
    let owner = SessionGuard::begin(SessionKind::Capsule).unwrap();
    let owner_context = owner.context();
    let conflict = SessionGuard::claim(SessionKind::Attachment).unwrap_err();
    assert_eq!(conflict.active, owner_context);
    assert_eq!(current_session(), Some(owner_context));
    drop(owner);
    assert_eq!(current_session(), None);
}

#[test]
fn attachment_continues_console_session_without_ending_it() {
    let _serial = TEST_SESSION_LOCK.lock().unwrap();
    let console = SessionGuard::begin(SessionKind::Console).unwrap();
    let console_context = console.context();

    let attachment = SessionGuard::begin_attachment().unwrap();
    assert_eq!(attachment.context(), console_context);
    drop(attachment);
    assert_eq!(current_session(), Some(console_context));

    drop(console);
    assert_eq!(current_session(), None);
}

#[test]
fn attachment_cannot_continue_non_console_owner() {
    let _serial = TEST_SESSION_LOCK.lock().unwrap();
    let capsule = SessionGuard::begin(SessionKind::Capsule).unwrap();
    let conflict = SessionGuard::begin_attachment().unwrap_err();
    assert_eq!(conflict.active, capsule.context());
    assert_eq!(conflict.requested, SessionKind::Attachment);
    drop(capsule);
}

#[test]
fn autonomous_cycle_export_correlation_matches_cycle_ownership() {
    let _serial = TEST_SESSION_LOCK.lock().unwrap();
    let invocation = current_invocation().unwrap_or_else(|| {
        let invocation = InvocationId::mint();
        set_current_invocation(invocation).expect("install test invocation");
        invocation
    });
    let capsule = SessionGuard::begin(SessionKind::Capsule).unwrap();
    let session = capsule.context().current.to_string();
    let exporter = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("test")));

    tracing::subscriber::with_default(subscriber, || {
        for name in [
            crate::schema::enums::BackgroundCycleName::BranchContext,
            crate::schema::enums::BackgroundCycleName::PrContext,
            crate::schema::enums::BackgroundCycleName::UsageAccount,
            crate::schema::enums::BackgroundCycleName::ProviderProbe,
            crate::schema::enums::BackgroundCycleName::AgentStatus,
            crate::schema::enums::BackgroundCycleName::InstanceRefresh,
        ] {
            crate::autonomous_cycle_operation(name)
                .expect("registered cycle")
                .complete(crate::schema::enums::OutcomeValue::Success, None);
        }
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(async {
                crate::spawn::spawn_prewarm_job(
                    crate::schema::enums::JobType::ImagePrewarm,
                    async {},
                    |()| crate::spawn::DetachedCompletion::success(),
                )
                .await
                .expect("prewarm job");
            });
    });
    provider.force_flush().expect("flush autonomous cycles");
    let spans = exporter.get_finished_spans().expect("autonomous cycles");
    assert_eq!(spans.len(), 8);
    for span in spans
        .iter()
        .filter(|span| span.name.as_ref() == crate::schema::spans::BACKGROUND_CYCLE)
    {
        let attr = |key: &str| {
            span.attributes
                .iter()
                .find(|attribute| attribute.key.as_str() == key)
                .map(|attribute| attribute.value.as_str().into_owned())
        };
        assert_eq!(attr(crate::schema::attrs::CLI_INVOCATION_ID), None);
        assert_eq!(attr(crate::schema::attrs::JOB_ID), None);
        assert_ne!(
            attr(crate::schema::attrs::CLI_INVOCATION_ID),
            Some(invocation.to_string())
        );
        let name = attr(crate::schema::attrs::BACKGROUND_CYCLE_NAME).expect("cycle name");
        if name == crate::schema::enums::BackgroundCycleName::InstanceRefresh.as_str() {
            assert_eq!(attr(crate::schema::attrs::std_attrs::SESSION_ID), None);
        } else {
            assert_eq!(
                attr(crate::schema::attrs::std_attrs::SESSION_ID).as_deref(),
                Some(session.as_str())
            );
        }
    }
    let producer = spans
        .iter()
        .find(|span| span.name.as_ref() == crate::schema::spans::PREWARM_SCHEDULE)
        .expect("producer");
    let consumer = spans
        .iter()
        .find(|span| span.name.as_ref() == crate::schema::spans::PREWARM_ATTEMPT)
        .expect("consumer");
    let attr = |span: &opentelemetry_sdk::trace::SpanData, key: &str| {
        span.attributes
            .iter()
            .find(|attribute| attribute.key.as_str() == key)
            .map(|attribute| attribute.value.as_str().into_owned())
    };
    for span in [producer, consumer] {
        assert_eq!(
            attr(span, crate::schema::attrs::CLI_INVOCATION_ID).as_deref(),
            Some(invocation.to_string().as_str())
        );
        assert_eq!(
            attr(span, crate::schema::attrs::std_attrs::SESSION_ID).as_deref(),
            Some(session.as_str())
        );
    }
    assert_eq!(
        attr(producer, crate::schema::attrs::JOB_ID),
        attr(consumer, crate::schema::attrs::JOB_ID)
    );
    assert_eq!(consumer.links.len(), 1);
    assert_eq!(
        consumer.links[0].span_context.span_id(),
        producer.span_context.span_id()
    );
    drop(capsule);
}
