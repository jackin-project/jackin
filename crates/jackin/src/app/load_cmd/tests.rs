// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::docker_startup_error;

#[test]
fn docker_startup_error_includes_visible_detail() {
    let error = anyhow::anyhow!("connect to Docker host unix:///tmp/missing.sock")
        .context("failed to connect to Docker daemon");

    let (title, message) = docker_startup_error(&error);

    assert_eq!(title, "Docker daemon not reachable");
    assert!(message.contains("jackin could not connect to the Docker daemon."));
    assert!(message.contains("failed to connect to Docker daemon"));
    assert!(message.contains("connect to Docker host unix:///tmp/missing.sock"));
    assert!(message.contains("Start Docker or switch to a reachable Docker context"));
}
