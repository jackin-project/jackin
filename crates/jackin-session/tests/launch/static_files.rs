// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, AgentInstance, AgentInstances, ConfigEdit, CredentialSource,
    FileMode, HostPath, Provider, ProviderName, SandboxPath, Secret,
};
use jackin_openshell::{Gateway, ProviderMaterial, ProviderProfile, ProviderSpec};
use jackin_session::plan_launch;
use jackin_testkit::{Rig, RoleFixture, SandboxHandle};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, ready};

const ROLE: &str = "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n\n[agents.omp]\ninstructions = \"instructions/omp.md\"\n\n[docker]\nenabled = false\n";

fn role() -> Result<RoleFixture, Box<dyn std::error::Error>> {
    Ok(RoleFixture::new("sentinel")?.manifest_text(ROLE.to_owned()))
}

async fn add_zai_account(rig: &Rig) -> Result<AccountName, Box<dyn std::error::Error>> {
    let path = HostPath::parse(&rig.home.as_path().join("zai.key"))?;
    rig.world
        .fs
        .write_new(&path, b"test-key", FileMode(0o600))
        .await?;
    let name = AccountName::parse("zai-work")?;
    rig.world
        .config
        .edit(vec![ConfigEdit::AddAccount(Account::new(
            name.clone(),
            Provider::Zai,
            CredentialSource::File { path },
        )?)])
        .await?;
    Ok(name)
}

async fn seed_provider(
    rig: &Rig,
    agent: Agent,
    account: &AccountName,
) -> Result<(), Box<dyn std::error::Error>> {
    let tuple = jackin_core::lookup_tuple(agent, Provider::Zai)?;
    let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
    rig.gateway().ensure_profile(&profile).await?;
    rig.gateway()
        .put_provider(&ProviderSpec {
            name: ProviderName::for_account(account, agent),
            profile: profile.id,
            keys: jackin_core::CredentialKeys {
                access_token: tuple.credential_env().unwrap_or_default(),
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

fn instance(agent: Agent, account: &AccountName) -> AgentInstance {
    AgentInstance::new(agent, Provider::Zai, account.clone(), "glm-5.3".to_owned())
}

async fn launch_pair(
    id: u8,
    lead: Agent,
    secondary: Agent,
) -> Result<(Rig, SandboxHandle), Box<dyn std::error::Error>> {
    let rig = Rig::builder().role(role()?).build().await?;
    let account = add_zai_account(&rig).await?;
    seed_provider(&rig, lead, &account).await?;
    seed_provider(&rig, secondary, &account).await?;
    let instances = AgentInstances::new(vec![
        instance(lead, &account),
        instance(secondary, &account),
    ])?;
    let config = rig.world.config.load().await?;
    let plan = plan_launch(&config, &rig.loaded, rig.request(id)?.instances(instances))?;
    let outcome = rig
        .run_planned(&rig.world.session, plan, CancellationToken::new())
        .await;
    let target = ready(outcome)?;
    let handle = rig
        .gateway()
        .sandbox(&target.sandbox)
        .ok_or("sandbox handle")?;
    Ok((rig, handle))
}

fn assert_file(handle: &SandboxHandle, path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = SandboxPath::parse(path)?;
    let bytes = handle
        .file(&path)
        .ok_or_else(|| format!("no file at {path}"))?;
    assert_eq!(handle.file_mode(&path), Some(FileMode(0o600)), "{path}");
    Ok(bytes)
}

fn assert_absent(handle: &SandboxHandle, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = SandboxPath::parse(path)?;
    assert!(handle.file(&path).is_none(), "{path}");
    Ok(())
}

#[tokio::test]
async fn claude_lead_and_omp_secondary_receive_equivalent_static_files() -> TestResult {
    guard(async {
        let (_rig, handle) = launch_pair(31, Agent::ClaudeCode, Agent::Omp).await?;
        assert_file(&handle, "/sandbox/.claude/settings.json")?;
        assert_eq!(
            assert_file(&handle, "/sandbox/.claude/.credentials.json")?,
            b"{}"
        );
        let config = String::from_utf8(assert_file(
            &handle,
            "/sandbox/.jackin/homes/omp/.omp/config.toml",
        )?)?;
        assert!(config.contains("baseUrl: https://api.z.ai/api/anthropic"));
        assert_file(&handle, "/sandbox/.jackin/homes/omp/.omp/auth.json")?;
        let models = String::from_utf8(assert_file(
            &handle,
            "/sandbox/.jackin/homes/omp/.omp/agent/models.yml",
        )?)?;
        assert!(models.contains("api: anthropic-messages"));
        for path in [
            "/sandbox/.omp/config.toml",
            "/sandbox/.omp/auth.json",
            "/sandbox/.omp/agent/models.yml",
            "/sandbox/.jackin/homes/claude/.claude/settings.json",
            "/sandbox/.jackin/homes/claude/.claude/.credentials.json",
            "/sandbox/.codex/config.toml",
            "/sandbox/.jackin/homes/codex/.codex/config.toml",
        ] {
            assert_absent(&handle, path)?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn claude_lead_and_codex_secondary_receive_equivalent_static_files() -> TestResult {
    guard(async {
        let (_rig, handle) = launch_pair(32, Agent::ClaudeCode, Agent::Codex).await?;
        assert_file(&handle, "/sandbox/.claude/settings.json")?;
        assert_eq!(
            assert_file(&handle, "/sandbox/.claude/.credentials.json")?,
            b"{}"
        );
        let config = String::from_utf8(assert_file(
            &handle,
            "/sandbox/.jackin/homes/codex/.codex/config.toml",
        )?)?;
        assert!(config.contains("model_provider = \"zai\""));
        assert!(config.contains("[projects.\"/sandbox/work/codex/project\"]"));
        assert!(!config.contains("[projects.\"/sandbox/project\"]"));
        for path in [
            "/sandbox/.codex/config.toml",
            "/sandbox/.codex/auth.json",
            "/sandbox/.jackin/homes/codex/.codex/auth.json",
            "/sandbox/.jackin/homes/claude/.claude/settings.json",
            "/sandbox/.jackin/homes/claude/.claude/.credentials.json",
            "/sandbox/.omp/config.toml",
            "/sandbox/.jackin/homes/omp/.omp/config.toml",
        ] {
            assert_absent(&handle, path)?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn omp_lead_and_claude_secondary_receive_equivalent_static_files() -> TestResult {
    guard(async {
        let (_rig, handle) = launch_pair(33, Agent::Omp, Agent::ClaudeCode).await?;
        assert_file(&handle, "/sandbox/.omp/config.toml")?;
        assert_file(&handle, "/sandbox/.omp/auth.json")?;
        assert_file(&handle, "/sandbox/.omp/agent/models.yml")?;
        assert_file(
            &handle,
            "/sandbox/.jackin/homes/claude/.claude/settings.json",
        )?;
        assert_eq!(
            assert_file(
                &handle,
                "/sandbox/.jackin/homes/claude/.claude/.credentials.json"
            )?,
            b"{}"
        );
        for path in [
            "/sandbox/.claude/settings.json",
            "/sandbox/.claude/.credentials.json",
            "/sandbox/.jackin/homes/omp/.omp/config.toml",
            "/sandbox/.jackin/homes/omp/.omp/auth.json",
            "/sandbox/.jackin/homes/omp/.omp/agent/models.yml",
            "/sandbox/.codex/config.toml",
            "/sandbox/.jackin/homes/codex/.codex/config.toml",
        ] {
            assert_absent(&handle, path)?;
        }
        Ok(())
    })
    .await
}
