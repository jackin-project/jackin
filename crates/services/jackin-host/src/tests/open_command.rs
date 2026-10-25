// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::host_desktop::host_open_command_with_policy;

#[test]
fn deny_policy_blocks_browser_open() {
    assert_eq!(
        host_open_command_with_policy("https://example.com", Some("no")),
        None
    );
    assert_eq!(
        host_open_command_with_policy("https://example.com", Some("deny")),
        None
    );
}

#[test]
fn allowed_https_url_builds_command_carrying_url() {
    let (program, args) = host_open_command_with_policy("https://example.com/download", None)
        .expect("default policy allows https links");
    assert!(!program.is_empty());
    assert!(
        args.iter()
            .any(|arg| arg.contains("https://example.com/download"))
    );
}

#[test]
fn non_web_inputs_are_rejected() {
    assert_eq!(host_open_command_with_policy("not a url", None), None);
    assert_eq!(
        host_open_command_with_policy("ftp://example.com/x", None),
        None
    );
}
