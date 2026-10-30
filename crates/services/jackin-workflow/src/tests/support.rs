// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use super::super::WorkflowService;

pub(super) struct Fixture {
    _temp: tempfile::TempDir,
    db_path: PathBuf,
    artifacts_dir: PathBuf,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let temp = tempfile::tempdir().expect("k4 fixture tempdir");
        let db_path = temp.path().join("workflow-runs.db");
        let artifacts_dir = temp.path().join("artifacts");
        Self {
            _temp: temp,
            db_path,
            artifacts_dir,
        }
    }

    pub(super) fn open(&self) -> WorkflowService {
        WorkflowService::open(&self.db_path, &self.artifacts_dir).expect("open k4 service")
    }

    pub(super) fn reopen(&self) -> WorkflowService {
        WorkflowService::open(&self.db_path, &self.artifacts_dir).expect("reopen k4 service")
    }
}
