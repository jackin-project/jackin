//! Re-exports from `jackin-docker` for backward compatibility within the
//! root binary crate. New code should import directly from `jackin_docker`.

pub use jackin_docker::docker_client::{
    BollardDockerClient, ContainerRow, ContainerSpec, ContainerState, DockerApi, NetworkRow,
    RemoveImageOutcome,
};

#[cfg(test)]
pub(crate) use crate::runtime::test_support::FakeDockerClient;

#[cfg(test)]
mod tests;
