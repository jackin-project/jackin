// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use jackin_core::{
    CommitId, FileSystem, HostPath, ProcessStatus, ProcessStdin, SandboxName, SecretBytes,
    TerminalText,
};

use super::ExportError;
use crate::project::git::{HostGit, Pace, words};
use crate::project::{HostProject, ObjectFormat};

const HEADS: &str = "refs/heads/";
const JACKIN: &str = "jackin/";
const PINS: [&str; 4] = [
    "-c",
    "core.bigFileThreshold=64m",
    "-c",
    "core.deltaBaseCacheLimit=96m",
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
pub(super) struct Update {
    pub(super) target: String,
    pub(super) tip: CommitId,
    pub(super) observed: Option<CommitId>,
}

#[derive(Debug)]
pub(super) struct Worktree {
    pub(super) path: HostPath,
    pub(super) branch: Option<String>,
}

pub(super) struct Host<'a> {
    git: HostGit<'a>,
    project: &'a HostProject<'a>,
}

pub(super) fn exported_prefix(sandbox: &SandboxName) -> String {
    format!("{HEADS}{JACKIN}{sandbox}/")
}

fn commit(text: &str) -> Result<CommitId, ExportError> {
    CommitId::parse(text.trim()).map_err(|error| ExportError::HostOutput {
        message: error.to_string(),
    })
}

impl<'a> Host<'a> {
    pub(super) fn new(project: &'a HostProject<'a>) -> Host<'a> {
        Host {
            git: project.git(),
            project,
        }
    }

    async fn run(&self, pace: Pace, args: Vec<String>) -> Result<Vec<u8>, ExportError> {
        self.run_from(pace, args, ProcessStdin::Null).await
    }

    async fn run_from(
        &self,
        pace: Pace,
        args: Vec<String>,
        stdin: ProcessStdin,
    ) -> Result<Vec<u8>, ExportError> {
        let output = self.git.capture_from(pace, args, Vec::new(), stdin).await?;
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
            head: self.resolve(HEAD).await?,
        };
        for line in listed.split(|byte| *byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            let (oid, name) = line
                .iter()
                .position(|byte| *byte == b' ')
                .map(|at| (&line[..at], &line[at + 1..]))
                .ok_or(ExportError::HostOutput {
                    message: "for-each-ref printed a line without a name".to_owned(),
                })?;
            let Ok(name) = std::str::from_utf8(name) else {
                continue;
            };
            let oid = std::str::from_utf8(oid).map_err(|_| ExportError::HostOutput {
                message: "for-each-ref printed a non-UTF-8 object name".to_owned(),
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

    async fn resolve(&self, name: &str) -> Result<Option<CommitId>, ExportError> {
        let output = self
            .git
            .capture(
                Pace::Quick,
                words(&["rev-parse", "-q", "--verify", name]),
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
                branch = std::str::from_utf8(rest).ok().map(str::to_owned);
            }
        }
        if let Some(path) = path {
            found.push(Worktree { path, branch });
        }
        Ok(found)
    }

    pub(super) async fn object_format(&self) -> Result<ObjectFormat, ExportError> {
        let printed = self
            .run(Pace::Quick, words(&["rev-parse", "--show-object-format"]))
            .await?;
        ObjectFormat::parse(String::from_utf8_lossy(&printed).trim()).ok_or(
            ExportError::HostOutput {
                message: "rev-parse printed an unknown object format".to_owned(),
            },
        )
    }

    pub(super) async fn import(
        &self,
        fs: &dyn FileSystem,
        bundle: &HostPath,
        pack_offset: u64,
        targets: &[String],
    ) -> Result<(), ExportError> {
        self.project.refuse_foreign_store(fs, targets).await?;
        let mut args = words(&PINS);
        args.extend(words(&["index-pack", "--stdin", "--strict", "--threads=1"]));
        let stdin = ProcessStdin::File {
            path: bundle.clone(),
            offset: pack_offset,
        };
        self.run_from(Pace::Long, args, stdin).await.map(drop)
    }

    pub(super) async fn publish(
        &self,
        fs: &dyn FileSystem,
        updates: Vec<Update>,
    ) -> Result<(), ExportError> {
        let targets: Vec<String> = updates.iter().map(|update| update.target.clone()).collect();
        self.project.refuse_foreign_store(fs, &targets).await?;
        let mut pending = updates;
        while !pending.is_empty() {
            let error = match self.transaction(&pending).await {
                Ok(()) => return Ok(()),
                Err(error @ ExportError::HostGit { .. }) => error,
                Err(error) => return Err(error),
            };
            let mut unmoved = Vec::new();
            let mut landed = false;
            for update in pending {
                let found = self.resolve(&update.target).await?;
                if found.as_ref() == Some(&update.tip) {
                    landed = true;
                } else if found == update.observed {
                    unmoved.push(update);
                } else {
                    return Err(ExportError::RefMoved {
                        host_ref: update.target,
                        observed: update.observed,
                        found,
                    });
                }
            }
            if !landed {
                return Err(error);
            }
            pending = unmoved;
        }
        Ok(())
    }

    async fn transaction(&self, updates: &[Update]) -> Result<(), ExportError> {
        let mut commands = String::new();
        for Update {
            target,
            tip,
            observed,
        } in updates
        {
            commands.push_str("option no-deref\n");
            match observed {
                Some(old) => commands.push_str(&format!("update {target} {tip} {old}\n")),
                None => commands.push_str(&format!("create {target} {tip}\n")),
            }
        }
        let stdin = ProcessStdin::Bytes(SecretBytes::new(commands.into_bytes()));
        self.run_from(Pace::Long, words(&["update-ref", "--stdin"]), stdin)
            .await
            .map(drop)
    }
}
