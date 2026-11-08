// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, AgentInstances, FileMode, SandboxPath};

use crate::plan::PlannedFile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadState {
    Born,
    Unborn,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreePlan {
    pub agent: Agent,
    pub path: SandboxPath,
    pub branch: Option<String>,
    pub worktree: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecLaunch {
    pub agent: Agent,
    pub home: SandboxPath,
    pub workdir: SandboxPath,
    pub argv: Vec<String>,
    pub provider: String,
    pub account: String,
    pub model: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiLayout {
    pub files: Vec<PlannedFile>,
    pub directories: Vec<SandboxPath>,
    pub homes: Vec<SandboxPath>,
    pub worktrees: Vec<WorktreePlan>,
    pub exec: Vec<ExecLaunch>,
}

fn path(text: String) -> SandboxPath {
    SandboxPath::parse(&text).unwrap_or_else(|_| SandboxPath::from_static("/sandbox/project"))
}

pub fn plan_multi(
    instances: &AgentInstances,
    lead: Agent,
    task: Option<&str>,
    head: HeadState,
) -> Result<MultiLayout, serde_json::Error> {
    let mut files = Vec::new();
    let mut directories = vec![
        path("/sandbox/.jackin/task".to_owned()),
        path("/sandbox/.jackin/task/status".to_owned()),
        path("/sandbox/.jackin/homes".to_owned()),
    ];
    if let Some(task) = task {
        files.push(PlannedFile {
            path: path("/sandbox/.jackin/task/task.md".to_owned()),
            mode: FileMode(0o444),
            text: task.to_owned(),
        });
    }
    files.push(PlannedFile {
        path: path("/sandbox/.jackin/agents.json".to_owned()),
        mode: FileMode(0o644),
        text: instances.agents_json()?,
    });
    let mut homes = Vec::new();
    let mut worktrees = Vec::new();
    let mut exec = Vec::new();
    for instance in instances.iter() {
        let home = path(format!(
            "/sandbox/.jackin/homes/{}",
            instance.agent.as_str()
        ));
        homes.push(home.clone());
        directories.push(home.clone());
        let is_lead = instance.agent == lead;
        let (workdir, branch, worktree) = if is_lead {
            (path("/sandbox/project".to_owned()), None, false)
        } else {
            match head {
                HeadState::Born => (
                    path(format!("/sandbox/work/{}/project", instance.agent.as_str())),
                    Some(format!("agent/{}", instance.agent.as_str())),
                    true,
                ),
                HeadState::Unborn => (
                    path(format!("/sandbox/work/{}/project", instance.agent.as_str())),
                    None,
                    false,
                ),
            }
        };
        directories.push(workdir.clone());
        if !is_lead {
            directories.push(path(format!(
                "/sandbox/.jackin/run/{}",
                instance.agent.as_str()
            )));
        }
        worktrees.push(WorktreePlan {
            agent: instance.agent,
            path: workdir.clone(),
            branch,
            worktree,
        });
        let mut argv = instance.agent.spec().launch_argv();
        if argv.is_empty() {
            argv.push(instance.agent.as_str().to_owned());
        }
        exec.push(ExecLaunch {
            agent: instance.agent,
            home,
            workdir,
            argv,
            provider: instance.provider.as_str().to_owned(),
            account: instance.account.as_str().to_owned(),
            model: instance.model.to_string(),
        });
    }
    Ok(MultiLayout {
        files,
        directories,
        homes,
        worktrees,
        exec,
    })
}

pub fn agents_json_mismatch(
    planned: &AgentInstances,
    found: Option<&[u8]>,
) -> Result<bool, serde_json::Error> {
    Ok(match found {
        None => true,
        Some(bytes) => bytes != planned.agents_json()?.as_bytes(),
    })
}
