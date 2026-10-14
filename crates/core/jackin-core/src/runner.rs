// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use crate::build_log_sink::BuildLogSink;

#[expect(
    clippy::struct_excessive_bools,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub capture_stderr: bool,
    pub capture_stdout: bool,
    pub quiet: bool,
    pub extra_env: Vec<(String, String)>,
    pub null_stdin: bool,
    pub stream_captured_output: bool,
    pub interactive: bool,
    pub tee_to_build_log: bool,
    pub build_log_sink: Option<Arc<dyn BuildLogSink>>,
    pub timeout: Option<std::time::Duration>,
    #[cfg(unix)]
    pub pinned_cwd: Option<Arc<std::fs::File>>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            capture_stderr: false,
            capture_stdout: false,
            quiet: false,
            extra_env: Vec::new(),
            null_stdin: false,
            stream_captured_output: true,
            interactive: false,
            tee_to_build_log: false,
            build_log_sink: None,
            timeout: None,
            #[cfg(unix)]
            pinned_cwd: None,
        }
    }
}

pub trait CommandRunner {
    async fn observe(
        &mut self,
        _program: &str,
        _args: &[&str],
        _cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<()>;
    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
    async fn capture_with_options(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
        opts: &RunOptions,
    ) -> anyhow::Result<String> {
        #[cfg(unix)]
        if opts.pinned_cwd.is_some() {
            anyhow::bail!("runner does not support descriptor-pinned working directories");
        }
        let _ = opts;
        self.capture(program, args, cwd).await
    }
    async fn capture_combined(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.capture(program, args, cwd).await
    }
    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String>;
}
