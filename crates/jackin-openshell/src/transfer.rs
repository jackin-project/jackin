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

use crate::cli::Scope;
use crate::discovery::failure_tail;
use crate::error::{GatewayError, GatewayOp, TransferOp};
use crate::gateway::{ProgramRun, ProgramStdin, ProjectUpload};
use crate::grpc::Inner;

const HEX: &[u8; 16] = b"0123456789abcdef";
const UPLOAD_STEPS: [(TransferOp, &str, bool); 2] = [
    (TransferOp::UploadFiltered, ".", false),
    (TransferOp::UploadGit, ".git", true),
];

fn process_error(error: ProcessError, op: GatewayOp) -> GatewayError {
    match error {
        ProcessError::Timeout { after, .. } => GatewayError::Timeout { op, after },
        ProcessError::Cancelled { .. } => GatewayError::Cancelled { op },
        spawn @ ProcessError::Spawn { .. } => GatewayError::Process(spawn),
    }
}

fn transfer_failure(op: TransferOp, status: ProcessStatus, note: &str) -> GatewayError {
    GatewayError::Transfer {
        op,
        status,
        tail: TerminalText::sanitize(note.as_bytes()),
    }
}

impl Inner {
    fn scope_args(&self) -> Scope<'_> {
        Scope {
            gateway: &self.name,
            workspace: &self.workspace,
        }
    }

    async fn run_cli(
        &self,
        args: Vec<String>,
        cwd: Option<HostPath>,
        step: TransferOp,
        op: GatewayOp,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let spec = ProcessSpec {
            program: Program::Openshell,
            args,
            cwd,
            env: Vec::new(),
            stdin: ProcessStdin::Null,
            timeout: self.transfer_timeout,
        };
        let output = self
            .runner
            .run(&spec, None, cancel)
            .await
            .map_err(|error| process_error(error, op))?;
        if output.status == ProcessStatus::Exited(0) {
            Ok(())
        } else {
            Err(GatewayError::Transfer {
                op: step,
                status: output.status,
                tail: failure_tail(&output),
            })
        }
    }
}

pub(crate) async fn upload_project(
    inner: &Inner,
    sandbox: &SandboxName,
    upload: &ProjectUpload,
    cancel: &CancellationToken,
) -> Result<(), GatewayError> {
    for (step, source, no_git_ignore) in UPLOAD_STEPS {
        let args = inner
            .scope_args()
            .upload(sandbox, source, &upload.dest, no_git_ignore);
        inner
            .run_cli(
                args,
                Some(upload.source.path().clone()),
                step,
                GatewayOp::Upload,
                cancel,
            )
            .await?;
    }
    Ok(())
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

pub(crate) async fn download_file(
    inner: &Inner,
    sandbox: &SandboxName,
    remote: &SandboxPath,
    local: &HostPath,
    cancel: &CancellationToken,
) -> Result<(), GatewayError> {
    let destination = local.as_path().display().to_string();
    let args = inner.scope_args().download(sandbox, remote, &destination);
    inner
        .run_cli(
            args,
            None,
            TransferOp::Download,
            GatewayOp::Download,
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
