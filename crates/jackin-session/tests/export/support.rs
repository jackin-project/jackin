// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;

use jackin_core::{CommitId, SandboxName};
use jackin_session::{ExportError, ExportReport};
use jackin_testkit::{HostSnapshot, Rig, SandboxHandle};
use tokio_util::sync::CancellationToken;

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
