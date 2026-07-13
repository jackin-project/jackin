#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::manual_assert,
    clippy::duration_suboptimal_units,
    clippy::filter_map_next,
    clippy::map_unwrap_or,
    clippy::redundant_closure,
    unreachable_pub,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]

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
