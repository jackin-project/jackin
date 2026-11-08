// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, FileMode, ModelName, Provider, ProviderName,
    SandboxPath, Secret, sandbox_layout,
};
use jackin_openshell::{Gateway, ProviderMaterial, ProviderProfile, ProviderSpec};
use jackin_session::{LaunchOutcome, plan_launch};
use jackin_testkit::{Fault, Rig, Trigger};
use tokio_util::sync::CancellationToken;

use crate::support::{add_zai_account, guard, launch_rig, ready, running};

pub(crate) async fn seed_omp_provider(
    rig: &Rig,
    account: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let tuple = jackin_core::lookup_tuple(Agent::Omp, Provider::Zai)?;
    let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
    rig.gateway().ensure_profile(&profile).await?;
    rig.gateway()
        .put_provider(&ProviderSpec {
            name: ProviderName::for_account(&AccountName::parse(account)?, Agent::Omp),
            profile: profile.id,
            keys: jackin_core::CredentialKeys {
                access_token: "ZAI_API_KEY",
                refresh_token: "",
                account_id: "",
                id_token: "",
                token_url: "",
            },
            material: ProviderMaterial::StaticKey {
                key: Secret::new("test-key".to_owned()),
            },
        })
        .await?;
    Ok(())
}

fn instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
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
    ])?)
}

fn omp_lead_instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::Omp,
            Provider::Zai,
            AccountName::parse("omp")?,
            ModelName::parse("glm-5.3")?,
        ),
        AgentInstance::new(
            Agent::Codex,
            Provider::Codex,
            AccountName::parse("work")?,
            ModelName::parse("gpt-5.1-codex")?,
        ),
    ])?)
}

#[tokio::test]
async fn two_instance_launch_leaves_non_lead_ready_and_supervises_only_lead()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig, "work").await?;
        let request = rig.request(7)?.instances(instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        assert_eq!(handle.supervisor(), Some(running(1)?));
        let sh = SandboxPath::from_static("/bin/sh");
        let runs = handle.runs(&sh);
        assert_eq!(
            runs.iter()
                .filter(|run| run
                    .args
                    .iter()
                    .any(|arg| arg.contains("/sandbox/.jackin/run/omp/output")))
                .count(),
            0
        );
        assert!(
            !runs.iter().any(|run| run
                .args
                .iter()
                .any(|arg| arg.contains("setsid") || arg.contains("script -qec"))),
            "{runs:?}"
        );
        assert!(handle.root().join(".jackin/run/omp").is_dir());
        assert!(handle.root().join(".jackin/task/status").is_dir());
        assert!(handle.root().join(".jackin/task").is_dir());
        let lead_starts = runs
            .iter()
            .filter(|run| {
                run.args
                    .iter()
                    .any(|arg| arg.contains("/sandbox/.jackin/run/codex/output"))
            })
            .count();
        assert_eq!(lead_starts, 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn selected_non_sorted_lead_is_labeled_and_owns_the_primary_checkout()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let rig = launch_rig().await?;
        let config = add_zai_account(&rig, "omp").await?;
        seed_omp_provider(&rig, "omp").await?;
        let instances = omp_lead_instances()?;
        let expected_json = instances.agents_json()?;
        let plan = plan_launch(&config, &rig.loaded, rig.request(31)?.instances(instances))?;
        let target = ready(
            rig.run_planned(&rig.world.session, plan, CancellationToken::new())
                .await,
        )?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let summary = rig
            .world
            .session
            .list(&rig.world.cancel)
            .await?
            .sandboxes
            .into_iter()
            .find(|summary| summary.name == target.sandbox)
            .ok_or("sandbox summary")?;
        let metadata = rig
            .world
            .session
            .agent_metadata(&summary, &rig.world.cancel)
            .await?;
        assert_eq!(summary.labels.planned.lead_agent, Agent::Omp);
        assert_eq!(metadata.lead_agent(), Agent::Omp);
        assert_eq!(metadata.instances.as_slice()[0].agent, Agent::Codex);
        assert_eq!(
            handle.file(&SandboxPath::from_static("/sandbox/.jackin/agents.json")),
            Some(expected_json.into_bytes())
        );
        let worktree = handle.root().join("work/codex/project");
        assert!(worktree.join(".git").is_file());
        assert!(!handle.root().join("work/omp/project").exists());
        let gitdir = std::fs::read_to_string(worktree.join(".git"))?;
        let gitdir = gitdir.trim().strip_prefix("gitdir: ").ok_or("gitdir")?;
        let head = std::fs::read_to_string(format!("{gitdir}/HEAD"))?;
        assert_eq!(head.trim(), "ref: refs/heads/agent/codex");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_partial_multi_agent_worktree_setup_resumes_without_losing_agent_work()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig, "work").await?;
        let request = rig.request(23)?.instances(instances()?);
        let cancellation = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::ProgramArgs {
                program: SandboxPath::from_static("/usr/bin/git"),
                args: vec![
                    "-C".to_owned(),
                    "/sandbox/work/omp/project".to_owned(),
                    "rev-parse".to_owned(),
                    "--show-toplevel".to_owned(),
                    "--symbolic-full-name".to_owned(),
                    "HEAD".to_owned(),
                ],
            },
            token: cancellation.clone(),
        });
        let first = rig.run(request, cancellation.clone()).await?;
        let Ok(LaunchOutcome::Cancelled(candidate)) = first.result else {
            return Err(format!("expected a cancelled launch, saw {:?}", first.result).into());
        };
        assert!(cancellation.is_cancelled());
        assert_eq!(candidate.launch_id, jackin_testkit::launch_id(23));
        let sandbox = rig.plan(23)?.sandbox_name().clone();
        let handle = rig.gateway().sandbox(&sandbox).ok_or("sandbox handle")?;
        let marker = sandbox_layout::uploaded_marker(&jackin_testkit::launch_id(23));
        let tree_marker = sandbox_layout::project_tree_marker(&jackin_testkit::launch_id(23));
        assert!(handle.file(&marker).is_none());
        assert!(handle.file(&tree_marker).is_some());
        let omp = handle.root().join("work/omp/project");
        assert!(omp.join(".git").is_file());
        std::fs::write(omp.join("resume-sentinel.txt"), b"preserve this work")?;

        let second = rig
            .run(
                rig.request(23)?.instances(instances()?),
                CancellationToken::new(),
            )
            .await?;
        let target = ready(second)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("resumed sandbox handle")?;
        assert_eq!(
            std::fs::read(omp.join("resume-sentinel.txt"))?,
            b"preserve this work"
        );
        assert!(handle.root().join(".jackin/homes/omp").is_dir());
        assert!(handle.file(&marker).is_some());
        let gitdir = std::fs::read_to_string(handle.root().join("work/omp/project/.git"))?;
        let gitdir = gitdir.trim().strip_prefix("gitdir: ").ok_or("gitdir")?;
        let head = std::fs::read_to_string(format!("{gitdir}/HEAD"))?;
        assert_eq!(head.trim(), "ref: refs/heads/agent/omp");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn setup_refuses_a_worktree_that_has_been_switched_to_another_branch()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig, "work").await?;
        let outcome = rig
            .run(
                rig.request(24)?.instances(instances()?),
                CancellationToken::new(),
            )
            .await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let worktree = handle.root().join("work/omp/project");
        let status = tokio::process::Command::new("/usr/bin/git")
            .args([
                "-C",
                worktree.to_str().ok_or("worktree path")?,
                "switch",
                "-c",
                "agent/foreign",
            ])
            .env_clear()
            .status()
            .await?;
        assert!(status.success());
        let result = rig
            .world
            .session
            .setup_worktrees(
                &target.sandbox,
                &instances()?,
                Agent::Codex,
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(
            result,
            Err(jackin_openshell::GatewayError::Protocol { .. })
        ));
        let gitdir = std::fs::read_to_string(worktree.join(".git"))?;
        let gitdir = gitdir.trim().strip_prefix("gitdir: ").ok_or("gitdir")?;
        let head = std::fs::read_to_string(format!("{gitdir}/HEAD"))?;
        assert_eq!(head.trim(), "ref: refs/heads/agent/foreign");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn restart_omp_stops_omp_only_and_starts_no_process() -> Result<(), Box<dyn std::error::Error>>
{
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig, "work").await?;
        let request = rig.request(9)?.instances(instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let sh = SandboxPath::from_static("/bin/sh");
        let count = |handle: &jackin_testkit::SandboxHandle, agent: &str| {
            handle
                .runs(&sh)
                .iter()
                .filter(|run| {
                    run.args
                        .iter()
                        .any(|arg| arg.contains(&format!("/sandbox/.jackin/run/{agent}/output")))
                })
                .count()
        };
        assert_eq!(count(&handle, "omp"), 0);
        let pkill = SandboxPath::from_static("/usr/bin/pkill");
        let stops_before = handle.runs(&pkill).len();
        let session = rig.session_with(crate::support::limits(30, 20));
        let expected = session
            .agent_metadata_for_sandbox(&target.sandbox, &CancellationToken::new())
            .await?
            .identity;
        session
            .restart_instance(&expected, Agent::Omp, &CancellationToken::new())
            .await?;
        assert_eq!(count(&handle, "omp"), 0);
        assert_eq!(count(&handle, "codex"), 0);
        let stops = handle.runs(&pkill);
        assert_eq!(stops.len(), stops_before + 1);
        assert!(
            stops
                .last()
                .is_some_and(|run| run.args == vec!["-x".to_owned(), "omp".to_owned()]),
            "{stops:?}"
        );
        assert!(!handle.runs(&sh).iter().any(|run| {
            run.args
                .iter()
                .any(|arg| arg.contains("setsid") || arg.contains("script"))
        }));
        handle.add_label("jackin.dev/agents", "00000000000000000000000000000000")?;
        let refused = session
            .restart_instance(&expected, Agent::Omp, &CancellationToken::new())
            .await;
        assert!(
            matches!(
                refused,
                Err(jackin_session::IntegrateError::AgentMetadata(
                    jackin_session::AgentMetadataError::Replaced
                ))
            ),
            "{refused:?}"
        );
        assert_eq!(handle.runs(&pkill).len(), stops_before + 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn two_instance_launch_writes_codex_placeholder_auth_like_single_instance()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig, "work").await?;
        let request = rig.request(8)?.instances(instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let spec = Agent::Codex.spec();
        let auth = handle.file(&spec.auth_file.path).ok_or("no codex auth")?;
        assert!(!auth.is_empty());
        assert_eq!(
            handle.file_mode(&spec.auth_file.path),
            Some(FileMode(0o600))
        );
        let single = launch_rig().await?;
        ready(single.launch_observed(1).await?)?;
        let single_handle = single.handle(1).await?;
        assert_eq!(
            auth,
            single_handle
                .file(&spec.auth_file.path)
                .ok_or("no single auth")?
        );
        assert_eq!(
            handle.file_mode(&spec.auth_file.path),
            single_handle.file_mode(&spec.auth_file.path)
        );
        Ok(())
    })
    .await
}
