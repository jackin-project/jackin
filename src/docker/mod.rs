//! Re-exports from `jackin-docker` for backward compatibility within the
//! root binary crate. New code should import directly from `jackin_docker`.

pub use jackin_docker::shell_runner::{ShellRunner, redact_env_args};
pub use jackin_docker::{CommandRunner, RunOptions};

#[cfg(test)]
mod tests;
