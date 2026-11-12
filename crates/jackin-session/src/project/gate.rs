// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{EnvValue, HostPath, ProcessStatus, RepoPath};

use crate::launch::LaunchError;
use crate::project::git::{HostGit, Pace, plain, words};
use crate::project::{ProjectStep, TreeId, TreeMismatch};

type Entries = BTreeMap<Vec<u8>, Vec<u8>>;

pub(super) struct HostGate<'a> {
    pub(super) git: HostGit<'a>,
    pub(super) shadow: HostPath,
    pub(super) index: HostPath,
    pub(super) objects: HostPath,
    pub(super) paths: HostPath,
    pub(super) alternates: &'a HostPath,
}

impl HostGate<'_> {
    fn env(&self) -> Vec<(String, EnvValue)> {
        vec![
            plain("GIT_CONFIG_NOSYSTEM", "1"),
            plain("GIT_CONFIG_GLOBAL", "/dev/null"),
            plain("GIT_INDEX_FILE", self.index.as_path().display().to_string()),
            plain(
                "GIT_OBJECT_DIRECTORY",
                self.objects.as_path().display().to_string(),
            ),
            plain(
                "GIT_ALTERNATE_OBJECT_DIRECTORIES",
                self.alternates.as_path().display().to_string(),
            ),
        ]
    }

    fn add(&self) -> Vec<String> {
        let mut args = words(&["--literal-pathspecs", "add", "-f", "--pathspec-file-nul"]);
        args.push(format!(
            "--pathspec-from-file={}",
            self.paths.as_path().display()
        ));
        args
    }

    pub(super) async fn tree(&self) -> Result<TreeId, LaunchError> {
        let shadow = self.git.within(&self.shadow);
        shadow
            .require(ProjectStep::HostTree, Pace::Long, self.add(), self.env())
            .await?;
        let written = shadow
            .require(
                ProjectStep::HostTree,
                Pace::Quick,
                words(&["write-tree"]),
                self.env(),
            )
            .await?;
        TreeId::parse(String::from_utf8_lossy(&written).trim()).map_err(|error| {
            LaunchError::ProjectOutput {
                step: ProjectStep::HostTree,
                message: error.to_string(),
            }
        })
    }

    pub(super) async fn entries(&self, tree: &TreeId) -> Result<Vec<u8>, LaunchError> {
        self.git
            .within(&self.shadow)
            .require(
                ProjectStep::TreeList,
                Pace::Long,
                words(&["ls-tree", "-r", "-z", tree.as_str()]),
                self.env(),
            )
            .await
    }

    pub(super) async fn autocrlf(&self) -> Result<Option<String>, LaunchError> {
        let output = self
            .git
            .capture(
                Pace::Quick,
                words(&["config", "--get", "core.autocrlf"]),
                Vec::new(),
            )
            .await?;
        Ok(match output.status {
            ProcessStatus::Exited(0) => {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                (!value.is_empty()).then_some(value)
            }
            ProcessStatus::Exited(_) | ProcessStatus::Signaled(_) => None,
        })
    }
}

fn parse(bytes: &[u8]) -> Entries {
    let mut entries = Entries::new();
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        if let Some(tab) = entry.iter().position(|byte| *byte == b'\t') {
            entries.insert(entry[tab + 1..].to_vec(), entry[..tab].to_vec());
        }
    }
    entries
}

fn paths(names: Vec<&Vec<u8>>) -> Vec<RepoPath> {
    names
        .into_iter()
        .filter_map(|name| RepoPath::parse(&String::from_utf8_lossy(name)).ok())
        .collect()
}

pub(super) fn mismatch(
    host: (TreeId, &[u8]),
    sandbox: (TreeId, &[u8]),
    autocrlf: Option<String>,
) -> TreeMismatch {
    let (host_tree, host_bytes) = host;
    let (sandbox_tree, sandbox_bytes) = sandbox;
    let host_entries = parse(host_bytes);
    let sandbox_entries = parse(sandbox_bytes);
    let host_only = host_entries
        .keys()
        .filter(|name| !sandbox_entries.contains_key(*name))
        .collect();
    let sandbox_only = sandbox_entries
        .keys()
        .filter(|name| !host_entries.contains_key(*name))
        .collect();
    let differing = host_entries
        .iter()
        .filter(|(name, header)| {
            sandbox_entries
                .get(*name)
                .is_some_and(|other| other != *header)
        })
        .map(|(name, _)| name)
        .collect();
    TreeMismatch {
        host: host_tree,
        sandbox: sandbox_tree,
        host_only: paths(host_only),
        sandbox_only: paths(sandbox_only),
        differing: paths(differing),
        autocrlf,
    }
}
