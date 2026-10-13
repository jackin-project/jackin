// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn container_state_short_label() {
    let cases: &[(ContainerState, &str)] = &[
        (ContainerState::Running, "running"),
        (ContainerState::Paused, "paused"),
        (ContainerState::Restarting, "restarting"),
        (ContainerState::Removing, "removing"),
        (ContainerState::Created, "created"),
        (ContainerState::Dead, "dead"),
        (
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            },
            "stopped exit:0",
        ),
        (
            ContainerState::Stopped {
                exit_code: 1,
                oom_killed: false,
            },
            "stopped exit:1",
        ),
        (
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: true,
            },
            "stopped oom_killed",
        ),
        (ContainerState::NotFound, "missing"),
        (
            ContainerState::InspectUnavailable("reason".to_owned()),
            "unavailable",
        ),
    ];

    for (state, expected) in cases {
        assert_eq!(
            state.short_label(),
            *expected,
            "short_label mismatch for {state:?}"
        );
    }
}

#[test]
fn docker_http_routes_are_static_bounded_templates() {
    let routes = [
        PING,
        CONTAINER_INSPECT,
        CONTAINER_REMOVE,
        CONTAINER_LIST,
        CONTAINER_CREATE,
        CONTAINER_START,
        VOLUME_REMOVE,
        NETWORK_CREATE,
        NETWORK_REMOVE,
        NETWORK_LIST,
        NETWORK_INSPECT,
        IMAGE_LIST,
        IMAGE_REMOVE,
        IMAGE_INSPECT,
        IMAGE_PULL,
        EXEC_CREATE,
        EXEC_START,
        EXEC_INSPECT,
    ];
    for route in routes {
        assert!(matches!(route.method, "GET" | "POST" | "DELETE"));
        assert!(route.template.starts_with('/'));
        assert!(!route.template.contains('?'));
        assert!(!route.template.contains("private"));
        for segment in route.template.split('/') {
            if segment.starts_with('{') {
                assert!(matches!(segment, "{id}" | "{name}"));
            }
        }
    }
}
