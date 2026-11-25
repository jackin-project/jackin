// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use jackin_console::backend::Effect;
use jackin_console::domain::account::{Account, CredentialSource, DetectedKind, UNSAVED_ID};
use jackin_console::domain::agent::{Agent as ConsoleAgent, Provider};
use jackin_console::domain::tuple::InstanceChoice;
use jackin_console::launch::{EnvAnswer, LaunchEvent, LaunchTarget};
use jackin_core::{
    Account as CoreAccount, AccountName, Agent, ConfigEdit, CredentialSource as Credential,
    GatewayName, OpenShellWorkspace, Provider as CoreProvider, ProviderName, RoleName,
    WorkspaceName,
};
use jackin_openshell::Gateway;
use jackin_testkit::{
    ACCOUNT_ID, ConsoleRig, FakeGateway, ScriptedConnector, TestResult, connector_over, guard,
    session_over,
};
use tokio_util::sync::CancellationToken;

use super::support::Lab;

type Out = Result<(), Box<dyn std::error::Error>>;

type Case<'a> = (&'a str, Vec<&'a str>, Vec<ConfigEdit>, Option<&'a str>);

fn account(text: &str) -> Result<AccountName, Box<dyn std::error::Error>> {
    Ok(AccountName::parse(text)?)
}

async fn second_login(lab: &Lab) -> Out {
    let login = CoreAccount::new(
        account("second")?,
        CoreProvider::Codex,
        Credential::CodexLogin {
            home: lab.rig.home.clone(),
            chatgpt_account_id: jackin_core::ChatgptAccountId::parse(ACCOUNT_ID)?,
        },
    )?;
    lab.edit(vec![
        ConfigEdit::AddAccount(login.clone()),
        ConfigEdit::BindWorkspaceAccount {
            workspace: WorkspaceName::parse("platform-backend")?,
            account: account("second")?,
        },
        ConfigEdit::BindRoleAccount {
            role: RoleName::parse("sentinel")?,
            agent: Agent::Codex,
            account: account("second")?,
        },
        ConfigEdit::SetDefaultAccount {
            account: account("second")?,
        },
    ])
    .await?;
    lab.rig
        .world
        .session
        .sync_account(&login, Agent::Codex, &CancellationToken::new())
        .await?;
    Ok(())
}

fn provider_of(name: &str) -> Result<ProviderName, Box<dyn std::error::Error>> {
    Ok(ProviderName::for_account(&account(name)?, Agent::Codex))
}

#[tokio::test]
async fn remove_clears_the_bindings_in_one_write() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        second_login(&lab).await?;
        lab.reopen().await?;
        assert!(
            lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("second")?)
        );
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        registry.accounts.retain(|a| a.id != "second");
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        let config = lab.config().await?;
        assert!(config.account(&account("second")?).is_none());
        assert!(config.default_account(CoreProvider::Codex).is_none());
        let workspace = config
            .workspace(&WorkspaceName::parse("platform-backend")?)
            .ok_or("workspace")?;
        assert!(workspace.bindings.is_empty());
        let role = config.role(&RoleName::parse("sentinel")?).ok_or("role")?;
        assert!(role.bindings.is_empty());
        assert!(
            !lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("second")?)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_new_codex_account_syncs_its_provider() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        let mut added = Account::registered(
            UNSAVED_ID,
            "newlogin",
            Provider::OpenAi,
            CredentialSource::LocalFolder {
                path: lab.rig.home.as_path().display().to_string(),
                detected: DetectedKind::CodexAuthJson,
            },
        );
        added.default_for_provider = true;
        registry.accounts.push(added);
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        let config = lab.config().await?;
        let saved = config.account(&account("newlogin")?).ok_or("account")?;
        assert_eq!(
            saved.chatgpt_login().map(|l| l.account_id.to_string()),
            Some(ACCOUNT_ID.to_owned())
        );
        assert_eq!(
            config.default_account(CoreProvider::Codex),
            Some(&account("newlogin")?)
        );
        assert!(
            lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("newlogin")?)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_new_account_with_a_typed_purpose_or_endpoint_is_refused_and_writes_nothing() -> TestResult
{
    guard(async {
        for typed in ["purpose", "endpoint", "nothing"] {
            let mut lab = Lab::open().await?;
            let before = lab.console.config_text().await?;
            let base = lab.world().accounts.clone();
            let mut registry = base.clone();
            let mut added = Account::registered(
                UNSAVED_ID,
                "newlogin",
                Provider::OpenAi,
                CredentialSource::LocalFolder {
                    path: lab.rig.home.as_path().display().to_string(),
                    detected: DetectedKind::CodexAuthJson,
                },
            );
            match typed {
                "purpose" => added.purpose = Some("work".to_owned()),
                "nothing" => {}
                _ => {
                    added.endpoint = Some(jackin_console::domain::account::Endpoint {
                        label: "custom".to_owned(),
                        host: "example.invalid".to_owned(),
                    });
                }
            }
            registry.accounts.push(added);
            lab.perform(Effect::SaveAccounts {
                ticket: 1,
                base: Box::new(base),
                registry: Box::new(registry),
            })
            .await?;
            let saved = lab.config().await?.account(&account("newlogin")?).is_some();
            assert_eq!(saved, typed == "nothing", "{typed}");
            assert_eq!(
                lab.console.config_text().await? == before,
                typed != "nothing",
                "{typed}"
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_folder_without_a_login_writes_nothing() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let empty = lab.code.join("alpha");
        let before = lab.console.config_text().await?;
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        registry.accounts.push(Account::registered(
            UNSAVED_ID,
            "newlogin",
            Provider::OpenAi,
            CredentialSource::LocalFolder {
                path: empty.display().to_string(),
                detected: DetectedKind::CodexAuthJson,
            },
        ));
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        assert_eq!(lab.console.config_text().await?, before);
        assert!(
            !lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("newlogin")?)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_stale_registry_keeps_the_other_handles_account() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let base = lab.world().accounts.clone();
        second_login(&lab).await?;
        let mut registry = base.clone();
        registry
            .accounts
            .iter_mut()
            .for_each(|a| a.default_for_provider = a.id == "work");
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        let config = lab.config().await?;
        assert!(config.account(&account("second")?).is_some());
        assert_eq!(
            config.default_account(CoreProvider::Zai),
            Some(&account("work")?)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn workspace_binding_from_the_editor() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.console.keys("down e 5 down p")?;
        lab.save().await?;
        let config = lab.config().await?;
        let workspace = config
            .workspace(&WorkspaceName::parse("platform-backend")?)
            .ok_or("workspace")?;
        assert_eq!(
            workspace.bindings.get(&CoreProvider::Zai),
            Some(&account("work")?)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn role_binding_from_the_roles_page() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        second_login(&lab).await?;
        lab.reopen().await?;
        lab.console.keys("R enter enter")?;
        let screen = lab.console.screen();
        assert!(screen.contains("second"), "{screen}");
        lab.console.keys("enter")?;
        lab.console.settle().await?;
        let bound = |config: &jackin_core::Config| -> Result<Option<AccountName>, Box<dyn std::error::Error>> {
            Ok(config
                .role(&RoleName::parse("sentinel")?)
                .and_then(|role| role.bindings.get(&Agent::Codex).cloned()))
        };
        assert_eq!(bound(&lab.config().await?)?, Some(account("second")?));
        lab.console.keys("d")?;
        lab.console.settle().await?;
        assert_eq!(bound(&lab.config().await?)?, None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn role_admin_offers_all_agents_and_binds_zai_for_omp_and_claude() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.edit(vec![ConfigEdit::AddAccount(CoreAccount::new(
            account("zai")?,
            CoreProvider::Zai,
            Credential::Env {
                var: jackin_core::EnvName::parse("ZAI_TEST_KEY")?,
            },
        )?)])
        .await?;
        lab.reopen().await?;
        let admin = lab.world().surfaces.roles.clone().ok_or("role admin")?;
        assert_eq!(
            admin.agents,
            [
                ConsoleAgent::Codex,
                ConsoleAgent::ClaudeCode,
                ConsoleAgent::GrokBuild,
                ConsoleAgent::MuseCode,
                ConsoleAgent::Antigravity,
                ConsoleAgent::MinimaxCode,
                ConsoleAgent::MimoCode,
                ConsoleAgent::KimiCode,
                ConsoleAgent::Omp,
            ]
        );
        for (console_agent, core_agent) in [
            (ConsoleAgent::Omp, Agent::Omp),
            (ConsoleAgent::ClaudeCode, Agent::ClaudeCode),
        ] {
            lab.perform(Effect::BindRole {
                role: "sentinel".to_owned(),
                agent: console_agent,
                base: None,
                account: Some("zai".to_owned()),
            })
            .await?;
            let config = lab.config().await?;
            let bound = config
                .role(&RoleName::parse("sentinel")?)
                .and_then(|role| role.bindings.get(&core_agent));
            assert_eq!(bound, Some(&account("zai")?));
            lab.reopen().await?;
            let projected = lab
                .world()
                .surfaces
                .roles
                .as_ref()
                .and_then(|roles| roles.binding("sentinel", console_agent))
                .cloned();
            assert_eq!(projected.as_deref(), Some("zai"));
        }
        lab.perform(Effect::BindRole {
            role: "sentinel".to_owned(),
            agent: ConsoleAgent::Omp,
            base: Some("zai".to_owned()),
            account: None,
        })
        .await?;
        let config = lab.config().await?;
        let role = config.role(&RoleName::parse("sentinel")?).ok_or("role")?;
        assert!(!role.bindings.contains_key(&Agent::Omp));
        assert_eq!(
            role.bindings.get(&Agent::ClaudeCode),
            Some(&account("zai")?)
        );
        lab.perform(Effect::BindRole {
            role: "sentinel".to_owned(),
            agent: ConsoleAgent::ClaudeCode,
            base: Some("zai".to_owned()),
            account: None,
        })
        .await?;
        let config = lab.config().await?;
        let role = config.role(&RoleName::parse("sentinel")?).ok_or("role")?;
        assert!(!role.bindings.contains_key(&Agent::Omp));
        assert!(!role.bindings.contains_key(&Agent::ClaudeCode));
        Ok(())
    })
    .await
}

async fn preselected(
    extra: &[&str],
    edits: Vec<ConfigEdit>,
) -> Result<(Option<String>, Option<String>), Box<dyn std::error::Error>> {
    let mut lab = Lab::open().await?;
    let mut adds = Vec::new();
    for name in extra {
        adds.push(ConfigEdit::AddAccount(CoreAccount::new(
            account(name)?,
            CoreProvider::Zai,
            Credential::File {
                path: jackin_core::HostPath::parse(&lab.rig.home.as_path().join("zai.txt"))?,
            },
        )?));
    }
    lab.edit(adds).await?;
    lab.edit(edits).await?;
    lab.reopen().await?;
    let config = lab.config().await?;
    let workspace = lab.workspace("platform-backend").ok_or("workspace")?;
    let name = WorkspaceName::parse("platform-backend")?;
    let role = RoleName::parse("sentinel")?;
    let core = config
        .select_account(&jackin_core::AccountQuery {
            flag: None,
            workspace: &name,
            role: &role,
            agent: Agent::Codex,
            provider: CoreProvider::Zai,
        })
        .ok()
        .map(|selected| selected.account.to_string());
    let app = lab.console.console().app();
    let world = &app.world;
    let mut feed = world.backend.launch(
        world,
        &LaunchTarget {
            workspace: Some(workspace.id),
            role: "sentinel".to_owned(),
            instances: vec![InstanceChoice {
                agent: jackin_console::domain::agent::Agent::Codex,
                provider: Provider::Zai,
                account: None,
                model: "glm-5.3".to_owned(),
            }],
            task: None,
        },
    );
    let mut events = Vec::new();
    let mut answered = false;
    for _ in 0..2000 {
        events.extend(feed.poll(world));
        if !answered
            && events
                .iter()
                .any(|event| matches!(event, LaunchEvent::NeedsInput(_)))
        {
            feed.answer_env(vec![EnvAnswer {
                name: "GREETING".to_owned(),
                value: "hello".to_owned(),
            }]);
            answered = true;
        }
        if events.iter().any(|event| {
            matches!(
                event,
                LaunchEvent::Ready(_) | LaunchEvent::Failed(_) | LaunchEvent::Refused(_)
            )
        }) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut sandboxes = lab.rig.world.gateway.sandboxes();
    let launched = if events
        .iter()
        .any(|event| matches!(event, LaunchEvent::Ready(_)))
    {
        if sandboxes.len() != 1 {
            return Err(format!("expected one sandbox, saw {}", sandboxes.len()).into());
        }
        let labels = jackin_core::SandboxLabels::from_labels(&sandboxes.remove(0).labels())?;
        Some(labels.planned.agents.agents_label())
    } else {
        if !sandboxes.is_empty() {
            return Err("a refused account selection created a sandbox".into());
        }
        None
    };
    Ok((launched, core))
}

#[tokio::test]
async fn the_preselected_account_equals_the_d26_table() -> TestResult {
    guard(async {
        let ws = |name: &str| -> Result<ConfigEdit, Box<dyn std::error::Error>> {
            Ok(ConfigEdit::BindWorkspaceAccount {
                workspace: WorkspaceName::parse("platform-backend")?,
                account: account(name)?,
            })
        };
        let role = |name: &str| -> Result<ConfigEdit, Box<dyn std::error::Error>> {
            Ok(ConfigEdit::BindRoleAccount {
                role: RoleName::parse("sentinel")?,
                agent: Agent::Codex,
                account: account(name)?,
            })
        };
        let default = |name: &str| -> Result<ConfigEdit, Box<dyn std::error::Error>> {
            Ok(ConfigEdit::SetDefaultAccount {
                account: account(name)?,
            })
        };
        let cases: Vec<Case<'_>> = vec![
            (
                "workspace beats role and default",
                vec!["a1", "a2"],
                vec![ws("a1")?, role("a2")?, default("work")?],
                Some("a1"),
            ),
            (
                "role beats default",
                vec!["a1", "a2"],
                vec![role("a2")?, default("work")?],
                Some("a2"),
            ),
            ("default only", vec!["a1"], vec![default("a1")?], Some("a1")),
            ("the only account", vec![], vec![], Some("work")),
            ("two accounts and no default", vec!["a1"], vec![], None),
        ];
        for (case, extra, edits, expected) in cases {
            let (launched, core) = preselected(&extra, edits).await?;
            let expected_launch = expected
                .map(|name| {
                    Ok::<_, Box<dyn std::error::Error>>(
                        jackin_core::AgentInstances::single(jackin_core::AgentInstance::new(
                            Agent::Codex,
                            CoreProvider::Zai,
                            account(name)?,
                            "glm-5.3".to_owned(),
                        ))
                        .agents_label(),
                    )
                })
                .transpose()?;
            assert_eq!(launched, expected_launch, "{case}: the launch");
            assert_eq!(core.as_deref(), expected, "{case}: Config::select_account");
        }
        Ok(())
    })
    .await
}

async fn dialed(
    lab: &Lab,
    peer: &Arc<FakeGateway>,
) -> Result<ConsoleRig, Box<dyn std::error::Error>> {
    let home: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&lab.rig.world.gateway);
    let other: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(peer);
    let connector = ScriptedConnector::new()
        .gateway(home, Vec::new())
        .gateway(other, Vec::new());
    let mut console = ConsoleRig::builder(&lab.rig.world)
        .connector(connector_over(&lab.rig.world, Arc::new(connector)))
        .build()
        .await?;
    console.skip_intro();
    Ok(console)
}

fn remove_second(registry: &mut jackin_console::domain::account::AccountRegistry) {
    registry.accounts.retain(|a| a.id != "second");
}

#[tokio::test]
async fn a_removal_reaches_a_gateway_that_was_added_after_the_console_opened() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        second_login(&lab).await?;
        lab.reopen().await?;
        let name = GatewayName::parse("late")?;
        let peer = Arc::new(FakeGateway::new(
            name.clone(),
            OpenShellWorkspace::parse("default")?,
        ));
        let login = lab
            .config()
            .await?
            .account(&account("second")?)
            .cloned()
            .ok_or("second")?;
        let entry = lab
            .config()
            .await?
            .gateways()
            .next()
            .map(|(_, entry)| entry.clone())
            .ok_or("entry")?;
        lab.console = dialed(&lab, &peer).await?;
        lab.edit(vec![ConfigEdit::AddGateway { name, entry }])
            .await?;
        let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&peer);
        session_over(&lab.rig.world, port)
            .sync_account(&login, Agent::Codex, &CancellationToken::new())
            .await?;
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        remove_second(&mut registry);
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        assert!(lab.config().await?.account(&account("second")?).is_none());
        assert!(!peer.providers().contains(&provider_of("second")?));
        assert!(
            !lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("second")?)
        );
        Ok(())
    })
    .await
}

async fn with_unreached_peer(
    lab: &mut Lab,
) -> Result<(Arc<ScriptedConnector>, GatewayName, Arc<FakeGateway>), Box<dyn std::error::Error>> {
    let name = GatewayName::parse("late")?;
    let peer = Arc::new(FakeGateway::new(
        name.clone(),
        OpenShellWorkspace::parse("default")?,
    ));
    let entry = lab
        .config()
        .await?
        .gateways()
        .next()
        .map(|(_, entry)| entry.clone())
        .ok_or("entry")?;
    let home: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&lab.rig.world.gateway);
    let other: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&peer);
    let connector = Arc::new(
        ScriptedConnector::new()
            .gateway(home, Vec::new())
            .gateway(other, Vec::new()),
    );
    lab.console = ConsoleRig::builder(&lab.rig.world)
        .connector(connector_over(
            &lab.rig.world,
            Arc::<ScriptedConnector>::clone(&connector),
        ))
        .build()
        .await?;
    lab.console.skip_intro();
    lab.edit(vec![ConfigEdit::AddGateway {
        name: name.clone(),
        entry,
    }])
    .await?;
    Ok((connector, name, peer))
}

fn added_login(registry: &mut jackin_console::domain::account::AccountRegistry, lab: &Lab) {
    registry.accounts.push(Account::registered(
        UNSAVED_ID,
        "newlogin",
        Provider::OpenAi,
        CredentialSource::LocalFolder {
            path: lab.rig.home.as_path().display().to_string(),
            detected: DetectedKind::CodexAuthJson,
        },
    ));
}

#[tokio::test]
async fn a_new_account_does_not_dial_a_peer_the_console_has_not_reached() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let (connector, name, peer) = with_unreached_peer(&mut lab).await?;
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        added_login(&mut registry, &lab);
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        assert!(lab.config().await?.account(&account("newlogin")?).is_some());
        assert!(
            lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("newlogin")?)
        );
        assert!(!peer.providers().contains(&provider_of("newlogin")?));
        assert_eq!(connector.calls(&name), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_add_with_a_removal_forgets_the_provider_on_a_peer_the_console_has_not_reached()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        second_login(&lab).await?;
        lab.reopen().await?;
        let (_connector, _name, peer) = with_unreached_peer(&mut lab).await?;
        let login = lab
            .config()
            .await?
            .account(&account("second")?)
            .cloned()
            .ok_or("second")?;
        let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&peer);
        session_over(&lab.rig.world, port)
            .sync_account(&login, Agent::Codex, &CancellationToken::new())
            .await?;
        assert!(peer.providers().contains(&provider_of("second")?));
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        remove_second(&mut registry);
        added_login(&mut registry, &lab);
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        let config = lab.config().await?;
        assert!(config.account(&account("newlogin")?).is_some());
        assert!(config.account(&account("second")?).is_none());
        assert!(!peer.providers().contains(&provider_of("second")?));
        Ok(())
    })
    .await
}

#[derive(Debug)]
struct EntryConnector {
    fallback: Arc<dyn Gateway>,
    by_builder: std::sync::Mutex<std::collections::BTreeMap<String, Arc<dyn Gateway>>>,
    behavior: std::sync::Mutex<Dial>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dial {
    Serve,
    Refuse,
    Stall,
}

impl EntryConnector {
    fn new(fallback: Arc<dyn Gateway>) -> EntryConnector {
        EntryConnector {
            fallback,
            by_builder: std::sync::Mutex::new(std::collections::BTreeMap::new()),
            behavior: std::sync::Mutex::new(Dial::Serve),
        }
    }

    fn serve(&self, builder: &str, gateway: Arc<dyn Gateway>) {
        if let Ok(mut held) = self.by_builder.lock() {
            held.insert(builder.to_owned(), gateway);
        }
    }

    fn behave(&self, behavior: Dial) {
        if let Ok(mut held) = self.behavior.lock() {
            *held = behavior;
        }
    }
}

#[async_trait::async_trait]
impl jackin_session::GatewayConnector for EntryConnector {
    async fn connect(
        &self,
        _name: &GatewayName,
        entry: &jackin_core::GatewayEntry,
        _cancel: &CancellationToken,
    ) -> Result<Arc<dyn Gateway>, jackin_session::ConnectError> {
        let behavior = self.behavior.lock().map_or(Dial::Serve, |held| *held);
        match behavior {
            Dial::Serve => {}
            Dial::Stall => std::future::pending().await,
            Dial::Refuse => {
                return Err(jackin_session::ConnectError::Gateway(
                    jackin_openshell::GatewayError::Unreachable {
                        endpoint: "moved".to_owned(),
                        message: "refused".to_owned(),
                    },
                ));
            }
        }
        let chosen = self
            .by_builder
            .lock()
            .ok()
            .and_then(|held| held.get(entry.builder.as_str()).cloned());
        Ok(chosen.unwrap_or_else(|| Arc::clone(&self.fallback)))
    }
}

fn moved_entry(builder: &str) -> Result<jackin_core::GatewayEntry, Box<dyn std::error::Error>> {
    jackin_core::Config::parse(&format!(
        "[gateways.moved]\nopenshell_workspace = \"default\"\nbuilder = \"{builder}\"\nplatform = \"linux/arm64\"\n"
    ))?
    .gateway(&GatewayName::parse("moved")?)
    .cloned()
    .ok_or_else(|| "moved".into())
}

async fn console_over(lab: &mut Lab, connector: &Arc<EntryConnector>) -> TestResult {
    lab.console = ConsoleRig::builder(&lab.rig.world)
        .deadline(Duration::from_millis(300))
        .connector(connector_over(
            &lab.rig.world,
            Arc::<EntryConnector>::clone(connector),
        ))
        .build()
        .await?;
    lab.console.skip_intro();
    Ok(())
}

async fn move_the_home_entry(
    lab: &mut Lab,
    behavior: Dial,
) -> Result<(GatewayName, Arc<FakeGateway>), Box<dyn std::error::Error>> {
    let home = lab
        .config()
        .await?
        .gateways()
        .next()
        .map(|(name, _)| name.clone())
        .ok_or("home gateway")?;
    let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&lab.rig.world.gateway);
    let connector = Arc::new(EntryConnector::new(port));
    let relocated = Arc::new(FakeGateway::new(
        home.clone(),
        OpenShellWorkspace::parse("default")?,
    ));
    let served: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&relocated);
    connector.serve("moved", served);
    console_over(lab, &connector).await?;
    connector.behave(behavior);
    lab.edit(vec![
        ConfigEdit::SetDefaultGateway(None),
        ConfigEdit::RemoveGateway(home.clone()),
        ConfigEdit::AddGateway {
            name: home.clone(),
            entry: moved_entry("moved")?,
        },
        ConfigEdit::SetDefaultGateway(Some(home.clone())),
    ])
    .await?;
    Ok((home, relocated))
}

async fn save_the_new_login(lab: &mut Lab) -> TestResult {
    let base = lab.world().accounts.clone();
    let mut registry = base.clone();
    added_login(&mut registry, lab);
    lab.perform(Effect::SaveAccounts {
        ticket: 1,
        base: Box::new(base),
        registry: Box::new(registry),
    })
    .await?;
    Ok(())
}

#[tokio::test]
async fn a_new_account_publishes_through_a_home_gateway_dialed_with_the_current_entry() -> TestResult
{
    guard(async {
        let mut lab = Lab::open().await?;
        let (_home, relocated) = move_the_home_entry(&mut lab, Dial::Serve).await?;
        save_the_new_login(&mut lab).await?;
        assert!(lab.config().await?.account(&account("newlogin")?).is_some());
        assert!(relocated.providers().contains(&provider_of("newlogin")?));
        assert!(
            !lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("newlogin")?)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_home_gateway_that_refuses_its_moved_entry_blocks_the_save_and_publishes_nothing()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let before = lab.config().await?;
        let (_home, relocated) = move_the_home_entry(&mut lab, Dial::Refuse).await?;
        save_the_new_login(&mut lab).await?;
        let config = lab.config().await?;
        assert!(config.account(&account("newlogin")?).is_none());
        assert!(!relocated.providers().contains(&provider_of("newlogin")?));
        assert!(
            !lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("newlogin")?)
        );
        assert_eq!(config.accounts().count(), before.accounts().count());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_home_gateway_that_stalls_on_its_moved_entry_blocks_the_save_within_the_connect_deadline()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let before = lab.config().await?;
        let (_home, relocated) = move_the_home_entry(&mut lab, Dial::Stall).await?;
        tokio::time::pause();
        let started = tokio::time::Instant::now();
        save_the_new_login(&mut lab).await?;
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the save waited {:?} for a stalled dial with a 300ms deadline",
            started.elapsed()
        );
        let config = lab.config().await?;
        assert!(config.account(&account("newlogin")?).is_none());
        assert!(!relocated.providers().contains(&provider_of("newlogin")?));
        assert_eq!(config.accounts().count(), before.accounts().count());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_removal_reaches_the_current_entry_of_a_peer_that_moved_after_it_was_reached()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        second_login(&lab).await?;
        lab.reopen().await?;
        let login = lab
            .config()
            .await?
            .account(&account("second")?)
            .cloned()
            .ok_or("second")?;
        let name = GatewayName::parse("late")?;
        let workspace = OpenShellWorkspace::parse("default")?;
        let old_peer = Arc::new(FakeGateway::new(name.clone(), workspace.clone()));
        let new_peer = Arc::new(FakeGateway::new(name.clone(), workspace));
        let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&lab.rig.world.gateway);
        let connector = Arc::new(EntryConnector::new(port));
        let old: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&old_peer);
        let new: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&new_peer);
        connector.serve("early", Arc::clone(&old));
        connector.serve("later", Arc::clone(&new));
        lab.edit(vec![ConfigEdit::AddGateway {
            name: name.clone(),
            entry: moved_entry("early")?,
        }])
        .await?;
        console_over(&mut lab, &connector).await?;
        for peer in [old, new] {
            session_over(&lab.rig.world, peer)
                .sync_account(&login, Agent::Codex, &CancellationToken::new())
                .await?;
        }
        lab.edit(vec![
            ConfigEdit::RemoveGateway(name.clone()),
            ConfigEdit::AddGateway {
                name: name.clone(),
                entry: moved_entry("later")?,
            },
        ])
        .await?;
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        remove_second(&mut registry);
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        assert!(lab.config().await?.account(&account("second")?).is_none());
        assert!(!new_peer.providers().contains(&provider_of("second")?));
        assert!(old_peer.providers().contains(&provider_of("second")?));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_add_publishes_nothing_when_the_home_gateway_left_the_config() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let home = lab
            .config()
            .await?
            .gateways()
            .next()
            .map(|(name, _)| name.clone())
            .ok_or("home gateway")?;
        let base = lab.world().accounts.clone();
        let mut registry = base.clone();
        registry.accounts.push(Account::registered(
            UNSAVED_ID,
            "newlogin",
            Provider::OpenAi,
            CredentialSource::LocalFolder {
                path: lab.rig.home.as_path().display().to_string(),
                detected: DetectedKind::CodexAuthJson,
            },
        ));
        lab.edit(vec![ConfigEdit::RemoveGateway(home)]).await?;
        let before = lab.console.config_text().await?;
        lab.perform(Effect::SaveAccounts {
            ticket: 1,
            base: Box::new(base),
            registry: Box::new(registry),
        })
        .await?;
        assert_eq!(lab.console.config_text().await?, before);
        assert!(
            !lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("newlogin")?)
        );
        Ok(())
    })
    .await
}

async fn cli_adds_an_account(lab: &Lab, name: &str) -> Out {
    lab.edit(vec![ConfigEdit::AddAccount(CoreAccount::new(
        account(name)?,
        CoreProvider::Codex,
        Credential::Env {
            var: jackin_core::EnvName::parse("SPARE_KEY")?,
        },
    )?)])
    .await?;
    Ok(())
}

async fn after_refresh(lab: &mut Lab, secs: u64) -> TestResult {
    for _ in 0..secs {
        lab.console.elapse(1);
        lab.console.tick(1);
        lab.console.settle().await?;
    }
    Ok(())
}

#[tokio::test]
async fn an_account_added_by_the_cli_shows_on_the_visible_accounts_view() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.step("c").await?;
        let before = lab.console.screen();
        assert!(!before.contains("spare"), "{before}");
        cli_adds_an_account(&lab, "spare").await?;
        after_refresh(&mut lab, 12).await?;
        let screen = lab.console.screen();
        assert!(screen.contains("spare"), "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_account_added_by_the_cli_shows_on_the_manager() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        cli_adds_an_account(&lab, "spare").await?;
        after_refresh(&mut lab, 12).await?;
        let world = &lab.world().accounts.accounts;
        assert!(world.iter().any(|a| a.display_name == "spare"));
        Ok(())
    })
    .await
}
