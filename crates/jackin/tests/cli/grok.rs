// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{
    AccountName, Agent, AgentInstance, AgentInstances, EnvName, Provider, ProviderName,
    SandboxLabels, SandboxPath,
};
use jackin_testkit::{Rig, RoleFixture, TestResult, guard, launch_id};

use super::support::{Failure, Harness, Ran, failed};

const ACCESS: &str = "FAKE-GROK-ACCESS-CLI";
const REFRESH: &str = "FAKE-GROK-REFRESH-CLI";
const CLIENT: &str = "grok-client-cli";
const CANARY: &str = "FAKE-XAI-API-KEY-CANARY";
const ROLE: &str = "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.grok]\ninstructions = \"instructions/grok.md\"\n\n[docker]\nenabled = false\n";

async fn grok_launch(model: Option<&str>, id: u8) -> Result<(Rig, Ran), Failure> {
    let role = RoleFixture::new("sentinel")?.manifest_text(ROLE.to_owned());
    let rig = Rig::builder().role(role).build().await?;
    let login = rig.home.as_path().join("grok-auth.json");
    let document = serde_json::json!({
        "https://auth.x.ai::issuer": {
            "key": ACCESS,
            "refresh_token": REFRESH,
            "oidc_client_id": CLIENT,
            "oidc_issuer": "https://auth.x.ai",
            "auth_mode": "oidc",
        }
    });
    std::fs::write(&login, document.to_string())?;
    let mut harness = Harness::new(&rig.world);
    harness.cwd = rig.repo.root().clone();
    harness
        .env
        .push(("XAI_API_KEY".to_owned(), CANARY.to_owned()));
    harness.write_config(&format!(
        "{}\n[accounts.grok-work]\nprovider = \"grok\"\ncredential = {{ source = \"host-login\", path = \"{}\" }}\n",
        rig.config.render(),
        login.display()
    ))?;
    let launch = launch_id(id).to_string();
    let mut args = vec![
        "load",
        "--agent",
        "grok",
        "--provider",
        "grok",
        "--account",
        "grok-work",
        "--launch-id",
        &launch,
    ];
    if let Some(model) = model {
        args.extend(["--model", model]);
    }
    let ran = harness.run(&args).await?;
    Ok((rig, ran))
}

fn assert_no_raw_credentials(rig: &Rig, ran: &Ran) -> TestResult {
    for secret in [ACCESS, REFRESH, CANARY] {
        assert!(!ran.stdout.contains(secret));
        assert!(!ran.stderr.contains(secret));
        for handle in rig.gateway().sandboxes() {
            assert!(
                handle
                    .environment()
                    .values()
                    .all(|value| !value.contains(secret))
            );
            assert!(
                handle
                    .files()
                    .values()
                    .all(|bytes| { !String::from_utf8_lossy(bytes).contains(secret) })
            );
        }
    }
    Ok(())
}

async fn assert_t03_launch(model: Option<&str>, id: u8) -> TestResult {
    let (rig, ran) = grok_launch(model, id).await?;
    ran.ok()?;
    let boxes = rig.gateway().sandboxes();
    assert_eq!(boxes.len(), 1);
    let handle = &boxes[0];
    let labels = SandboxLabels::from_labels(&handle.labels())?;
    let expected = AgentInstances::single(AgentInstance::new(
        Agent::GrokBuild,
        Provider::Grok,
        AccountName::parse("grok-work")?,
        "grok-4.7".to_owned(),
    ));
    assert_eq!(
        labels.planned.agents.agents_label(),
        expected.agents_label()
    );
    let instances: AgentInstances = serde_json::from_slice(
        &handle
            .file(&SandboxPath::from_static("/sandbox/.jackin/agents.json"))
            .ok_or("missing agents.json")?,
    )?;
    assert_eq!(instances.lead().model, "grok-4.7");
    assert_eq!(
        handle.main().program.as_str(),
        "/usr/local/bin/jackin-start"
    );
    assert_eq!(handle.main().args, ["grok", "--trust"]);
    let environment = handle.environment();
    assert!(!environment.contains_key(&EnvName::parse("XAI_API_KEY")?));
    let config = String::from_utf8(
        handle
            .file(&SandboxPath::from_static("/sandbox/.grok/config.toml"))
            .ok_or("missing Grok config")?,
    )?;
    assert!(config.contains("default = \"grok-4.7\""));
    assert!(config.contains("disable_api_key_auth = true"));
    assert!(config.contains("GROK_SESSION_TOKEN"));
    assert!(!config.contains("XAI_API_KEY"));
    assert!(!config.contains("refresh_token"));
    assert!(
        handle
            .file(&SandboxPath::from_static("/sandbox/.grok/auth.json"))
            .is_none()
    );
    let provider = rig
        .gateway()
        .provider(&ProviderName::for_account(
            &AccountName::parse("grok-work")?,
            Agent::GrokBuild,
        ))
        .ok_or("missing Grok provider")?;
    assert_eq!(provider.profile.as_str(), "grok");
    assert_eq!(provider.access_key, "GROK_SESSION_TOKEN");
    assert_eq!(provider.access_token, ACCESS);
    assert_eq!(provider.refresh_token, REFRESH);
    assert_eq!(provider.client_id, CLIENT);
    assert!(provider.additional_credentials.is_empty());
    assert_no_raw_credentials(&rig, &ran)?;
    Ok(())
}

#[tokio::test]
async fn t03_cli_accepts_default_and_explicit_models_with_generic_oauth_publication() -> TestResult
{
    guard(async {
        assert_t03_launch(None, 51).await?;
        assert_t03_launch(Some("grok-4.7"), 52).await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn t03_cli_rejects_the_effective_reply_model_without_effects() -> TestResult {
    guard(async {
        let (rig, ran) = grok_launch(Some("grok-4.7-build"), 53).await?;
        failed(&ran, ExitClass::Usage, &[])?;
        assert_eq!(
            ran.stderr,
            "error: model grok-4.7-build is not allowed for agent grok with provider grok\n"
        );
        assert!(rig.world.engine.builds().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert!(rig.gateway().sandboxes().is_empty());
        assert_no_raw_credentials(&rig, &ran)?;
        Ok(())
    })
    .await
}
