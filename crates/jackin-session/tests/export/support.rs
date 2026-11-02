// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use jackin_core::{
    CommitId, LineSink, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, SandboxName,
    SecretOutput, sandbox_layout,
};
use jackin_session::{ExportError, ExportReport};
use jackin_testkit::{HostSnapshot, Rig, SandboxHandle, TestRunner};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(crate) struct CancelsOn {
    inner: Arc<TestRunner>,
    word: &'static str,
    token: CancellationToken,
}

impl CancelsOn {
    pub(crate) fn new(rig: &Rig, word: &'static str, token: &CancellationToken) -> CancelsOn {
        CancelsOn {
            inner: Arc::clone(&rig.world.runner),
            word,
            token: token.clone(),
        }
    }
}

#[async_trait]
impl ProcessRunner for CancelsOn {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        if spec.args.iter().any(|arg| arg == self.word) {
            self.token.cancel();
        }
        self.inner.run(spec, lines, cancel).await
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        self.inner.run_secret(spec, cancel).await
    }
}

#[derive(Debug)]
pub(crate) struct Sees {
    inner: Arc<TestRunner>,
    args: Mutex<Vec<Vec<String>>>,
}

impl Sees {
    pub(crate) fn new(rig: &Rig) -> Sees {
        Sees {
            inner: Arc::clone(&rig.world.runner),
            args: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn ran(&self, word: &str) -> bool {
        self.args
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|args| args.iter().any(|arg| arg == word))
    }
}

#[async_trait]
impl ProcessRunner for Sees {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        self.args
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(spec.args.clone());
        self.inner.run(spec, lines, cancel).await
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        self.inner.run_secret(spec, cancel).await
    }
}

pub(crate) async fn export(rig: &Rig, name: &SandboxName) -> Result<ExportReport, ExportError> {
    rig.world
        .session
        .export(&rig.config, name, CancellationToken::new())
        .await
}

pub(crate) fn ref_of(name: &SandboxName, branch: &str) -> String {
    format!("refs/heads/jackin/{name}/{branch}")
}

pub(crate) async fn snapshot(rig: &Rig) -> Result<HostSnapshot, Box<dyn Error>> {
    HostSnapshot::take(rig.repo.root()).await
}

pub(crate) fn exports(snapshot: &HostSnapshot) -> BTreeMap<String, String> {
    snapshot.exported.clone()
}

pub(crate) async fn tip(handle: &SandboxHandle, rev: &str) -> Result<CommitId, Box<dyn Error>> {
    Ok(CommitId::parse(
        handle.git(&["rev-parse", rev]).await?.trim(),
    )?)
}

pub(crate) fn staging_left(rig: &Rig) -> Result<Vec<String>, Box<dyn Error>> {
    let mut left = Vec::new();
    for entry in std::fs::read_dir(rig.world.cache_dir().as_path())? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with("export-") && !name.contains(".lock") {
            left.push(name);
        }
    }
    Ok(left)
}

pub(crate) fn bundles_left(handle: &SandboxHandle) -> Vec<String> {
    handle
        .files()
        .keys()
        .map(|path| path.as_str().to_owned())
        .filter(|path| path.starts_with(sandbox_layout::BUNDLE_DIR.as_str()))
        .collect()
}

pub(crate) async fn branch_with_commit(
    handle: &SandboxHandle,
    branch: &str,
    file: &str,
) -> Result<CommitId, Box<dyn Error>> {
    handle.git(&["checkout", "-q", "-b", branch]).await?;
    handle
        .commit_in_project(file, branch.as_bytes(), branch)
        .await
}

pub(crate) fn digests(
    handle: &SandboxHandle,
) -> BTreeMap<jackin_core::SandboxPath, jackin_core::Sha256> {
    handle
        .files()
        .into_iter()
        .map(|(path, bytes)| (path, jackin_core::Sha256::of(&bytes)))
        .collect()
}

pub(crate) async fn state(rig: &Rig, name: &SandboxName) -> jackin_session::WorkState {
    rig.world.session.work_state(&rig.config, name).await
}
