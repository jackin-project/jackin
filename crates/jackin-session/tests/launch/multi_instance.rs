// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, AgentInstance, AgentInstances, ConfigEdit, CredentialSource,
    FileMode, HostPath, Provider, ProviderName, SandboxPath, Secret, sandbox_layout,
};
use jackin_openshell::{Gateway, ProviderMaterial, ProviderProfile, ProviderSpec};
use jackin_testkit::Rig;
use tokio_util::sync::CancellationToken;

use crate::support::{guard, multi_agent_rig, ready, running};

const OMP_ACCOUNT: &str = "zai-work";

async fn seed_omp_provider(rig: &mut Rig) -> Result<(), Box<dyn std::error::Error>> {
    let path = HostPath::parse(&rig.home.as_path().join("zai.key"))?;
    rig.world
        .fs
        .write_new(&path, b"test-key", FileMode(0o600))
        .await?;
    rig.world
        .config
        .edit(vec![ConfigEdit::AddAccount(Account::new(
            AccountName::parse(OMP_ACCOUNT)?,
            Provider::Zai,
            CredentialSource::File { path },
        )?)])
        .await?;
    rig.config = rig.world.config.load().await?;
    let tuple = jackin_core::lookup_tuple(Agent::Omp, Provider::Zai)?;
    let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
    rig.gateway().ensure_profile(&profile).await?;
    rig.gateway()
        .put_provider(&ProviderSpec {
            name: ProviderName::for_account(&AccountName::parse(OMP_ACCOUNT)?, Agent::Omp),
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
            Agent::ClaudeCode,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
        AgentInstance::new(
            Agent::Omp,
            Provider::Zai,
            AccountName::parse(OMP_ACCOUNT)?,
            "glm-5.3".to_owned(),
        ),
    ])?)
}

fn codex_instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::Codex,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
        AgentInstance::new(
            Agent::ClaudeCode,
            Provider::Zai,
            AccountName::parse(OMP_ACCOUNT)?,
            "glm-5.3".to_owned(),
        ),
    ])?)
}

#[tokio::test]
async fn two_instance_launch_leaves_non_lead_ready_and_supervises_only_lead()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let mut rig = multi_agent_rig().await?;
        seed_omp_provider(&mut rig).await?;
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
                    .any(|arg| arg.contains("/sandbox/.jackin/run/claude/output"))
            })
            .count();
        assert_eq!(lead_starts, 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn restart_omp_stops_omp_only_and_starts_no_process() -> Result<(), Box<dyn std::error::Error>>
{
    guard(async {
        let mut rig = multi_agent_rig().await?;
        seed_omp_provider(&mut rig).await?;
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
        session
            .restart_instance(&target.sandbox, Agent::Omp, &CancellationToken::new())
            .await?;
        assert_eq!(count(&handle, "omp"), 0);
        assert_eq!(count(&handle, "claude"), 0);
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
        Ok(())
    })
    .await
}

#[tokio::test]
async fn two_instance_launch_writes_codex_env_like_single_instance()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let mut rig = multi_agent_rig().await?;
        seed_omp_provider(&mut rig).await?;
        let request = rig.request(8)?.instances(codex_instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        let path = sandbox_layout::env_file(&crate::support::launch_id(8));
        let env = handle.file(&path).ok_or("no codex env")?;
        assert!(!env.is_empty());
        assert_eq!(handle.file_mode(&path), Some(FileMode(0o600)));
        let single = multi_agent_rig().await?;
        ready(single.launch_observed(1).await?)?;
        let single_handle = single.handle(1).await?;
        let single_path = sandbox_layout::env_file(&crate::support::launch_id(1));
        assert_eq!(
            env,
            single_handle.file(&single_path).ok_or("no single env")?
        );
        assert_eq!(
            handle.file_mode(&path),
            single_handle.file_mode(&single_path)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn failed_codex_provider_publication_cleans_up_only_codex_and_preserves_sibling_providers()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let mut rig = multi_agent_rig().await?;
        let work = AccountName::parse("work")?;
        let path = HostPath::parse(&rig.world.home().as_path().join("zai.key"))?;
        rig.world
            .fs
            .write_new(&path, b"ZAI-FIXTURE-KEY", FileMode(0o600))
            .await?;
        let account = Account::new(work.clone(), Provider::Zai, CredentialSource::File { path })?;
        let edits = vec![
            ConfigEdit::RemoveAccountClearingBindings(work),
            ConfigEdit::AddAccount(account.clone()),
        ];
        for edit in &edits {
            rig.config.apply(edit.clone())?;
        }
        rig.world.config.edit(edits).await?;
        rig.world
            .session
            .sync_account(&account, Agent::Omp, &CancellationToken::new())
            .await?;
        let other = Account::new(
            AccountName::parse("other")?,
            Provider::Zai,
            account.credential().clone(),
        )?;
        rig.world
            .config
            .edit(vec![ConfigEdit::AddAccount(other.clone())])
            .await?;
        rig.world
            .session
            .sync_account(&other, Agent::Omp, &CancellationToken::new())
            .await?;
        let omp = ProviderName::for_account(account.name(), Agent::Omp);
        let codex = ProviderName::for_account(account.name(), Agent::Codex);
        let other_provider = ProviderName::for_account(other.name(), Agent::Omp);
        let original_omp = rig.gateway().provider(&omp);
        let original_other = rig.gateway().provider(&other_provider);
        rig.gateway()
            .inject(jackin_testkit::Fault::PutProviderFailsAfterWrite {
                error: jackin_openshell::GatewayError::Unreachable {
                    endpoint: "gateway".to_owned(),
                    message: "connection reset".to_owned(),
                },
            });
        let members = AgentInstances::new(vec![
            AgentInstance::new(
                Agent::Codex,
                Provider::Zai,
                account.name().clone(),
                "glm-5.3".to_owned(),
            ),
            AgentInstance::new(
                Agent::ClaudeCode,
                Provider::Zai,
                account.name().clone(),
                "glm-5.3".to_owned(),
            ),
        ])?;
        let outcome = rig
            .run(
                rig.request(42)?.instances(members),
                CancellationToken::new(),
            )
            .await?;

        let failed = outcome.result.err().ok_or("the launch went through")?;
        assert_eq!(failed.step, jackin_session::LaunchStep::Provider);
        assert!(rig.gateway().provider(&codex).is_none());
        assert_eq!(rig.gateway().provider(&omp), original_omp);
        assert_eq!(rig.gateway().provider(&other_provider), original_other);
        assert!(rig.gateway().sandboxes().is_empty());
        Ok(())
    })
    .await
}
