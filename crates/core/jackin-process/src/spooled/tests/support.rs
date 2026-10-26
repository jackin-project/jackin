// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use tempfile::NamedTempFile;

use crate::{ExecRequest, SpoolError};

/// Private named mode-0600 files retained for each strict-spool API call.
pub(super) struct OutputPair {
    stdout: NamedTempFile,
    stderr: NamedTempFile,
    directory: tempfile::TempDir,
}

impl OutputPair {
    pub(super) fn files(&self) -> (File, File) {
        (
            self.stdout
                .as_file()
                .try_clone()
                .expect("clone stdout spool fixture"),
            self.stderr
                .as_file()
                .try_clone()
                .expect("clone stderr spool fixture"),
        )
    }

    pub(super) fn stdout_file(&self) -> File {
        self.stdout
            .as_file()
            .try_clone()
            .expect("clone stdout spool fixture")
    }

    pub(super) fn stderr_file(&self) -> File {
        self.stderr
            .as_file()
            .try_clone()
            .expect("clone stderr spool fixture")
    }

    pub(super) fn stdout_path(&self) -> PathBuf {
        debug_assert!(self.stdout.path().starts_with(self.directory.path()));
        self.stdout.path().to_path_buf()
    }

    pub(super) fn stderr_path(&self) -> PathBuf {
        debug_assert!(self.stderr.path().starts_with(self.directory.path()));
        self.stderr.path().to_path_buf()
    }
}

pub(super) fn output_pair() -> OutputPair {
    let directory = tempfile::tempdir().expect("create spool fixture directory");
    let stdout = NamedTempFile::new_in(directory.path()).expect("create private stdout spool");
    let stderr = NamedTempFile::new_in(directory.path()).expect("create private stderr spool");
    OutputPair {
        stdout,
        stderr,
        directory,
    }
}

pub(super) fn unlinked_file() -> File {
    let pair = output_pair();
    let file = pair.stdout_file();
    std::fs::remove_file(pair.stdout_path()).expect("unlink negative-link-count fixture");
    file
}

pub(super) fn read_file(file: &mut File) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(super) fn request(script: &str, stdout: usize, stderr: usize) -> ExecRequest {
    ExecRequest::new("/bin/sh", ["-c", script]).output_limits(stdout, stderr)
}

pub(super) fn typed_error(error: anyhow::Error) -> SpoolError {
    error
        .downcast::<SpoolError>()
        .expect("strict spool returns its typed error")
}

pub(super) fn file_stdin_error(
    input: File,
    request: &ExecRequest,
    input_limit: usize,
) -> SpoolError {
    let spools = output_pair();
    let (stdout, stderr) = spools.files();
    typed_error(
        crate::exec_spooled_sync_cancellable_with_stdin_file(
            request,
            input,
            input_limit,
            stdout,
            stderr,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .expect_err("invalid file-backed stdin is rejected before spawn"),
    )
}
