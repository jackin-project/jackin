// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::hash_map::RandomState;
use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasher;
use std::time::Duration;

use jackin_core::{EnvName, SandboxName, SandboxPath, TerminalText};
use openshell_sdk::raw::proto::{
    ExecSandboxInput, ExecSandboxRequest, exec_sandbox_event, exec_sandbox_input,
};
use tokio_util::sync::CancellationToken;

use super::Inner;
use crate::error::{GatewayError, GatewayOp, Missing};
use crate::gateway::{ProgramOutcome, ProgramRun, ProgramStdin, SandboxFile};

const STDIN_CHUNK: usize = 32 * 1024;
const SHELL: &str = "/bin/sh";
const MARKER_SCRIPT: &str = "marker=$1; shift; \"$@\"; code=$?; if [ \"$code\" -eq 0 ]; then touch -- \"$marker\" || exit 1; fi; exit \"$code\"";
const NAME_MAX: usize = 255;
const WRITE_SCRIPT: &str = "umask 077; target=$1; mode=$2; tmp=$3; trap 'rm -f -- \"$tmp\"' EXIT HUP INT TERM; mkdir -p -- \"$(dirname -- \"$target\")\" || exit 1; cat > \"$tmp\" || exit 1; chmod \"$mode\" \"$tmp\" || exit 1; mv -f -- \"$tmp\" \"$target\" || exit 1";
const READ_SCRIPT: &str = "[ -f \"$1\" ] || exit 66; exec cat -- \"$1\"";
const MISSING_FILE: i32 = 66;

pub(super) fn exec_start(
    scope: openshell_sdk::raw::proto::WorkspaceSelector,
    sandbox: &SandboxName,
    command: Vec<String>,
    workdir: Option<&SandboxPath>,
    environment: HashMap<String, String>,
) -> ExecSandboxRequest {
    ExecSandboxRequest {
        workspace_scope: Some(scope),
        sandbox: sandbox.to_string(),
        command,
        workdir: workdir
            .map(|path| path.as_str().to_owned())
            .unwrap_or_default(),
        environment,
        ..ExecSandboxRequest::default()
    }
}

fn start_input(request: ExecSandboxRequest) -> ExecSandboxInput {
    ExecSandboxInput {
        payload: Some(exec_sandbox_input::Payload::Start(request)),
    }
}

fn stdin_inputs(bytes: &[u8]) -> impl Iterator<Item = ExecSandboxInput> + '_ {
    bytes.chunks(STDIN_CHUNK).map(|chunk| ExecSandboxInput {
        payload: Some(exec_sandbox_input::Payload::Stdin(chunk.to_vec())),
    })
}

fn environment(env: &BTreeMap<EnvName, String>) -> HashMap<String, String> {
    env.iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect()
}

fn command(run: &ProgramRun) -> Vec<String> {
    let mut argv = Vec::new();
    if let Some(marker) = &run.touch_on_success {
        argv.extend([
            SHELL.to_owned(),
            "-c".to_owned(),
            MARKER_SCRIPT.to_owned(),
            "jackin-run".to_owned(),
            marker.as_str().to_owned(),
        ]);
    }
    argv.push(run.program.as_str().to_owned());
    argv.extend(run.args.iter().cloned());
    argv
}

fn temp_path(target: &SandboxPath) -> String {
    let token = format!("{:016x}", RandomState::new().hash_one(0_u8));
    let (dir, base) = target
        .as_str()
        .rsplit_once('/')
        .unwrap_or(("", target.as_str()));
    let suffix = format!(".jackin-{token}");
    let beside = format!(".{base}{suffix}");
    let name = if beside.len() <= NAME_MAX {
        beside
    } else {
        suffix
    };
    format!("{dir}/{name}")
}

fn protocol(message: impl ToString) -> GatewayError {
    GatewayError::Protocol {
        message: message.to_string(),
    }
}

impl Inner {
    pub(crate) async fn run_program(
        &self,
        run: &ProgramRun,
        cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError> {
        let start = exec_start(
            self.scope(),
            &run.sandbox,
            command(run),
            run.workdir.as_ref(),
            environment(&run.env),
        );
        let mut messages = vec![start_input(start)];
        if let ProgramStdin::Data(data) = &run.stdin {
            messages.extend(stdin_inputs(data.expose()));
        }
        self.exec_collect(
            GatewayOp::Program,
            &run.sandbox,
            messages,
            run.timeout,
            cancel,
        )
        .await
    }

    pub(crate) async fn read_files(
        &self,
        sandbox: &SandboxName,
        paths: &[SandboxPath],
    ) -> Result<Vec<Option<Vec<u8>>>, GatewayError> {
        let cancel = CancellationToken::new();
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let argv = [SHELL, "-c", READ_SCRIPT, "jackin-read", path.as_str()]
                .map(str::to_owned)
                .to_vec();
            let start = exec_start(self.scope(), sandbox, argv, None, HashMap::new());
            let outcome = self
                .exec_collect(
                    GatewayOp::ReadFiles,
                    sandbox,
                    vec![start_input(start)],
                    self.call_timeout,
                    &cancel,
                )
                .await?;
            match outcome.exit_code {
                0 => files.push(Some(outcome.stdout)),
                MISSING_FILE => files.push(None),
                code => {
                    return Err(protocol(format!(
                        "reading {} failed with exit code {code}: {}",
                        path.as_str(),
                        TerminalText::sanitize(&outcome.stderr).as_str()
                    )));
                }
            }
        }
        Ok(files)
    }

    pub(crate) async fn write_file(
        &self,
        sandbox: &SandboxName,
        file: &SandboxFile,
    ) -> Result<(), GatewayError> {
        let argv = [
            SHELL,
            "-c",
            WRITE_SCRIPT,
            "jackin-write",
            file.path.as_str(),
            &format!("{:o}", file.mode.0),
            &temp_path(&file.path),
        ]
        .map(str::to_owned)
        .to_vec();
        let start = exec_start(self.scope(), sandbox, argv, None, HashMap::new());
        let mut messages = vec![start_input(start)];
        messages.extend(stdin_inputs(file.content.expose()));
        let outcome = self
            .exec_collect(
                GatewayOp::WriteFile,
                sandbox,
                messages,
                self.call_timeout,
                &CancellationToken::new(),
            )
            .await?;
        if outcome.exit_code == 0 {
            Ok(())
        } else {
            Err(protocol(format!(
                "writing {} failed with exit code {}: {}",
                file.path.as_str(),
                outcome.exit_code,
                TerminalText::sanitize(&outcome.stderr).as_str()
            )))
        }
    }

    async fn exec_collect(
        &self,
        op: GatewayOp,
        sandbox: &SandboxName,
        messages: Vec<ExecSandboxInput>,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError> {
        let missing = Some(Missing::Sandbox(sandbox.clone()));
        let collect = async {
            let mut grpc = self.grpc();
            let mut stream = grpc
                .exec_sandbox_interactive(tokio_stream::iter(messages))
                .await
                .map_err(|status| self.map_status(op, missing.clone(), &status))?
                .into_inner();
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut exit = None;
            loop {
                match stream.message().await {
                    Ok(Some(event)) => match event.payload {
                        Some(exec_sandbox_event::Payload::Stdout(chunk)) => {
                            stdout.extend_from_slice(&chunk.data);
                        }
                        Some(exec_sandbox_event::Payload::Stderr(chunk)) => {
                            stderr.extend_from_slice(&chunk.data);
                        }
                        Some(exec_sandbox_event::Payload::Exit(event)) => {
                            exit = Some(event.exit_code);
                        }
                        None => {}
                    },
                    Ok(None) => break,
                    Err(status) => return Err(self.map_status(op, missing.clone(), &status)),
                }
            }
            exit.map(|exit_code| ProgramOutcome {
                exit_code,
                stdout,
                stderr,
            })
            .ok_or_else(|| protocol("the exec stream ended without an exit event"))
        };
        let result = tokio::select! {
            biased;
            () = cancel.cancelled() => Err(GatewayError::Cancelled { op }),
            outcome = tokio::time::timeout(timeout, collect) => match outcome {
                Ok(result) => result,
                Err(_) => Err(GatewayError::Timeout { op, after: timeout }),
            },
        };
        match result {
            Err(error) => Err(self.refine_not_ready(sandbox, error).await),
            ok => ok,
        }
    }
}
