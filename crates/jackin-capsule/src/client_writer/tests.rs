// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn attach_response_enqueue_failure_completes_rpc_owner() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let attrs = [
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
                value: jackin_telemetry::Value::Str("jackin"),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
                value: jackin_telemetry::Value::Str("jackin.capsule.Attach/Detach"),
            },
        ];
        let operation =
            jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs)
                .expect("attach server operation");
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        drop(out_rx);
        let (completion_tx, mut completion_rx) = mpsc::unbounded_channel();
        let mut writer = ClientWriter::default();
        writer.attach_with_completions(out_tx, completion_tx);

        writer.send_attach_response(
            jackin_protocol::attach::AttachControlResponse {
                request_id: 7,
                result: jackin_protocol::attach::AttachControlResult::Success,
            },
            crate::attach_protocol::AttachResponseCompletion {
                request_id: 7,
                operation: Some(operation),
                outcome: jackin_telemetry::schema::enums::OutcomeValue::Success,
                error_type: None,
            },
        );

        completion_rx.try_recv().unwrap_err();
    });
    export.force_flush();
    assert_eq!(export.error_span_count(), 1);
}
