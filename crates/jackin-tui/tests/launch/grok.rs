// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::agent::{Agent as ConsoleAgent, Provider as ConsoleProvider};
use jackin_console::domain::tuple::{InstanceChoice, TupleState as ConsoleTupleState};
use jackin_console::launch::{LaunchEvent, LaunchTarget};
use jackin_core::{
    Account, AccountName, Agent, AgentInstances, ConfigEdit, CredentialSource, EnvName, HostPath,
    Provider, ProviderName, SandboxPath,
};
use jackin_testkit::{ConsoleRig, RoleFixture, TestResult, guard};

use super::support::{direct_launch, only};

const ACCESS: &str = "FAKE-GROK-ACCESS-CONSOLE";
const REFRESH: &str = "FAKE-GROK-REFRESH-CONSOLE";
const CLIENT: &str = "grok-client-console";
const CANARY: &str = "FAKE-XAI-CONSOLE-CANARY";
const ROLE: &str = "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.grok]\ninstructions = \"instructions/grok.md\"\n\n[docker]\nenabled = false\n";

#[tokio::test]
async fn t03_console_offer_and_direct_launch_are_ready_with_host_session_auth() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?.manifest_text(ROLE.to_owned());
        let rig = jackin_testkit::Rig::builder().role(role).build().await?;
        let login = HostPath::parse(&rig.home.as_path().join("grok-auth.json"))?;
        let document = serde_json::json!({
            "https://auth.x.ai::issuer": {
                "key": ACCESS,
                "refresh_token": REFRESH,
                "oidc_client_id": CLIENT,
                "oidc_issuer": "https://auth.x.ai",
                "auth_mode": "oidc",
            }
        });
        tokio::fs::write(login.as_path(), document.to_string()).await?;
        rig.world.seed_all(&rig.config).await?;
        rig.world
            .config
            .edit(vec![
                ConfigEdit::AddAccount(Account::new(
                    AccountName::parse("grok-work")?,
                    Provider::Grok,
                    CredentialSource::HostLogin { path: login },
                )?),
                ConfigEdit::AddAccount(Account::new(
                    AccountName::parse("grok-env")?,
                    Provider::Grok,
                    CredentialSource::Env {
                        var: EnvName::parse("XAI_KEY")?,
                    },
                )?),
                ConfigEdit::SetDefaultAccount {
                    account: AccountName::parse("work")?,
                },
            ])
            .await?;
        let mut console = ConsoleRig::builder(&rig.world)
            .size(160, 44)
            .env("XAI_KEY", CANARY)
            .build()
            .await?;
        console.skip_intro();
        let app = console.console().app();
        let offer = app
            .world
            .surfaces
            .tuples
            .iter()
            .find(|offer| {
                offer.agent == ConsoleAgent::GrokBuild && offer.provider == ConsoleProvider::XAi
            })
            .ok_or("missing T03 console offer")?;
        assert_eq!(offer.state, ConsoleTupleState::Supported);
        assert_eq!(offer.reason, None);
        assert_eq!(offer.allowed_models, ["grok-4.7".to_owned()]);
        assert_eq!(offer.default_model, "grok-4.7");
        assert!(!offer.allowed_models.contains(&"grok-4.7-build".to_owned()));
        let tuple = jackin_core::lookup_tuple(Agent::GrokBuild, Provider::Grok)?;
        assert_eq!(tuple.reason, None);
        let workspace = app
            .world
            .workspaces
            .iter()
            .find(|workspace| workspace.name == "platform-backend")
            .map(|workspace| workspace.id)
            .ok_or("missing platform-backend workspace")?;
        let events = direct_launch(
            &app.world,
            &LaunchTarget {
                workspace: Some(workspace),
                role: "sentinel".to_owned(),
                instances: vec![InstanceChoice {
                    agent: ConsoleAgent::GrokBuild,
                    provider: ConsoleProvider::XAi,
                    account: Some("grok-work".to_owned()),
                    model: String::new(),
                }],
                task: None,
            },
        )
        .await?;
        assert!(
            events
                .iter()
                .any(|event| matches!(event, LaunchEvent::Ready(_))),
            "{events:?}"
        );
        let handle = only(&rig)?;
        let instances: AgentInstances = serde_json::from_slice(
            &handle
                .file(&SandboxPath::from_static("/sandbox/.jackin/agents.json"))
                .ok_or("missing agents.json")?,
        )?;
        assert_eq!(instances.lead().agent, Agent::GrokBuild);
        assert_eq!(instances.lead().provider, Provider::Grok);
        assert_eq!(instances.lead().account.as_str(), "grok-work");
        assert_eq!(instances.lead().model, "grok-4.7");
        assert_eq!(handle.main().args, ["grok", "--trust"]);
        let environment = handle.environment();
        assert!(!environment.contains_key(&EnvName::parse("XAI_API_KEY")?));
        assert!(!environment.contains_key(&EnvName::parse("XAI_KEY")?));
        let config = String::from_utf8(
            handle
                .file(&SandboxPath::from_static("/sandbox/.grok/config.toml"))
                .ok_or("missing Grok config")?,
        )?;
        assert!(config.contains("default = \"grok-4.7\""));
        assert!(config.contains("disable_api_key_auth = true"));
        assert!(config.contains("GROK_SESSION_TOKEN"));
        assert!(!config.contains("XAI_API_KEY"));
        assert!(
            handle
                .file(&SandboxPath::from_static("/sandbox/.grok/auth.json"))
                .is_none()
        );
        let provider_name =
            ProviderName::for_account(&AccountName::parse("grok-work")?, Agent::GrokBuild);
        assert_eq!(
            rig.gateway().providers().as_slice(),
            std::slice::from_ref(&provider_name)
        );
        let provider = rig
            .gateway()
            .provider(&provider_name)
            .ok_or("no provider")?;
        assert_eq!(provider.access_key, "GROK_SESSION_TOKEN");
        assert_eq!(provider.access_token, ACCESS);
        assert_eq!(provider.refresh_token, REFRESH);
        assert_eq!(provider.client_id, CLIENT);
        for secret in [ACCESS, REFRESH, CANARY] {
            assert!(environment.values().all(|value| !value.contains(secret)));
            assert!(
                handle
                    .files()
                    .values()
                    .all(|bytes| { !String::from_utf8_lossy(bytes).contains(secret) })
            );
        }
        Ok(())
    })
    .await
}
