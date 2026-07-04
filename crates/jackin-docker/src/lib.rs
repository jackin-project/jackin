// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Concrete Docker daemon and subprocess runner for jackin❯.
//!
//! **Architecture Invariant:** L2 infrastructure crate. Allowed
//! dependencies: `jackin-core`, `jackin-diagnostics`. Must NOT depend
//! on presentation (`jackin-launch-tui`, `jackin-console`, `jackin-tui`)
//! or application (`jackin-runtime`, `jackin-env`). Build output is
//! streamed through the `BuildLogSink` port trait defined in
//! `jackin-core`; the concrete UI adapter (`DiagnosticsBuildLogSink`)
//! lives in `jackin-launch-tui`.

pub mod docker_client;
pub mod net;
pub mod shell_runner;

pub use docker_client::BollardDockerClient;
pub use shell_runner::ShellRunner;
// Re-export the shared traits and types from jackin-core.
pub use jackin_core::{CommandRunner, DockerApi, RunOptions};
