//! jackin-docker: Docker client adapter and shell runner.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`DockerApi`] — Docker client surface.

pub mod docker_client;
pub mod net;
pub mod shell_runner;

pub use docker_client::BollardDockerClient;
pub use shell_runner::ShellRunner;
// Re-export the shared traits and types from jackin-core.
pub use jackin_core::{CommandRunner, DockerApi, RunOptions};
