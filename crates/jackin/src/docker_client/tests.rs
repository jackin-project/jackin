//! Tests that tested `BollardDockerClient` internals have moved to `crates/jackin-docker`.
//! Only the `FakeDockerClient` re-export is verified here.

#[test]
fn fake_docker_client_accessible() {
    // Just verify FakeDockerClient is importable from this shim.
    let _ = std::mem::size_of::<super::FakeDockerClient>();
}
