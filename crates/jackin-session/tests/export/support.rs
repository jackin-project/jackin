// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use jackin_core::{
    CommitId, EnvValue, LineSink, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, Program,
    SandboxName, SecretOutput, sandbox_layout,
};
use jackin_session::{ExportError, ExportReport};
use jackin_testkit::{HostSnapshot, Rig, SandboxHandle, TestRunner};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub(crate) struct Call {
    pub(crate) program: Program,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
}

impl Call {
    pub(crate) fn subcommand(&self) -> Option<&str> {
        let mut args = self.args.iter();
        while let Some(arg) = args.next() {
            if arg == "-c" {
                args.next();
            } else if !arg.starts_with('-') {
                return Some(arg);
            }
        }
        None
    }
}

#[derive(Debug)]
pub(crate) struct Sees {
    inner: Arc<TestRunner>,
    calls: Mutex<Vec<Call>>,
}

impl Sees {
    pub(crate) fn new(rig: &Rig) -> Sees {
        Sees {
            inner: Arc::clone(&rig.world.runner),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn mark(&self) -> usize {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    pub(crate) fn since(&self, mark: usize) -> Vec<Call> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .skip(mark)
            .cloned()
            .collect()
    }

    pub(crate) fn ran(&self, word: &str) -> bool {
        self.since(0)
            .iter()
            .any(|call| call.args.iter().any(|arg| arg == word))
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
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Call {
                program: spec.program,
                args: spec.args.clone(),
                env: spec
                    .env
                    .iter()
                    .map(|(name, value)| match value {
                        EnvValue::Plain(text) => (name.clone(), text.clone()),
                        EnvValue::Secret(_) => (name.clone(), String::new()),
                    })
                    .collect(),
            });
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

pub(crate) fn rewrite_packed(
    git_dir: &std::path::Path,
    add: &[(Vec<u8>, &CommitId)],
    drop: &[Vec<u8>],
) -> Result<(), Box<dyn Error>> {
    let file = git_dir.join("packed-refs");
    let mut entries: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for line in std::fs::read(&file)
        .unwrap_or_default()
        .split(|byte| *byte == b'\n')
    {
        if line.is_empty() || line.starts_with(b"#") || line.starts_with(b"^") {
            continue;
        }
        let split = line
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or("bad line")?;
        entries.push((line[split + 1..].to_vec(), line[..split].to_vec()));
    }
    entries.retain(|(name, _)| !drop.contains(name));
    for (name, id) in add {
        entries.push((name.clone(), id.as_str().as_bytes().to_vec()));
    }
    entries.sort();
    let mut out = b"# pack-refs with: peeled fully-peeled sorted \n".to_vec();
    for (name, id) in entries {
        out.extend(id);
        out.push(b' ');
        out.extend(name);
        out.push(b'\n');
    }
    std::fs::write(&file, out)?;
    Ok(())
}
