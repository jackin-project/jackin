// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn apple_supervisor_env(debug: bool) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = vec![
        ("JACKIN_CAPSULE_FORCE_DAEMON".to_owned(), "1".to_owned()),
        (
            jackin_protocol::CAPSULE_SUPERVISOR_PID_ENV.to_owned(),
            jackin_protocol::APPLE_CAPSULE_SUPERVISOR_PID.to_string(),
        ),
    ];
    if debug {
        env.push(("JACKIN_TELEMETRY_LEVEL".to_owned(), "debug".to_owned()));
    }
    env
}
