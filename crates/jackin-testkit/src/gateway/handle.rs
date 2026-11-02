// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use jackin_core::{
    EnvName, FileMode, Labels, Phase, SandboxName, SandboxPath, SupervisorState, sandbox_layout,
};
use jackin_openshell::TermSize;

use super::RunRecord;
use super::exec::host_path;
use super::state::Cell;

#[derive(Clone, Debug)]
pub struct SandboxHandle {
    cell: Arc<Cell>,
}

impl SandboxHandle {
    pub(super) fn new(cell: Arc<Cell>) -> SandboxHandle {
        SandboxHandle { cell }
    }

    pub fn name(&self) -> SandboxName {
        self.cell.name.clone()
    }

    pub fn phase(&self) -> Phase {
        self.cell.state().phase
    }

    pub fn labels(&self) -> Labels {
        self.cell.state().labels().to_labels()
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
        let root = self.cell.root();
        let mut found = BTreeMap::new();
        collect_files(&root, &root, &mut found);
        found
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

    pub fn runs(&self, program: &SandboxPath) -> Vec<RunRecord> {
        self.cell
            .state()
            .runs
            .iter()
            .filter(|(path, _)| path == program)
            .map(|(_, record)| record.clone())
            .collect()
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

fn collect_files(root: &Path, dir: &Path, found: &mut BTreeMap<SandboxPath, Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect_files(root, &path, found);
        } else if kind.is_file()
            && let (Ok(relative), Ok(bytes)) = (path.strip_prefix(root), std::fs::read(&path))
            && let Ok(sandbox) =
                SandboxPath::parse(&format!("/sandbox/{}", relative.to_string_lossy()))
        {
            found.insert(sandbox, bytes);
        }
    }
}
