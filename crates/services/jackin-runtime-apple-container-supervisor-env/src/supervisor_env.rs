// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Supervisor env pairs for the apple-container launch spec.
//!
//! [`apple_supervisor_env`] builds the daemon-mode and supervisor-PID
//! entries (plus the debug telemetry level when requested) that the
//! `launch` path seeds into the container spec env.

/// Build the supervisor env pairs for an apple-container launch spec.
pub fn apple_supervisor_env(debug: bool) -> Vec<(String, String)> {
    // JACKIN_CAPSULE_FORCE_DAEMON=1 enables daemon mode without PID 1 (vminitd
    // is PID 1 inside apple/container VMs; capsule runs as entrypoint at PID 2+).
    let mut env: Vec<(String, String)> = vec![
        ("JACKIN_CAPSULE_FORCE_DAEMON".to_owned(), "1".to_owned()),
        (
            // vminitd is PID 1; Capsule entrypoint is launched after it. This
            // is the Apple launch contract, not a runtime probe of the live PID.
            jackin_protocol::CAPSULE_SUPERVISOR_PID_ENV.to_owned(),
            jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID.to_string(),
        ),
    ];
    if debug {
        env.push(("JACKIN_TELEMETRY_LEVEL".to_owned(), "debug".to_owned()));
    }
    env
}
