// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, FileMode, Provider, ProviderName,
    SandboxPath, Secret,
};
use jackin_openshell::{Gateway, ProviderMaterial, ProviderProfile, ProviderSpec};
use jackin_testkit::Rig;
use tokio_util::sync::CancellationToken;

use crate::support::{guard, launch_rig, ready, running};

async fn seed_omp_provider(rig: &Rig) -> Result<(), Box<dyn std::error::Error>> {
    let tuple = jackin_core::lookup_tuple(Agent::Omp, Provider::Zai)?;
    let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
    rig.gateway().ensure_profile(&profile).await?;
    rig.gateway()
        .put_provider(&ProviderSpec {
            name: ProviderName::for_account(&AccountName::parse("work")?, Agent::Omp),
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

#[tokio::test]
async fn two_instance_launch_leaves_non_lead_ready_and_supervises_only_lead()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig).await?;
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
async fn restart_omp_stops_omp_only_and_starts_no_process() -> Result<(), Box<dyn std::error::Error>>
{
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig).await?;
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
        Ok(())
    })
    .await
}

#[tokio::test]
async fn two_instance_launch_writes_codex_placeholder_auth_like_single_instance()
-> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig).await?;
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
