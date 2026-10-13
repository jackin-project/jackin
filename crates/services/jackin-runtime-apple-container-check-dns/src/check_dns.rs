// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! DNS health check for the apple-container backend.
//!
//! [`check_dns`] runs an `nslookup` probe inside the container
//! after attach returns and warns on the macOS sleep/wake DNS
//! hiccup, so the operator knows to reconnect when the agent
//! cannot reach the network.

/// DNS health check — an `nslookup` probe run after attach returns. macOS
/// sleep/wake can drop DNS inside the VM; surface a "reconnect" hint if affected.
#[expect(
    clippy::print_stderr,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub async fn check_dns(container_name: &str) {
    let result = jackin_runtime_process_telemetry::process_telemetry::exec_async(
        &jackin_process::ExecRequest::new(
            "container",
            [
                "exec",
                "--user",
                jackin_runtime_identity::identity::CAPSULE_SUPERVISOR_USER,
                container_name,
                "sh",
                "-c",
                "nslookup github.com >/dev/null 2>&1 && echo ok || echo hiccup",
            ],
        ),
    )
    .await;

    match result {
        Ok(o) if o.success => {
            let out = String::from_utf8_lossy(&o.stdout).trim().to_owned();
            if out == "hiccup" {
                eprintln!(
                    "[jackin] apple-container: DNS hiccup detected after sleep/wake. \
                     If the agent cannot reach the network, run `jackin hardline` to reconnect."
                );
            }
        }
        _ => {}
    }
}
