// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(target_os = "linux")]
use super::admitted_identity;
#[cfg(target_os = "linux")]
use jackin_protocol::{CapsuleConfig, SessionIdentity};
#[cfg(target_os = "linux")]
use std::collections::BTreeMap;

#[cfg(target_os = "linux")]
#[test]
fn unknown_instance_never_falls_back_to_shell_identity() {
    let config = CapsuleConfig {
        shell_identity: Some(SessionIdentity {
            uid: 3000,
            gid: 3000,
        }),
        instance_identities: BTreeMap::from([(
            "known".to_owned(),
            SessionIdentity {
                uid: 3001,
                gid: 3001,
            },
        )]),
        ..CapsuleConfig::default()
    };

    assert_eq!(
        admitted_identity(&config, Some("known")),
        config.instance_identities.get("known").copied()
    );
    assert_eq!(admitted_identity(&config, Some("missing")), None);
    assert_eq!(admitted_identity(&config, None), config.shell_identity);
}

#[cfg(not(target_os = "linux"))]
#[test]
fn isolated_sessions_fail_closed_before_any_launch() {
    let args = vec![
        "-".to_owned(),
        "2000".to_owned(),
        "2000".to_owned(),
        "/bin/true".to_owned(),
    ];
    let error = super::run_isolated_command(&args).expect_err("non-Linux launch must fail");
    assert!(error.to_string().contains("Linux Landlock boundary"));
}

#[cfg(target_os = "linux")]
mod linux_grants;
#[cfg(target_os = "linux")]
mod linux_privileges;
#[cfg(target_os = "linux")]
mod linux_rules;
