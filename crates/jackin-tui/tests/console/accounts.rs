// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_console::backend::Effect;
use jackin_console::domain::account::{Account, CredentialSource, DetectedKind, UNSAVED_ID};
use jackin_console::domain::agent::Provider;
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
            config.default_account(CoreProvider::Codex),
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
            workspace.bindings.get(&CoreProvider::Codex),
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
        lab.console.keys("R enter enter")?;
        let screen = lab.console.screen();
        assert!(screen.contains("work"), "{screen}");
        lab.console.keys("enter")?;
        lab.console.settle().await?;
        let bound = |config: &jackin_core::Config| -> Result<Option<AccountName>, Box<dyn std::error::Error>> {
            Ok(config
                .role(&RoleName::parse("sentinel")?)
                .and_then(|role| role.bindings.get(&CoreProvider::Codex).cloned()))
        };
        assert_eq!(bound(&lab.config().await?)?, Some(account("work")?));
        lab.console.keys("d")?;
        lab.console.settle().await?;
        assert_eq!(bound(&lab.config().await?)?, None);
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
            CoreProvider::Codex,
            Credential::CodexLogin {
                home: lab.rig.home.clone(),
                chatgpt_account_id: jackin_core::ChatgptAccountId::parse(ACCOUNT_ID)?,
            },
        )?));
    }
    lab.edit(adds).await?;
    lab.edit(edits).await?;
    lab.reopen().await?;
    let config = lab.config().await?;
    let workspace = lab.workspace("platform-backend").ok_or("workspace")?;
    let offer = lab.world().offer_for(
        jackin_console::domain::agent::Agent::Codex,
        Some(&workspace),
        Some("sentinel"),
    );
    let name = WorkspaceName::parse("platform-backend")?;
    let role = RoleName::parse("sentinel")?;
    let core = config
        .select_account(&jackin_core::AccountQuery {
            flag: None,
            workspace: &name,
            role: &role,
            agent: Agent::Codex,
            provider: CoreProvider::Codex,
        })
        .ok()
        .map(|selected| selected.account.to_string());
    Ok((offer.preselected, core))
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
            let (shown, core) = preselected(&extra, edits).await?;
            if expected.is_some() {
                assert_eq!(shown.as_deref(), expected, "{case}: the picker");
            }
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
