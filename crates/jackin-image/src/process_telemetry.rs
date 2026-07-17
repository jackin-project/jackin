// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_process::{ExecRequest, ExecResult};
use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue, ProcessExecutableName};

pub(crate) async fn exec_async(
    request: &ExecRequest,
    executable: ProcessExecutableName,
) -> anyhow::Result<ExecResult> {
    let operation = jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::PROCESS_COMMAND,
        &[jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXECUTABLE_NAME,
            value: jackin_telemetry::Value::Str(executable.as_str()),
        }],
    );
    let result = jackin_process::exec_async(request).await;
    let completion = match &result {
        Ok(output) => {
            if let Some(code) = output.code {
                let _attribute = operation.set_attr(jackin_telemetry::Attr {
                    key: jackin_telemetry::schema::attrs::std_attrs::PROCESS_EXIT_CODE,
                    value: jackin_telemetry::Value::I64(i64::from(code)),
                });
            }
            if output.timed_out {
                (OutcomeValue::Timeout, Some(ErrorType::Timeout))
            } else if output.success {
                (OutcomeValue::Success, None)
            } else {
                (OutcomeValue::Failure, Some(ErrorType::ProcessExitNonzero))
            }
        }
        Err(_) => (OutcomeValue::Failure, Some(ErrorType::ProcessSpawnError)),
    };
    operation.complete(completion.0, completion.1);
    result.map_err(|_| anyhow::anyhow!("process spawn failed"))
}

pub(crate) const fn agent_executable(agent: jackin_core::Agent) -> ProcessExecutableName {
    match agent {
        jackin_core::Agent::Claude => ProcessExecutableName::Claude,
        jackin_core::Agent::Codex => ProcessExecutableName::Codex,
        jackin_core::Agent::Amp => ProcessExecutableName::Amp,
        jackin_core::Agent::Kimi => ProcessExecutableName::Kimi,
        jackin_core::Agent::Opencode => ProcessExecutableName::Opencode,
        jackin_core::Agent::Grok => ProcessExecutableName::Grok,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn exports_image_process_outcome_matrix_without_artifact_material() {
        let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
        let _subscriber = tracing::subscriber::set_default(subscriber);

        let success = ExecRequest::new(
            "sh",
            [
                "-c",
                "printf operator-secret-token; printf operator-secret-stderr >&2",
            ],
        );
        exec_async(&success, ProcessExecutableName::Gh)
            .await
            .unwrap();

        let nonzero = ExecRequest::new("sh", ["-c", "exit 23"]);
        exec_async(&nonzero, ProcessExecutableName::Grok)
            .await
            .unwrap();

        let timeout = ExecRequest::new("sh", ["-c", "sleep 1"]).timeout(Duration::from_millis(5));
        exec_async(&timeout, ProcessExecutableName::JackinCapsule)
            .await
            .unwrap();

        let missing = ExecRequest::new(
            "/operator-secret/cache/jackin-capsule.tmp",
            ["operator-secret-version"],
        );
        let error = exec_async(&missing, ProcessExecutableName::JackinCapsule)
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "process spawn failed");

        export.force_flush();
        assert_eq!(export.finished_spans().len(), 4);
        assert_eq!(export.error_span_count(), 3);
        assert!(export.contains_span_text("gh"));
        assert!(export.contains_span_text("grok"));
        assert!(export.contains_span_text("jackin-capsule"));
        assert!(export.contains_span_text("process_exit_nonzero"));
        assert!(export.contains_span_text("process_spawn_error"));
        assert!(export.contains_span_text("timeout"));
        for secret in [
            "operator-secret-token",
            "operator-secret-stderr",
            "/operator-secret/cache/jackin-capsule.tmp",
            "operator-secret-version",
        ] {
            assert!(!export.contains_span_text(secret));
        }
    }
}
