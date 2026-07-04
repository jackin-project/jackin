// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

const CONSTRUCT_DOCKERFILE: &str = include_str!("../../../docker/construct/Dockerfile");

#[test]
fn construct_installs_linux_clipboard_helpers_for_agent_compatibility() {
    for package in ["wl-clipboard", "xauth", "xclip"] {
        assert!(
            CONSTRUCT_DOCKERFILE.contains(package),
            "construct image must install {package} for Linux agent clipboard helper compatibility"
        );
    }
}
