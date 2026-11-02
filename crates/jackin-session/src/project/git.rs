// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    EnvValue, HostPath, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, ProcessStatus,
    ProcessStdin, Program, TerminalText,
};
use tokio_util::sync::CancellationToken;

use crate::launch::LaunchError;
use crate::project::ProjectStep;

const QUICK: Duration = Duration::from_mins(2);
const LONG: Duration = Duration::from_mins(30);

pub(crate) fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|word| (*word).to_owned()).collect()
}

pub(crate) fn plain(name: &str, value: impl Into<String>) -> (String, EnvValue) {
    (name.to_owned(), EnvValue::Plain(value.into()))
}

pub(crate) fn exit_code(status: ProcessStatus) -> i32 {
    match status {
        ProcessStatus::Exited(code) => code,
        ProcessStatus::Signaled(signal) => 128 + signal,
    }
}

pub(crate) fn failure(step: ProjectStep, output: &ProcessOutput) -> LaunchError {
    let text = if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    LaunchError::ProjectFailed {
        step,
        exit_code: exit_code(output.status),
        output: TerminalText::sanitize(text),
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Pace {
    Quick,
    Long,
}

#[derive(Clone, Copy)]
pub(crate) struct HostGit<'a> {
    pub(crate) runner: &'a dyn ProcessRunner,
    pub(crate) root: &'a HostPath,
    pub(crate) cancel: &'a CancellationToken,
}

impl HostGit<'_> {
    pub(crate) async fn capture(
        &self,
        pace: Pace,
        args: Vec<String>,
        mut env: Vec<(String, EnvValue)>,
    ) -> Result<ProcessOutput, ProcessError> {
        env.push(plain("GIT_OPTIONAL_LOCKS", "0"));
        let spec = ProcessSpec {
            program: Program::Git,
            args,
            cwd: Some(self.root.clone()),
            env,
            env_remove_prefixes: vec!["GIT_".to_owned()],
            stdin: ProcessStdin::Null,
            timeout: match pace {
                Pace::Quick => QUICK,
                Pace::Long => LONG,
            },
        };
        self.runner.run(&spec, None, self.cancel).await
    }

    pub(crate) async fn require(
        &self,
        step: ProjectStep,
        pace: Pace,
        args: Vec<String>,
        env: Vec<(String, EnvValue)>,
    ) -> Result<Vec<u8>, LaunchError> {
        let output = self.capture(pace, args, env).await?;
        if output.status == ProcessStatus::Exited(0) {
            Ok(output.stdout)
        } else {
            Err(failure(step, &output))
        }
    }
}
