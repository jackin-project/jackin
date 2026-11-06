// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jackin_core::{
    CommitId, EnvName, FileMode, ImageId, LabelError, Labels, Phase, SandboxName, SandboxPath,
    SupervisorState, Timestamp, sandbox_layout,
};
use jackin_openshell::TermSize;

use super::exec::host_path;
use super::state::Cell;
use super::{RunRecord, ShellWitness};
use crate::fixtures::{FixtureError, git};
use crate::host::FakeClock;
use crate::sync::locked;

#[derive(Clone, Debug)]
pub struct SandboxHandle {
    cell: Arc<Cell>,
    clock: Arc<FakeClock>,
}

impl SandboxHandle {
    pub(super) fn new(cell: Arc<Cell>, clock: Arc<FakeClock>) -> SandboxHandle {
        SandboxHandle { cell, clock }
    }

    pub fn name(&self) -> SandboxName {
        self.cell.name.clone()
    }

    pub fn root(&self) -> PathBuf {
        self.cell.root()
    }

    pub fn created_at(&self) -> Option<Timestamp> {
        self.cell.state().created_at
    }

    pub fn forget_created_at(&self) {
        self.cell.state().created_at = None;
    }

    pub fn image_id(&self) -> Option<ImageId> {
        self.cell.state().image_id.clone()
    }

    pub fn phase(&self) -> Phase {
        self.cell.state().phase
    }

    pub fn labels(&self) -> Labels {
        self.cell.state().all_labels()
    }

    pub fn add_label(&self, key: &str, value: &str) -> Result<(), LabelError> {
        let mut state = self.cell.state();
        let extended = Labels::from_pairs(
            state
                .all_labels()
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .chain([(key.to_owned(), value.to_owned())]),
        )?;
        state.shown = Some(extended);
        Ok(())
    }

    pub fn environment(&self) -> BTreeMap<EnvName, String> {
        self.cell.state().spec.environment()
    }

    pub fn file(&self, path: &SandboxPath) -> Option<Vec<u8>> {
        let host = host_path(&self.cell.root(), path.as_str())?;
        host.is_file().then(|| std::fs::read(host).ok())?
    }

    pub fn file_mode(&self, path: &SandboxPath) -> Option<FileMode> {
        let host = host_path(&self.cell.root(), path.as_str())?;
        let meta = std::fs::metadata(host).ok()?;
        meta.is_file()
            .then(|| FileMode(meta.permissions().mode() & 0o777))
    }

    pub fn files(&self) -> BTreeMap<SandboxPath, Vec<u8>> {
        self.walk().0
    }

    pub fn links(&self) -> BTreeMap<SandboxPath, String> {
        self.walk().1
    }

    fn walk(
        &self,
    ) -> (
        BTreeMap<SandboxPath, Vec<u8>>,
        BTreeMap<SandboxPath, String>,
    ) {
        let root = self.cell.root();
        let mut walked = Walked::default();
        collect(&root, &root, &mut walked);
        (walked.files, walked.links)
    }

    pub fn supervisor(&self) -> Option<SupervisorState> {
        let bytes = self.file(&sandbox_layout::RUN_STATE)?;
        SupervisorState::parse(&String::from_utf8_lossy(&bytes)).ok()
    }

    pub fn attachments(&self) -> usize {
        self.cell.state().main.attachments()
    }

    pub fn main_input(&self) -> Vec<u8> {
        self.cell.state().main.input()
    }

    pub fn pty_size(&self) -> Option<TermSize> {
        self.cell.state().main.pty()
    }

    pub fn resizes(&self) -> Vec<TermSize> {
        self.cell.state().main.resizes()
    }

    pub fn raw_input(&self) -> Vec<u8> {
        self.cell.state().main.raw()
    }

    pub fn advance_repaint(&self) {
        self.cell.state().main.advance_repaint();
    }

    pub fn set_replay_window(&self, bytes: Option<usize>) {
        self.cell.state().replay_window = bytes;
    }

    pub fn set_stream_exit_status(&self, status: Option<i32>) {
        self.cell.state().stream_status = status;
    }

    pub fn shells(&self) -> Vec<ShellWitness> {
        self.cell
            .state()
            .shells
            .iter()
            .map(|shell| locked(shell).clone())
            .collect()
    }

    pub fn runs(&self, program: &SandboxPath) -> Vec<RunRecord> {
        self.cell
            .state()
            .runs
            .iter()
            .filter(|(path, _)| path == program)
            .map(|(_, record)| record.clone())
            .collect()
    }

    pub async fn git(&self, args: &[&str]) -> Result<String, FixtureError> {
        let _working = FakeClock::working(&self.clock);
        let project = self.root().join("project");
        Ok(String::from_utf8(git(&project, args).await?)?)
    }

    pub async fn commit_in_project(
        &self,
        file: &str,
        content: &[u8],
        message: &str,
    ) -> Result<CommitId, FixtureError> {
        let _working = FakeClock::working(&self.clock);
        let target = self.root().join("project").join(file);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&target, content).await?;
        self.git(&["add", "-A"]).await?;
        self.git(&["commit", "-q", "-m", message]).await?;
        Ok(CommitId::parse(
            self.git(&["rev-parse", "HEAD"]).await?.trim(),
        )?)
    }

    pub fn print_main(&self, bytes: &[u8]) {
        self.cell.state().main.emit(bytes);
    }

    pub fn undelivered_main_output(&self) -> usize {
        self.cell.state().main.undelivered()
    }

    pub fn close_main_streams(&self) {
        self.cell.state().main.exit(None);
    }

    pub fn exit_codex(&self, code: i32) {
        let root = self.cell.root();
        let mut state = self.cell.state();
        if let Some(supervisor) = &mut state.supervisor {
            supervisor.codex_exited(&root, code);
        }
    }

    pub fn exit_main(&self, code: i32) {
        let mut state = self.cell.state();
        if state.phase == Phase::Ready {
            state.main_exited(code);
        }
    }
}

#[derive(Default)]
struct Walked {
    files: BTreeMap<SandboxPath, Vec<u8>>,
    links: BTreeMap<SandboxPath, String>,
}

fn collect(root: &Path, dir: &Path, found: &mut Walked) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect(root, &path, found);
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let Ok(sandbox) = SandboxPath::parse(&format!("/sandbox/{}", relative.to_string_lossy()))
        else {
            continue;
        };
        if kind.is_symlink() {
            if let Ok(target) = std::fs::read_link(&path) {
                found
                    .links
                    .insert(sandbox, target.to_string_lossy().into_owned());
            }
        } else if kind.is_file()
            && let Ok(bytes) = std::fs::read(&path)
        {
            found.files.insert(sandbox, bytes);
        }
    }
}
