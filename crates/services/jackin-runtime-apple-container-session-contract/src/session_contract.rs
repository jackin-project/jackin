// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Session contract printer for the apple-container backend.
//!
//! [`print_session_contract`] prints the security-boundary summary
//! shown to the operator before the interactive attach begins, so
//! they see the isolation model and residual risks before the
//! session starts.

use jackin_runtime_apple_container_client::apple_container_client::AppleContainerMount;

/// Print the session contract — the security boundary summary shown to the
/// operator before the interactive attach begins, so they see the isolation
/// model and residual risks before the session starts.
#[expect(
    clippy::print_stderr,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn print_session_contract(
    container_name: &str,
    image: &str,
    provider_version: &str,
    mounts: &[AppleContainerMount],
    debug: bool,
) {
    eprintln!();
    eprintln!("[jackin] session contract");
    eprintln!("  backend:              apple-container");
    eprintln!("  provider:             apple/container {provider_version}");
    eprintln!("  container:            {container_name}");
    eprintln!("  image:                {image}");
    eprintln!("  isolation:            own Linux kernel via Virtualization.framework");
    eprintln!("  host filesystem:      explicit bind mounts only");
    eprintln!("  host Docker socket:   not mounted");
    eprintln!("  inner Docker (DinD):  disabled — pending Phase 0 DinD validation");
    eprintln!(
        "  force_daemon:         JACKIN_CAPSULE_FORCE_DAEMON=1 (capsule PID 2+ under vminitd)"
    );
    eprintln!("  mounts ({}):", mounts.len());
    if mounts.is_empty() {
        eprintln!("    (none)");
    } else {
        for mount in mounts {
            let suffix = if mount.readonly { ":ro" } else { "" };
            eprintln!(
                "    {}:{}{suffix}",
                mount.source.display(),
                mount.target.display()
            );
        }
    }
    eprintln!("  network:              per-container IP via vmnet (no port mapping)");
    eprintln!("  dns:                  may hiccup after macOS sleep/wake — reconnect if affected");
    eprintln!("  residual risk:");
    eprintln!("    DinD not enabled; Docker workflows inside the VM require Phase 0 validation.");
    eprintln!("    apple/container vminitd is PID 1; signal forwarding relies on gRPC/vsock.");
    eprintln!("    Build-time Docker (image build) still runs on host Docker engine.");
    if debug {
        eprintln!("  debug mode:           on (--debug)");
    }
    eprintln!();
}
