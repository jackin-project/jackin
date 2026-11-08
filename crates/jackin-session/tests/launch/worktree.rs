// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{AccountName, Agent, AgentInstance, AgentInstances, ModelName, Provider};
use tokio_util::sync::CancellationToken;

use crate::support::{guard, launch_rig, ready};

#[tokio::test]
async fn worktree_setup_creates_home_branch_and_checkout() -> Result<(), Box<dyn Error>> {
    guard(async {
        let rig = launch_rig().await?;
        let outcome = rig.launch_observed(7).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let instances = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Codex,
                AccountName::parse("work")?,
                ModelName::parse("gpt-5.1-codex")?,
            ),
            AgentInstance::new(
                Agent::Omp,
                Provider::Zai,
                AccountName::parse("work")?,
                ModelName::parse("glm-5.3")?,
            ),
        ])?;
        rig.world
            .session
            .setup_worktrees(
                &target.sandbox,
                &instances,
                instances.as_slice()[0].agent,
                &CancellationToken::new(),
            )
            .await?;
        let root = handle.root();
        assert!(root.join(".jackin/homes/codex").is_dir());
        assert!(root.join(".jackin/homes/omp").is_dir());
        assert!(root.join("work/omp/project/README.md").is_file());
        assert!(root.join("work/omp/project/.git").is_file());
        let gitdir = std::fs::read_to_string(root.join("work/omp/project/.git"))?;
        let gitdir = gitdir.trim().strip_prefix("gitdir: ").ok_or("gitdir")?;
        let head = std::fs::read_to_string(format!("{gitdir}/HEAD"))?;
        assert_eq!(head.trim(), "ref: refs/heads/agent/omp");
        Ok(())
    })
    .await
}
