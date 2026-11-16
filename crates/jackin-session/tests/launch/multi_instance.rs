// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AccountName, Agent, AgentInstance, AgentInstances, FileMode, Provider, ProviderName, SandboxPath, Secret};
use jackin_openshell::{Gateway, ProviderMaterial, ProviderProfile, ProviderSpec};
use jackin_testkit::Rig;
use tokio_util::sync::CancellationToken;

use crate::support::{guard, launch_rig, ready, running};

async fn seed_omp_provider(rig: &Rig) -> Result<(), Box<dyn std::error::Error>> {
    let tuple = jackin_core::lookup_tuple(Agent::Omp, Provider::Zai)?;
    let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
    rig.gateway().ensure_profile(&profile).await?;
    rig.gateway().put_provider(&ProviderSpec { name: ProviderName::for_account(&AccountName::parse("work")?, Agent::Omp), profile: profile.id, keys: jackin_core::CredentialKeys { access_token: "ZAI_API_KEY", refresh_token: "", account_id: "", id_token: "", token_url: "" }, material: ProviderMaterial::StaticKey { key: Secret::new("test-key".to_owned()) } }).await?;
    Ok(())
}

fn instances() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(Agent::Codex, Provider::Codex, AccountName::parse("work")?, "gpt-5.1-codex".to_owned()),
        AgentInstance::new(Agent::Omp, Provider::Zai, AccountName::parse("work")?, "glm-5.3".to_owned()),
    ])?)
}

#[tokio::test]
async fn two_instance_launch_starts_non_lead_detached_and_supervises_only_lead() -> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig).await?;
        let request = rig.request(7)?.instances(instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox handle")?;
        assert_eq!(handle.supervisor(), Some(running(1)?));
        let sh = SandboxPath::from_static("/bin/sh");
        let runs = handle.runs(&sh);
        let start = runs.iter().find(|run| run.args.iter().any(|arg| arg.contains("/sandbox/.jackin/run/omp/output"))).ok_or("no non-lead start exec record")?;
        assert_eq!(start.workdir.as_ref().map(|p| p.as_str()), Some("/sandbox/work/omp/project"));
        assert!(start.args.iter().any(|arg| arg.contains("omp")), "{start:?}");
        assert!(start.env.values().any(|v| v == "/sandbox/.jackin/homes/omp"), "{start:?}");
        let lead_starts = runs.iter().filter(|run| run.args.iter().any(|arg| arg.contains("/sandbox/.jackin/run/codex/output"))).count();
        assert_eq!(lead_starts, 0);
        Ok(())
    }).await
}

#[tokio::test]
async fn two_instance_launch_writes_codex_placeholder_auth_like_single_instance() -> Result<(), Box<dyn std::error::Error>> {
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig).await?;
        let request = rig.request(8)?.instances(instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig.gateway().sandbox(&target.sandbox).ok_or("sandbox handle")?;
        let spec = Agent::Codex.spec();
        let auth = handle.file(&spec.auth_file.path).ok_or("no codex auth")?;
        assert!(!auth.is_empty());
        assert_eq!(handle.file_mode(&spec.auth_file.path), Some(FileMode(0o600)));
        let single = launch_rig().await?;
        ready(single.launch_observed(1).await?)?;
        let single_handle = single.handle(1).await?;
        assert_eq!(auth, single_handle.file(&spec.auth_file.path).ok_or("no single auth")?);
        assert_eq!(handle.file_mode(&spec.auth_file.path), single_handle.file_mode(&spec.auth_file.path));
        Ok(())
    }).await
}
