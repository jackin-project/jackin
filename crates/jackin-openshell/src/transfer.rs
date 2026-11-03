// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use jackin_core::{
    EnvValue, HostPath, ProcessError, ProcessSpec, ProcessStatus, ProcessStdin, Program,
    ProjectRoot, SandboxName, SandboxPath, Sha256, TerminalText, sandbox_layout,
};
use sha2::{Digest, Sha256 as Hasher};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use crate::cli::{Scope, inherited_env_prefixes, upload_env_prefixes};
use crate::discovery::failure_tail;
use crate::error::{GatewayError, GatewayOp, TransferOp};
use crate::gateway::{ProgramRun, ProgramStdin};
use crate::grpc::Inner;

use jackin_core::OutputLine;
use tokio::sync::mpsc;

const DOWNLOAD_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const LINE_CAPACITY: usize = 16;
const GROWTH_POLL: Duration = Duration::from_millis(10);

const HEX: &[u8; 16] = b"0123456789abcdef";

const HOSTILE_GIT_CONFIG: [(&str, &str); 2] =
    [("core.fsmonitor", "false"), ("core.hooksPath", "/dev/null")];

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn git_config_env() -> Vec<(String, EnvValue)> {
    let mut env = vec![(
        "GIT_CONFIG_COUNT".to_owned(),
        EnvValue::Plain(HOSTILE_GIT_CONFIG.len().to_string()),
    )];
    for (index, (key, value)) in HOSTILE_GIT_CONFIG.iter().enumerate() {
        env.push((
            format!("GIT_CONFIG_KEY_{index}"),
            EnvValue::Plain((*key).to_owned()),
        ));
        env.push((
            format!("GIT_CONFIG_VALUE_{index}"),
            EnvValue::Plain((*value).to_owned()),
        ));
    }
    env
}

fn within_limit(size: u64) -> Result<(), GatewayError> {
    if size > DOWNLOAD_LIMIT {
        Err(GatewayError::DownloadTooLarge {
            size,
            limit: DOWNLOAD_LIMIT,
        })
    } else {
        Ok(())
    }
}

fn temp_beside(target: &Path) -> Option<PathBuf> {
    let (parent, name) = (target.parent()?, target.file_name()?);
    Some(parent.join(format!(
        ".{}.jackin-{}-{}",
        name.to_string_lossy(),
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    )))
}

fn process_error(error: ProcessError, op: GatewayOp) -> GatewayError {
    match error {
        ProcessError::Timeout { after, group, .. } => GatewayError::Timeout { op, after, group },
        ProcessError::Cancelled { group, .. } => GatewayError::Cancelled { op, group },
        failed @ (ProcessError::Spawn { .. }
        | ProcessError::OutputTooLarge { .. }
        | ProcessError::Read { .. }
        | ProcessError::Group { .. }) => GatewayError::Process(failed),
    }
}

fn transfer_failure(op: TransferOp, status: ProcessStatus, note: &str) -> GatewayError {
    GatewayError::Transfer {
        op,
        status,
        tail: TerminalText::sanitize(note.as_bytes()),
    }
}

struct CliCall {
    args: Vec<String>,
    cwd: Option<HostPath>,
    env: Vec<(String, EnvValue)>,
    env_remove_prefixes: Vec<String>,
    step: TransferOp,
    op: GatewayOp,
}

impl Inner {
    fn scope_args(&self) -> Scope<'_> {
        Scope {
            gateway: &self.name,
            endpoint: &self.endpoint,
            workspace: &self.workspace,
        }
    }

    async fn run_cli(&self, call: CliCall, cancel: &CancellationToken) -> Result<(), GatewayError> {
        let spec = ProcessSpec {
            program: Program::Openshell,
            args: call.args,
            cwd: call.cwd,
            env: call.env,
            env_remove_prefixes: call.env_remove_prefixes,
            stdin: ProcessStdin::Null,
            timeout: self.transfer_timeout,
        };
        let (sink, mut lines) = mpsc::channel::<OutputLine>(LINE_CAPACITY);
        let drain = async { while lines.recv().await.is_some() {} };
        let running = async {
            let sink = sink;
            self.runner.run(&spec, Some(&sink), cancel).await
        };
        let (output, ()) = tokio::join!(running, drain);
        let output = output.map_err(|error| process_error(error, call.op))?;
        if output.status == ProcessStatus::Exited(0) {
            Ok(())
        } else {
            Err(GatewayError::Transfer {
                op: call.step,
                status: output.status,
                tail: failure_tail(&output),
            })
        }
    }
}

pub(crate) async fn upload_tree(
    inner: &Inner,
    sandbox: &SandboxName,
    source: &ProjectRoot,
    dest: &SandboxPath,
    cancel: &CancellationToken,
) -> Result<(), GatewayError> {
    let args = inner.scope_args().upload(sandbox, ".", dest, false);
    inner
        .run_cli(
            CliCall {
                args,
                cwd: Some(source.path().clone()),
                env: git_config_env(),
                env_remove_prefixes: upload_env_prefixes(),
                step: TransferOp::UploadFiltered,
                op: GatewayOp::UploadTree,
            },
            cancel,
        )
        .await
}

pub(crate) async fn upload_file(
    inner: &Inner,
    sandbox: &SandboxName,
    source: &HostPath,
    dest: &SandboxPath,
    cancel: &CancellationToken,
) -> Result<(), GatewayError> {
    let name = source.as_path().display().to_string();
    let args = inner.scope_args().upload(sandbox, &name, dest, true);
    inner
        .run_cli(
            CliCall {
                args,
                cwd: None,
                env: git_config_env(),
                env_remove_prefixes: upload_env_prefixes(),
                step: TransferOp::UploadFile,
                op: GatewayOp::UploadFile,
            },
            cancel,
        )
        .await?;
    let sent = local_digest(source.as_path()).await.map_err(|error| {
        transfer_failure(
            TransferOp::UploadFile,
            ProcessStatus::Exited(0),
            &format!("the uploaded file cannot be read: {error}"),
        )
    })?;
    let landed = remote_digest(inner, sandbox, dest, cancel).await?;
    if sent == landed.as_str() {
        Ok(())
    } else {
        Err(transfer_failure(
            TransferOp::Digest,
            ProcessStatus::Exited(0),
            &format!("sha256 mismatch: the sandbox has {landed}, the upload has {sent}"),
        ))
    }
}

async fn local_digest(path: &Path) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        hex.push(char::from(HEX[usize::from(byte >> 4)]));
        hex.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(hex)
}

async fn remote_digest(
    inner: &Inner,
    sandbox: &SandboxName,
    remote: &SandboxPath,
    cancel: &CancellationToken,
) -> Result<Sha256, GatewayError> {
    let run = ProgramRun {
        sandbox: sandbox.clone(),
        program: sandbox_layout::SHA256SUM.clone(),
        args: vec![remote.as_str().to_owned()],
        workdir: None,
        env: std::collections::BTreeMap::new(),
        stdin: ProgramStdin::Closed,
        touch_on_success: None,
        timeout: inner.transfer_timeout,
    };
    let outcome = inner.run_program(&run, cancel).await?;
    if outcome.exit_code != 0 {
        return Err(GatewayError::Transfer {
            op: TransferOp::Digest,
            status: ProcessStatus::Exited(outcome.exit_code),
            tail: crate::discovery::tail(&outcome.stderr),
        });
    }
    let stdout = String::from_utf8_lossy(&outcome.stdout);
    stdout
        .split_whitespace()
        .next()
        .and_then(|token| Sha256::parse(token).ok())
        .ok_or_else(|| {
            transfer_failure(
                TransferOp::Digest,
                ProcessStatus::Exited(0),
                "the sandbox sha256sum output has no digest",
            )
        })
}

async fn remote_size(
    inner: &Inner,
    sandbox: &SandboxName,
    remote: &SandboxPath,
    cancel: &CancellationToken,
) -> Result<u64, GatewayError> {
    let run = ProgramRun {
        sandbox: sandbox.clone(),
        program: sandbox_layout::STAT.clone(),
        args: ["-c", "%s", "--", remote.as_str()]
            .map(str::to_owned)
            .to_vec(),
        workdir: None,
        env: std::collections::BTreeMap::new(),
        stdin: ProgramStdin::Closed,
        touch_on_success: None,
        timeout: inner.transfer_timeout,
    };
    let outcome = inner.run_program(&run, cancel).await?;
    if outcome.exit_code != 0 {
        return Err(GatewayError::Transfer {
            op: TransferOp::Size,
            status: ProcessStatus::Exited(outcome.exit_code),
            tail: crate::discovery::tail(&outcome.stderr),
        });
    }
    String::from_utf8_lossy(&outcome.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|_| {
            transfer_failure(
                TransferOp::Size,
                ProcessStatus::Exited(0),
                "the sandbox stat output has no size",
            )
        })
}

async fn watch_growth(
    temp: &Path,
    stop: &CancellationToken,
    finished: &CancellationToken,
) -> Option<u64> {
    let mut tick = tokio::time::interval(GROWTH_POLL);
    loop {
        tokio::select! {
            biased;
            () = finished.cancelled() => return None,
            _ = tick.tick() => {}
        }
        if let Ok(landed) = tokio::fs::metadata(temp).await
            && landed.len() > DOWNLOAD_LIMIT
        {
            stop.cancel();
            return Some(landed.len());
        }
    }
}

async fn download_into(
    inner: &Inner,
    sandbox: &SandboxName,
    remote: &SandboxPath,
    temp: &Path,
    cancel: &CancellationToken,
) -> Result<Sha256, GatewayError> {
    let destination = temp.display().to_string();
    let args = inner.scope_args().download(sandbox, remote, &destination);
    let call = CliCall {
        args,
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: inherited_env_prefixes(),
        step: TransferOp::Download,
        op: GatewayOp::Download,
    };
    let stop = cancel.child_token();
    let finished = CancellationToken::new();
    let running = async {
        let result = inner.run_cli(call, &stop).await;
        finished.cancel();
        result
    };
    let (result, grown) = tokio::join!(running, watch_growth(temp, &stop, &finished));
    if let Some(size) = grown {
        return Err(GatewayError::DownloadTooLarge {
            size,
            limit: DOWNLOAD_LIMIT,
        });
    }
    result?;
    if let Ok(landed) = tokio::fs::metadata(temp).await {
        within_limit(landed.len())?;
    }
    let expected = remote_digest(inner, sandbox, remote, cancel).await?;
    let actual = local_digest(temp).await.map_err(|error| {
        transfer_failure(
            TransferOp::Download,
            ProcessStatus::Exited(0),
            &format!("the downloaded file cannot be read: {error}"),
        )
    })?;
    if actual == expected.as_str() {
        Ok(expected)
    } else {
        Err(transfer_failure(
            TransferOp::Digest,
            ProcessStatus::Exited(0),
            &format!("sha256 mismatch: the sandbox has {expected}, the download has {actual}"),
        ))
    }
}

pub(crate) async fn download_file(
    inner: &Inner,
    sandbox: &SandboxName,
    remote: &SandboxPath,
    local: &HostPath,
    cancel: &CancellationToken,
) -> Result<Sha256, GatewayError> {
    within_limit(remote_size(inner, sandbox, remote, cancel).await?)?;
    let temp = temp_beside(local.as_path()).ok_or_else(|| {
        transfer_failure(
            TransferOp::Download,
            ProcessStatus::Exited(0),
            "the download target has no directory or name",
        )
    })?;
    let landed = download_into(inner, sandbox, remote, &temp, cancel).await;
    let renamed = match landed {
        Ok(digest) => tokio::fs::rename(&temp, local.as_path())
            .await
            .map(|()| digest)
            .map_err(|error| {
                transfer_failure(
                    TransferOp::Download,
                    ProcessStatus::Exited(0),
                    &format!("the downloaded file cannot be moved into place: {error}"),
                )
            }),
        Err(error) => Err(error),
    };
    if renamed.is_err() {
        drop(tokio::fs::remove_file(&temp).await);
    }
    renamed
}
