// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;

use jackin_core::{AttemptId, StartRequest, SupervisorState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SupervisorModel {
    Cooperative,
    ExitsOnStart { code: i32 },
    NeverWritesState,
}

#[derive(Debug)]
pub(crate) struct Supervisor {
    model: SupervisorModel,
    last: Option<AttemptId>,
    running: Option<AttemptId>,
}

const RUN_DIR: &str = ".jackin/run";

fn write_state(sandbox: &Path, state: SupervisorState) {
    let dir = sandbox.join(RUN_DIR);
    let temp = dir.join("state.tmp");
    let written = fs::create_dir_all(&dir)
        .and_then(|()| fs::write(&temp, format!("{}\n", state.render())))
        .and_then(|()| fs::rename(&temp, dir.join("state")));
    drop(written);
}

impl Supervisor {
    pub(crate) fn new(model: SupervisorModel) -> Supervisor {
        Supervisor {
            model,
            last: None,
            running: None,
        }
    }

    pub(crate) fn restart(&mut self, sandbox: &Path) {
        self.last = None;
        self.running = None;
        let dir = sandbox.join(RUN_DIR);
        drop(fs::remove_file(dir.join("state")));
        drop(fs::remove_file(dir.join("start")));
        if self.model != SupervisorModel::NeverWritesState {
            write_state(sandbox, SupervisorState::Waiting);
        }
    }

    pub(crate) fn stop(&mut self) {
        self.running = None;
    }

    pub(crate) fn request_written(&mut self, sandbox: &Path) {
        if self.model == SupervisorModel::NeverWritesState || self.running.is_some() {
            return;
        }
        let Ok(text) = fs::read_to_string(sandbox.join(RUN_DIR).join("start")) else {
            return;
        };
        let Ok(StartRequest(attempt)) = StartRequest::parse(&text) else {
            return;
        };
        if self.last.is_some_and(|last| attempt <= last) {
            return;
        }
        self.last = Some(attempt);
        match self.model {
            SupervisorModel::Cooperative => {
                self.running = Some(attempt);
                write_state(sandbox, SupervisorState::Running { attempt });
            }
            SupervisorModel::ExitsOnStart { code } => {
                write_state(sandbox, SupervisorState::Exited { attempt, code });
            }
            SupervisorModel::NeverWritesState => {}
        }
    }

    pub(crate) fn codex_exited(&mut self, sandbox: &Path, code: i32) {
        let Some(attempt) = self.running.take() else {
            return;
        };
        write_state(sandbox, SupervisorState::Exited { attempt, code });
        self.request_written(sandbox);
    }
}
