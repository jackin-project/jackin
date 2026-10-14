// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, bail};

const ATTACH_MAX_WAIT_MS: u64 = 60_000;
const ATTACH_POLL_MS: u64 = 500;

pub async fn wait_for_capsule(container_name: &str) -> Result<()> {
    let check_cmd = format!(
        "test -S /jackin/run/jackin.sock && /jackin/runtime/jackin-capsule protocol-check --expected-major {}",
        jackin_protocol::capsule_transport::CONTROL_PROTOCOL_MAJOR
    );
    let deadline =
        tokio::time::Instant::now() + tokio::time::Duration::from_millis(ATTACH_MAX_WAIT_MS);

    loop {
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "timed out waiting for jackin-capsule daemon in container {container_name}; \
                 check `container logs {container_name}` for startup errors"
            );
        }

        let output = jackin_runtime_process_telemetry::process_telemetry::exec_async(
            &jackin_process::ExecRequest::new(
                "container",
                [
                    "exec",
                    "--user",
                    jackin_runtime_identity::identity::CAPSULE_SUPERVISOR_USER,
                    container_name,
                    "sh",
                    "-c",
                    &check_cmd,
                ],
            ),
        )
        .await;

        match output {
            Ok(o) if o.success => return Ok(()),
            _ => {
                tokio::time::sleep(tokio::time::Duration::from_millis(ATTACH_POLL_MS)).await;
            }
        }
    }
}
