// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::{AccountName, Agent, AgentInstance, AgentInstances, Provider, SandboxPath};
use jackin_openshell::{Gateway, GatewayError, ProgramRun, ProgramStdin};
use jackin_testkit::{Fault, Trigger};
use tokio_util::sync::CancellationToken;

use crate::support::{guard, launch_rig, ready};

const FLIP_ANCESTOR: &str = r#"root=$1
outside=$2
count=0
while [ "$count" -lt 3 ]; do
  count=$((count + 1))
  rm -rf "$root/work"
  ln -s "$outside" "$root/work"
done
printf '%s\n' "$count"
exit 0
"#;

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
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        rig.world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await?;
        let root = handle.root();
        assert!(root.join(".jackin/homes/codex").is_dir());
        assert!(root.join(".jackin/homes/claude").is_dir());
        assert!(root.join("work/claude/project/README.md").is_file());
        assert!(root.join("work/claude/project/.git").is_file());
        let gitdir = std::fs::read_to_string(root.join("work/claude/project/.git"))?;
        let gitdir = gitdir.trim().strip_prefix("gitdir: ").ok_or("gitdir")?;
        let head = std::fs::read_to_string(format!("{gitdir}/HEAD"))?;
        assert_eq!(head.trim(), "ref: refs/heads/agent/claude");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn worktree_setup_replaces_a_stale_worktree_directory() -> Result<(), Box<dyn Error>> {
    guard(async {
        let rig = launch_rig().await?;
        let outcome = rig.launch_observed(8).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let stale = handle.root().join("work/claude/project");
        std::fs::create_dir_all(&stale)?;
        std::fs::write(stale.join("stale.txt"), b"stale")?;
        let instances = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        rig.world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await?;
        assert!(
            handle
                .root()
                .join("work/claude/project/README.md")
                .is_file()
        );
        assert!(handle.root().join("work/claude/project/.git").is_file());
        assert!(!stale.join("stale.txt").exists());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn worktree_setup_refuses_a_repository_without_an_initial_commit()
-> Result<(), Box<dyn Error>> {
    guard(async {
        let rig = launch_rig().await?;
        let outcome = rig.launch_observed(9).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        handle
            .git(&["checkout", "-q", "--orphan", "unborn"])
            .await?;
        let instances = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        let refused = rig
            .world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await;
        assert!(
            matches!(refused, Err(GatewayError::Protocol { .. })),
            "{refused:?}"
        );
        assert!(!handle.root().join("work/claude/project").exists());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn worktree_setup_keeps_the_lead_project_when_the_parent_is_a_symlink()
-> Result<(), Box<dyn Error>> {
    guard(async {
        let rig = launch_rig().await?;
        let outcome = rig.launch_observed(10).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let root = handle.root();
        std::fs::create_dir_all(root.join("work"))?;
        std::os::unix::fs::symlink(&root, root.join("work/claude"))?;
        let instances = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        let refused = rig
            .world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await;
        assert!(
            matches!(refused, Err(GatewayError::Protocol { .. })),
            "{refused:?}"
        );
        assert!(root.join("project/.git").exists());
        assert!(root.join("project/README.md").is_file());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn worktree_setup_keeps_an_existing_valid_checkout() -> Result<(), Box<dyn Error>> {
    guard(async {
        let rig = launch_rig().await?;
        let outcome = rig.launch_observed(11).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let instances = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        rig.world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await?;
        let checkout = handle.root().join("work/claude/project");
        std::fs::write(checkout.join("uncommitted.txt"), b"kept")?;
        rig.world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await?;
        assert!(checkout.join("uncommitted.txt").is_file());
        assert!(checkout.join(".git").is_file());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn worktree_setup_reconciles_a_stale_registration_and_resumes() -> Result<(), Box<dyn Error>>
{
    guard(async {
        let rig = launch_rig().await?;
        let outcome = rig.launch_observed(12).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let instances = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        rig.world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await?;
        let checkout = handle.root().join("work/claude/project");
        std::fs::remove_dir_all(&checkout)?;
        rig.world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await?;
        assert!(checkout.join(".git").is_file());
        assert!(checkout.join("README.md").is_file());
        let gitdir = std::fs::read_to_string(checkout.join(".git"))?;
        let gitdir = gitdir.trim().strip_prefix("gitdir: ").ok_or("gitdir")?;
        let head = std::fs::read_to_string(format!("{gitdir}/HEAD"))?;
        assert_eq!(head.trim(), "ref: refs/heads/agent/claude");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn worktree_setup_confines_every_step_while_an_ancestor_flaps() -> Result<(), Box<dyn Error>>
{
    guard(async {
        let rig = launch_rig().await?;
        let outcome = rig.launch_observed(13).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let root = handle.root();
        let outside = root
            .parent()
            .ok_or("the sandbox has no parent")?
            .join("outside-sentinel");
        std::fs::create_dir_all(&outside)?;
        std::fs::write(outside.join("sentinel.txt"), b"intact")?;
        let instances = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        let cancel = CancellationToken::new();
        let probe = Trigger::Program(SandboxPath::from_static("/usr/bin/git"));
        rig.gateway().inject(Fault::HoldAt {
            trigger: probe.clone(),
        });
        let setup = rig
            .world
            .session
            .setup_worktrees(&target.sandbox, &instances, &cancel);
        let flipping = async {
            assert!(rig.gateway().until_held(&probe).await);
            let flip = ProgramRun {
                sandbox: target.sandbox.clone(),
                program: SandboxPath::from_static("/bin/sh"),
                args: vec![
                    "-c".to_owned(),
                    FLIP_ANCESTOR.to_owned(),
                    "flip".to_owned(),
                    root.to_string_lossy().into_owned(),
                    outside.to_string_lossy().into_owned(),
                ],
                workdir: None,
                env: std::collections::BTreeMap::new(),
                stdin: ProgramStdin::Closed,
                touch_on_success: None,
                timeout: Duration::from_secs(120),
            };
            let flipped = rig.world.gateway.run_program(&flip, &cancel).await?;
            assert!(rig.gateway().release(&probe));
            Ok::<_, Box<dyn Error>>(flipped)
        };
        let (setup, flipping) = tokio::join!(setup, flipping);
        let flipped = flipping?;
        assert_eq!(flipped.exit_code, 0, "{flipped:?}");
        let flips = String::from_utf8_lossy(&flipped.stdout)
            .trim()
            .parse::<u32>()
            .map_err(|_| format!("the flapper reported no flips: {flipped:?}"))?;
        assert_eq!(flips, 3, "{flipped:?}");
        let substituted = std::fs::symlink_metadata(root.join("work"))
            .map_err(|error| format!("the ancestor is unreadable: {error}"))?;
        assert!(substituted.file_type().is_symlink(), "{substituted:?}");
        let refused = setup;
        assert!(
            matches!(
                refused,
                Err(GatewayError::Protocol { ref message })
                    if message.contains("refused a substituted ancestor")
            ),
            "{refused:?}"
        );
        assert!(root.join("project/.git").exists());
        assert!(root.join("project/README.md").is_file());
        let mut outside_entries: Vec<String> = std::fs::read_dir(&outside)?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        outside_entries.sort();
        assert_eq!(outside_entries, vec!["sentinel.txt".to_owned()]);
        assert_eq!(
            std::fs::read(outside.join("sentinel.txt"))?,
            b"intact".to_vec()
        );
        std::fs::remove_file(root.join("work"))?;
        rig.world
            .session
            .setup_worktrees(&target.sandbox, &instances, &cancel)
            .await?;
        assert!(root.join("work/claude/project/.git").is_file());
        assert!(root.join("project/.git").exists());
        let mut outside_entries: Vec<String> = std::fs::read_dir(&outside)?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        outside_entries.sort();
        assert_eq!(outside_entries, vec!["sentinel.txt".to_owned()]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn worktree_setup_repairs_a_registration_that_points_at_an_old_checkout()
-> Result<(), Box<dyn Error>> {
    guard(async {
        let rig = launch_rig().await?;
        let outcome = rig.launch_observed(14).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let instances = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                AccountName::parse("work")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        rig.world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await?;
        let checkout = handle.root().join("work/claude/project");
        std::fs::write(checkout.join("uncommitted.txt"), b"kept")?;
        let gitdir = std::fs::read_to_string(checkout.join(".git"))?;
        let admin = std::path::PathBuf::from(
            gitdir
                .trim()
                .strip_prefix("gitdir: ")
                .ok_or("the checkout names no administrative directory")?,
        );
        let backlink = admin.join("gitdir");
        std::fs::write(&backlink, b"/sandbox/work/claude/old/project/.git\n")?;
        rig.world
            .session
            .setup_worktrees(&target.sandbox, &instances, &CancellationToken::new())
            .await?;
        assert!(checkout.join("uncommitted.txt").is_file());
        assert!(checkout.join(".git").is_file());
        let repaired = std::fs::read_to_string(&backlink)?;
        assert_eq!(
            repaired.trim(),
            std::fs::canonicalize(checkout.join(".git"))?
                .to_string_lossy()
                .to_string()
        );
        let listed = handle.git(&["worktree", "list", "--porcelain"]).await?;
        assert!(listed.contains("work/claude/project"), "{listed}");
        Ok(())
    })
    .await
}
