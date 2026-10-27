// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use crate::digest::plan_hash;
use crate::error::{Diagnostic, ErrorCode, WorkflowError, WorkflowResult};
use crate::model::ExecutionPlan;
use crate::resolve::FrozenInput;

const PLANS_DIR: &str = "plans";
const INPUTS_DIR: &str = "inputs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputStore {
    root: PathBuf,
}

impl InputStore {
    pub fn open(root: &Path) -> WorkflowResult<Self> {
        for dir in [PLANS_DIR, INPUTS_DIR] {
            std::fs::create_dir_all(root.join(dir)).map_err(|_| {
                WorkflowError::single(Diagnostic::new(
                    root.display().to_string(),
                    "store",
                    ErrorCode::MissingInput,
                    "cannot create the input store directory",
                    "point the store at a writable directory",
                ))
            })?;
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub fn persist(&self, plan: &ExecutionPlan, inputs: &[&FrozenInput]) -> WorkflowResult<String> {
        for input in inputs {
            self.write_input(&input.digest, &input.bytes)?;
        }
        let hash = plan_hash(plan)?;
        if plan.plan_hash != hash {
            return Err(WorkflowError::single(Diagnostic::new(
                "plan",
                "plan_hash",
                ErrorCode::PlanDigestMismatch,
                "plan carries a hash that does not match its bytes",
                "recompile the workflow to produce the plan",
            )));
        }
        let stored = serde_json::to_vec(plan).map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                "plan",
                "plan",
                ErrorCode::MissingInput,
                "cannot encode the stored plan",
                "recompile the workflow to produce the plan",
            ))
        })?;
        let path = self.root.join(PLANS_DIR).join(format!("{hash}.plan.json"));
        std::fs::write(&path, &stored).map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                path.display().to_string(),
                "plan",
                ErrorCode::MissingInput,
                "cannot write the stored plan",
                "point the store at a writable directory",
            ))
        })?;
        protect(&path);
        Ok(hash)
    }

    pub fn read_plan(&self, hash: &str) -> WorkflowResult<ExecutionPlan> {
        let path = self.root.join(PLANS_DIR).join(format!("{hash}.plan.json"));
        let bytes = std::fs::read(&path).map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                path.display().to_string(),
                "plan",
                ErrorCode::PlanDigestMismatch,
                format!("no stored plan for hash `{hash}`"),
                "recompile the workflow to produce the plan",
            ))
        })?;
        let mut plan: ExecutionPlan = serde_json::from_slice(&bytes).map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                path.display().to_string(),
                "plan",
                ErrorCode::PlanDigestMismatch,
                "stored plan bytes are not a valid plan",
                "recompile the workflow to produce the plan",
            ))
        })?;
        let recomputed = plan_hash(&plan).map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                path.display().to_string(),
                "plan",
                ErrorCode::PlanDigestMismatch,
                "stored plan bytes cannot be hashed",
                "recompile the workflow to produce the plan",
            ))
        })?;
        if recomputed != hash || plan.plan_hash != hash {
            return Err(WorkflowError::single(Diagnostic::new(
                path.display().to_string(),
                "plan_hash",
                ErrorCode::PlanDigestMismatch,
                "stored plan hash does not match the stored bytes",
                "recompile the workflow; never edit stored plans",
            )));
        }
        plan.plan_hash = hash.to_owned();
        Ok(plan)
    }

    pub fn read_input(&self, digest: &str) -> WorkflowResult<Vec<u8>> {
        let path = self.root.join(INPUTS_DIR).join(digest);
        std::fs::read(&path).map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                path.display().to_string(),
                "input",
                ErrorCode::PlanDigestMismatch,
                format!("stored input `{digest}` is missing"),
                "recompile the workflow to restore the inputs",
            ))
        })
    }

    fn write_input(&self, digest: &str, bytes: &[u8]) -> WorkflowResult<()> {
        let path = self.root.join(INPUTS_DIR).join(digest);
        if path.is_file() {
            return Ok(());
        }
        std::fs::write(&path, bytes).map_err(|_| {
            WorkflowError::single(Diagnostic::new(
                path.display().to_string(),
                "input",
                ErrorCode::MissingInput,
                "cannot write the stored input",
                "point the store at a writable directory",
            ))
        })?;
        protect(&path);
        Ok(())
    }
}

fn protect(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o444);
            let _unused = std::fs::set_permissions(path, permissions);
        }
    }
    #[cfg(not(unix))]
    {
        let _unused = path;
    }
}
