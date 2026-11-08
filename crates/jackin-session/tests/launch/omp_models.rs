// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, AgentInstance, AgentInstances, ConfigEdit, CredentialSource,
    FileMode, HostPath, Provider, ProviderName, SandboxPath, Secret,
};
use jackin_openshell::{Gateway, ProviderMaterial, ProviderProfile, ProviderSpec};
use jackin_session::plan_launch;
use jackin_testkit::Rig;
use tokio_util::sync::CancellationToken;

use crate::support::{TestResult, guard, launch_rig, ready};

const MODELS: &str = "providers:\n  zai:\n    baseUrl: https://api.z.ai/api/coding/paas/v4\n    apiKey: ZAI_API_KEY\n";

fn lead_path() -> Result<SandboxPath, Box<dyn std::error::Error>> {
    Ok(SandboxPath::parse("/sandbox/.omp/agent/models.yml")?)
}

fn home_path() -> Result<SandboxPath, Box<dyn std::error::Error>> {
    Ok(SandboxPath::parse(
        "/sandbox/.jackin/homes/omp/.omp/agent/models.yml",
    )?)
}

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
            "gpt-5.1-codex".to_owned(),
        ),
        AgentInstance::new(
            Agent::Omp,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
    ])?)
}

fn assert_models(handle: &jackin_testkit::SandboxHandle, path: &SandboxPath) -> TestResult {
    let text = handle.file(path).ok_or("no models.yml")?;
    assert_eq!(text, MODELS.as_bytes());
    assert_eq!(handle.file_mode(path), Some(FileMode(0o600)));
    Ok(())
}

#[tokio::test]
async fn a_non_lead_omp_instance_gets_the_lead_models_file_and_a_home_copy() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        seed_omp_provider(&rig).await?;
        let request = rig.request(21)?.instances(instances()?);
        let outcome = rig.run(request, CancellationToken::new()).await?;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        assert_models(&handle, &lead_path()?)?;
        assert_models(&handle, &home_path()?)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_omp_lead_launch_gets_the_lead_models_file_and_no_home_copy() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let key = HostPath::parse(&rig.home.as_path().join("zai.key"))?;
        rig.world
            .fs
            .write_new(&key, b"KEY-ZAI-53", FileMode(0o600))
            .await?;
        let account = Account::new(
            AccountName::parse("zai-work")?,
            Provider::Zai,
            CredentialSource::File { path: key },
        )?;
        rig.world
            .config
            .edit(vec![ConfigEdit::AddAccount(account)])
            .await?;
        let config = rig.world.config.load().await?;
        let request = rig.request(22)?.tuple(Agent::Omp, Provider::Zai);
        let plan = plan_launch(&config, &rig.loaded, request)?;
        let outcome = rig
            .run_planned(&rig.world.session, plan, CancellationToken::new())
            .await;
        let target = ready(outcome)?;
        let handle = rig
            .gateway()
            .sandbox(&target.sandbox)
            .ok_or("sandbox handle")?;
        assert_models(&handle, &lead_path()?)?;
        assert!(handle.file(&home_path()?).is_none());
        assert_eq!(
            rig.gateway().providers(),
            [ProviderName::parse("jackin-zai-work-omp")?]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_codex_only_launch_writes_no_models_file() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(23).await?)?;
        let handle = rig.handle(23).await?;
        assert!(handle.file(&lead_path()?).is_none());
        assert!(handle.file(&home_path()?).is_none());
        Ok(())
    })
    .await
}
