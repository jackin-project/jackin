// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use jackin_core::{CommitId, EnvValue, HostPath, ProcessStatus, SandboxName, TerminalText};

use super::ExportError;
use crate::project::git::{HostGit, Pace, words};

const HEADS: &str = "refs/heads/";
const JACKIN: &str = "jackin/";
const BOUNDS: [&str; 4] = [
    "-c",
    "pack.windowMemory=256m",
    "-c",
    "core.bigFileThreshold=64m",
];
const STRICT: [&str; 4] = [
    "-c",
    "fetch.fsckObjects=true",
    "-c",
    "transfer.fsckObjects=true",
];

pub(super) const HEAD: &str = "HEAD";

#[derive(Debug)]
pub(super) struct HostRefs {
    pub(super) branches: BTreeMap<String, CommitId>,
    pub(super) exported: BTreeMap<String, CommitId>,
    pub(super) every: Vec<String>,
    pub(super) head: Option<CommitId>,
}

#[derive(Debug)]
pub(super) struct Worktree {
    pub(super) path: HostPath,
    pub(super) branch: Option<String>,
}

pub(super) struct Host<'a> {
    pub(super) git: HostGit<'a>,
}

pub(super) fn exported_prefix(sandbox: &SandboxName) -> String {
    format!("{HEADS}{JACKIN}{sandbox}/")
}

fn commit(text: &str) -> Result<CommitId, ExportError> {
    CommitId::parse(text.trim()).map_err(|error| ExportError::HostOutput {
        message: error.to_string(),
    })
}

impl Host<'_> {
    async fn run(&self, pace: Pace, args: Vec<String>) -> Result<Vec<u8>, ExportError> {
        self.run_with(pace, args, Vec::new()).await
    }

    async fn run_with(
        &self,
        pace: Pace,
        args: Vec<String>,
        env: Vec<(String, EnvValue)>,
    ) -> Result<Vec<u8>, ExportError> {
        let output = self.git.capture(pace, args, env).await?;
        if output.status == ProcessStatus::Exited(0) {
            return Ok(output.stdout);
        }
        let text = if output.stderr.is_empty() {
            &output.stdout
        } else {
            &output.stderr
        };
        Err(ExportError::HostGit {
            status: output.status,
            output: TerminalText::sanitize(text),
        })
    }

    pub(super) async fn refs(&self, sandbox: &SandboxName) -> Result<HostRefs, ExportError> {
        let listed = self
            .run(
                Pace::Quick,
                words(&[
                    "for-each-ref",
                    "--format=%(objectname) %(refname)",
                    "refs/heads",
                ]),
            )
            .await?;
        let prefix = exported_prefix(sandbox);
        let mut refs = HostRefs {
            branches: BTreeMap::new(),
            exported: BTreeMap::new(),
            every: Vec::new(),
            head: self.head().await?,
        };
        for line in String::from_utf8_lossy(&listed).lines() {
            let (oid, name) = line.split_once(' ').ok_or(ExportError::HostOutput {
                message: "for-each-ref printed a line without a name".to_owned(),
            })?;
            refs.every.push(name.to_owned());
            let tip = commit(oid)?;
            if let Some(branch) = name.strip_prefix(&prefix) {
                refs.exported.insert(branch.to_owned(), tip);
            } else if let Some(branch) = name.strip_prefix(HEADS)
                && !branch.starts_with(JACKIN)
            {
                refs.branches.insert(branch.to_owned(), tip);
            }
        }
        Ok(refs)
    }

    async fn head(&self) -> Result<Option<CommitId>, ExportError> {
        let output = self
            .git
            .capture(
                Pace::Quick,
                words(&["rev-parse", "-q", "--verify", "HEAD"]),
                Vec::new(),
            )
            .await?;
        match output.status {
            ProcessStatus::Exited(0) => commit(&String::from_utf8_lossy(&output.stdout)).map(Some),
            ProcessStatus::Exited(1) => Ok(None),
            status => Err(ExportError::HostGit {
                status,
                output: TerminalText::sanitize(&output.stderr),
            }),
        }
    }

    pub(super) async fn worktrees(&self) -> Result<Vec<Worktree>, ExportError> {
        let listed = self
            .run(
                Pace::Quick,
                words(&["worktree", "list", "--porcelain", "-z"]),
            )
            .await?;
        let mut found = Vec::new();
        let mut path: Option<HostPath> = None;
        let mut branch = None;
        for field in listed.split(|byte| *byte == 0) {
            if field.is_empty() {
                if let Some(path) = path.take() {
                    found.push(Worktree {
                        path,
                        branch: branch.take(),
                    });
                }
                continue;
            }
            if let Some(rest) = field.strip_prefix(b"worktree ") {
                path = Some(HostPath::parse(&PathBuf::from(OsStr::from_bytes(rest)))?);
            } else if let Some(rest) = field.strip_prefix(b"branch ") {
                branch = Some(String::from_utf8_lossy(rest).into_owned());
            }
        }
        if let Some(path) = path {
            found.push(Worktree { path, branch });
        }
        Ok(found)
    }

    pub(super) async fn verify(&self, bundle: &HostPath) -> Result<(), ExportError> {
        let mut args = words(&BOUNDS);
        args.extend(words(&["bundle", "verify", "--quiet"]));
        args.push(bundle.as_path().display().to_string());
        self.run(Pace::Long, args).await.map(drop)
    }

    pub(super) async fn heads(
        &self,
        bundle: &HostPath,
    ) -> Result<BTreeMap<String, CommitId>, ExportError> {
        let mut args = words(&BOUNDS);
        args.extend(words(&["bundle", "list-heads"]));
        args.push(bundle.as_path().display().to_string());
        let listed = self.run(Pace::Long, args).await?;
        let mut heads = BTreeMap::new();
        for line in String::from_utf8_lossy(&listed).lines() {
            let (oid, name) = line.split_once(' ').ok_or(ExportError::HostOutput {
                message: "list-heads printed a line without a name".to_owned(),
            })?;
            heads.insert(name.to_owned(), commit(oid)?);
        }
        Ok(heads)
    }

    pub(super) async fn fetch(
        &self,
        bundle: &HostPath,
        refspecs: &[String],
    ) -> Result<(), ExportError> {
        let mut args = words(&BOUNDS);
        args.extend(words(&STRICT));
        args.extend(words(&[
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "fetch",
            "--quiet",
            "--atomic",
            "--no-tags",
            "--no-prune",
            "--no-write-fetch-head",
            "--no-recurse-submodules",
        ]));
        args.push(bundle.as_path().display().to_string());
        args.extend_from_slice(refspecs);
        self.run(Pace::Long, args).await.map(drop)
    }
}
