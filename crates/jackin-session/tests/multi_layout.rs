// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AccountName, Agent, AgentInstance, AgentInstances, FileMode, Provider};
use jackin_session::{HeadState, agents_json_mismatch, plan_multi};

fn instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::Codex,
            Provider::Codex,
            AccountName::parse("work")?,
            "gpt-6.1-sol".to_owned(),
        ),
        AgentInstance::new(
            Agent::Omp,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
    ])?)
}

#[test]
fn layout_born_and_unborn() -> Result<(), Box<dyn std::error::Error>> {
    let instances = instances()?;
    let layout = plan_multi(&instances, Some("do it"), HeadState::Born)?;
    let task = layout
        .files
        .iter()
        .find(|file| file.path.as_str() == "/sandbox/.jackin/task/task.md")
        .ok_or("task file")?;
    assert_eq!(task.mode, FileMode(0o444));
    assert!(
        layout
            .directories
            .iter()
            .any(|dir| dir.as_str() == "/sandbox/.jackin/task/status")
    );
    assert_eq!(layout.homes.len(), 2);
    let lead = &layout.worktrees[0];
    assert_eq!(lead.path.as_str(), "/sandbox/project");
    assert_eq!(lead.branch, None);
    let other = &layout.worktrees[1];
    assert!(other.worktree);
    assert_eq!(other.branch.as_deref(), Some("agent/omp"));
    let unborn = plan_multi(&instances, Some("do it"), HeadState::Unborn)?;
    assert!(!unborn.worktrees[1].worktree);
    assert_eq!(unborn.worktrees[1].branch, None);
    assert!(agents_json_mismatch(&instances, Some(b"[]")));
    assert!(!agents_json_mismatch(&instances, None));
    assert!(!agents_json_mismatch(
        &instances,
        Some(instances.agents_json().as_bytes())
    ));
    Ok(())
}
