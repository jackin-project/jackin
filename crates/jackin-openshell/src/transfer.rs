// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{
    HostPath, ProcessError, ProcessSpec, ProcessStatus, ProcessStdin, Program, SandboxName,
    SandboxPath, Sha256, TerminalText, sandbox_layout,
};
use sha2::{Digest, Sha256 as Hasher};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use crate::cli::{Scope, inherited_env_prefixes, upload_env_prefixes};
use crate::discovery::failure_tail;
use crate::error::{GatewayError, GatewayOp, TransferOp};
use crate::gateway::{ProgramRun, ProgramStdin, ProjectUpload, SandboxFile};
use crate::git_config::{parse_listing, render, sanitize_entries};
use crate::grpc::Inner;

use jackin_core::{EnvValue, FileMode, OutputLine, ProcessRunner, SecretBytes};
use std::time::Duration;
use tokio::sync::mpsc;

const DOWNLOAD_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
const GIT_LIST_TIMEOUT: Duration = Duration::from_secs(30);
const GIT_CONFIG_MODE: FileMode = FileMode(0o644);
const LINE_CAPACITY: usize = 16;
static STAT: SandboxPath = SandboxPath::from_static("/usr/bin/stat");

const HEX: &[u8; 16] = b"0123456789abcdef";
const UPLOAD_STEPS: [(TransferOp, &str, bool); 2] = [
    (TransferOp::UploadFiltered, ".", false),
    (TransferOp::UploadGit, ".git", true),
];

fn process_error(error: ProcessError, op: GatewayOp) -> GatewayError {
    match error {
        ProcessError::Timeout { after, .. } => GatewayError::Timeout { op, after },
        ProcessError::Cancelled { .. } => GatewayError::Cancelled { op },
        failed @ (ProcessError::Spawn { .. }
        | ProcessError::OutputTooLarge { .. }
        | ProcessError::Read { .. }) => GatewayError::Process(failed),
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
            env: Vec::new(),
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

pub async fn sanitized_git_config(
    runner: &dyn ProcessRunner,
    config: &HostPath,
    cancel: &CancellationToken,
) -> Result<String, GatewayError> {
    let mut args: Vec<String> = ["config", "--no-includes", "-f"]
        .map(str::to_owned)
        .to_vec();
    args.push(config.as_path().display().to_string());
    args.extend(["--list", "-z"].map(str::to_owned));
    let spec = ProcessSpec {
        program: Program::Git,
        args,
        cwd: None,
        env: vec![
            (
                "GIT_CONFIG_NOSYSTEM".to_owned(),
                EnvValue::Plain("1".to_owned()),
            ),
            (
                "GIT_CONFIG_GLOBAL".to_owned(),
                EnvValue::Plain("/dev/null".to_owned()),
            ),
        ],
        env_remove_prefixes: vec!["GIT_".to_owned()],
        stdin: ProcessStdin::Null,
        timeout: GIT_LIST_TIMEOUT,
    };
    let output = runner
        .run(&spec, None, cancel)
        .await
        .map_err(|error| process_error(error, GatewayOp::Upload))?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(GatewayError::Transfer {
            op: TransferOp::GitConfig,
            status: output.status,
            tail: failure_tail(&output),
        });
    }
    render(&sanitize_entries(parse_listing(&output.stdout)?))
}

fn project_paths(upload: &ProjectUpload) -> Result<(HostPath, HostPath), GatewayError> {
    let root = upload.source.path();
    let unusable = |_| GatewayError::UnsupportedGitDirectory { path: root.clone() };
    Ok((
        HostPath::resolve(Path::new(".git"), root).map_err(unusable)?,
        HostPath::resolve(Path::new(".git/config"), root).map_err(unusable)?,
    ))
}

fn sandbox_config_path(dest: &SandboxPath) -> Result<SandboxPath, GatewayError> {
    dest.join(".git")
        .and_then(|git| git.join("config"))
        .map_err(|error| GatewayError::InvalidArgument {
            field: "dest".to_owned(),
            message: error.to_string(),
        })
}

pub(crate) async fn upload_project(
    inner: &Inner,
    sandbox: &SandboxName,
    upload: &ProjectUpload,
    cancel: &CancellationToken,
) -> Result<(), GatewayError> {
    let (git_dir, config) = project_paths(upload)?;
    let is_directory = tokio::fs::symlink_metadata(git_dir.as_path())
        .await
        .is_ok_and(|metadata| metadata.is_dir());
    if !is_directory {
        return Err(GatewayError::UnsupportedGitDirectory { path: git_dir });
    }
    let target = sandbox_config_path(&upload.dest)?;
    let sanitized = sanitized_git_config(inner.runner.as_ref(), &config, cancel).await?;
    for (step, source, no_git_ignore) in UPLOAD_STEPS {
        let args = inner
            .scope_args()
            .upload(sandbox, source, &upload.dest, no_git_ignore);
        inner
            .run_cli(
                CliCall {
                    args,
                    cwd: Some(upload.source.path().clone()),
                    env_remove_prefixes: upload_env_prefixes(),
                    step,
                    op: GatewayOp::Upload,
                },
                cancel,
            )
            .await?;
    }
    inner
        .write_file(
            sandbox,
            &SandboxFile {
                path: target,
                mode: GIT_CONFIG_MODE,
                content: SecretBytes::new(sanitized.into_bytes()),
            },
        )
        .await
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
        program: STAT.clone(),
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

async fn fetch(
    inner: &Inner,
    sandbox: &SandboxName,
    remote: &SandboxPath,
    local: &HostPath,
    cancel: &CancellationToken,
) -> Result<(), GatewayError> {
    let size = remote_size(inner, sandbox, remote, cancel).await?;
    if size > DOWNLOAD_LIMIT {
        return Err(GatewayError::DownloadTooLarge {
            size,
            limit: DOWNLOAD_LIMIT,
        });
    }
    let destination = local.as_path().display().to_string();
    let args = inner.scope_args().download(sandbox, remote, &destination);
    inner
        .run_cli(
            CliCall {
                args,
                cwd: None,
                env_remove_prefixes: inherited_env_prefixes(),
                step: TransferOp::Download,
                op: GatewayOp::Download,
            },
            cancel,
        )
        .await?;
    let expected = remote_digest(inner, sandbox, remote, cancel).await?;
    let actual = local_digest(local.as_path()).await.map_err(|error| {
        transfer_failure(
            TransferOp::Download,
            ProcessStatus::Exited(0),
            &format!("the downloaded file cannot be read: {error}"),
        )
    })?;
    if actual == expected.as_str() {
        Ok(())
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
) -> Result<(), GatewayError> {
    let result = fetch(inner, sandbox, remote, local, cancel).await;
    if result.is_err() {
        drop(tokio::fs::remove_file(local.as_path()).await);
    }
    result
}
