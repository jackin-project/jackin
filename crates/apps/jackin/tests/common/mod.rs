#![expect(
    unreachable_pub,
    reason = "shared integration-test helper module: pub organizes fixtures, not a crate export"
)]

use jackin_core::JackinPaths;
use jackin_docker::docker_client::ContainerSpec;

pub use jackin_test_support::{FakeDockerClient, FakeRunner};

fn fake_runner_usage_marker() -> FakeRunner {
    FakeRunner::for_load_agent([String::new()])
}
const _: fn() -> FakeRunner = fake_runner_usage_marker;

pub fn install_capsule_binary_stub(paths: &JackinPaths) {
    jackin_image::capsule_binary::install_test_stub(paths)
        .expect("install jackin-capsule test stub");
}

pub fn install_agent_binary_stubs(paths: &JackinPaths) {
    for agent in jackin_core::Agent::ALL {
        jackin_image::agent_binary::install_test_stub(paths, *agent)
            .expect("install agent binary stub");
    }
}

const _: fn(&JackinPaths) = install_capsule_binary_stub;
const _: fn(&JackinPaths) = install_agent_binary_stubs;

pub fn launched_role_container(docker: &FakeDockerClient) -> (String, ContainerSpec) {
    docker
        .created_containers
        .borrow()
        .iter()
        .find(|(_, spec)| {
            spec.labels
                .get("jackin.kind")
                .is_some_and(|value| value == "role")
        })
        .cloned()
        .expect("expected role container create request")
}
const _: fn(&FakeDockerClient) -> (String, ContainerSpec) = launched_role_container;
