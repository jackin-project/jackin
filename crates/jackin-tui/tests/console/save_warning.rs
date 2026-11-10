// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::Effect;
use jackin_console::domain::account::{Account, CredentialSource, DetectedKind, UNSAVED_ID};
use jackin_console::domain::agent::{Agent, Provider};
use jackin_console::domain::workspace::EnvVar;
use jackin_console::world::SaveWarning;
use jackin_core::{
    AccountName, Agent as CoreAgent, ConfigEdit, Provider as CoreProvider, ProviderName, RoleName,
    RoleSource, Workspace, WorkspaceName,
};
use jackin_testkit::{FsFault, RoleFixture, TestResult, guard};

use super::support::Lab;

fn warned(lab: &mut Lab) {
    assert_eq!(
        lab.world().committed_save_warning,
        Some(SaveWarning::DirectorySync)
    );
}

fn unchanged(lab: &mut Lab) {
    assert_eq!(lab.world().committed_save_warning, None);
    assert!(
        !lab.console
            .status_line()
            .contains(SaveWarning::DirectorySync.status()),
        "{}",
        lab.console.status_line()
    );
}

async fn second_workspace(lab: &Lab) -> Result<(), Box<dyn std::error::Error>> {
    let path = jackin_core::HostPath::parse(&std::fs::canonicalize(lab.code.join("alpha"))?)?;
    lab.edit(vec![ConfigEdit::AddWorkspace {
        name: WorkspaceName::parse("second")?,
        workspace: Workspace::new(path, false),
    }])
    .await
}

fn offline_account(lab: &Lab) -> Result<Account, Box<dyn std::error::Error>> {
    Ok(Account::registered(
        UNSAVED_ID,
        "offline-zai",
        Provider::Zai,
        CredentialSource::LocalFolder {
            path: lab.rig.home.as_path().join("zai.txt").display().to_string(),
            detected: DetectedKind::Unknown,
        },
    ))
}

fn gateway_account(lab: &Lab) -> Result<Account, Box<dyn std::error::Error>> {
    Ok(Account::registered(
        UNSAVED_ID,
        "newlogin",
        Provider::OpenAi,
        CredentialSource::LocalFolder {
            path: lab.rig.home.as_path().display().to_string(),
            detected: DetectedKind::CodexAuthJson,
        },
    ))
}

fn provider_of(name: &str) -> Result<ProviderName, Box<dyn std::error::Error>> {
    Ok(ProviderName::for_account(
        &AccountName::parse(name)?,
        CoreAgent::Codex,
    ))
}

async fn save_accounts(lab: &mut Lab, added: Account) -> TestResult {
    let base = lab.world().accounts.clone();
    let mut registry = base.clone();
    registry.accounts.push(added);
    lab.perform(Effect::SaveAccounts {
        ticket: 1,
        base: Box::new(base),
        registry: Box::new(registry),
    })
    .await
}

#[tokio::test]
async fn committed_workspace_save_applies_and_warns() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let base = lab.workspace("platform-backend").ok_or("workspace")?;
        let mut payload = base.clone();
        payload.docker = !payload.docker;
        lab.rig.world.fail_fs(FsFault::SyncDir);
        lab.perform(Effect::SaveWorkspace {
            id: Some(base.id),
            base: Some(Box::new(base)),
            workspace: Box::new(payload.clone()),
        })
        .await?;
        let name = WorkspaceName::parse("platform-backend")?;
        assert_eq!(
            lab.config()
                .await?
                .workspace(&name)
                .map(|workspace| workspace.docker),
            Some(payload.docker)
        );
        warned(&mut lab);
        lab.reopen().await?;
        assert_eq!(
            lab.workspace("platform-backend")
                .map(|workspace| workspace.docker),
            Some(payload.docker)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn committed_workspace_delete_applies_and_warns() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        second_workspace(&lab).await?;
        lab.reopen().await?;
        let id = lab.workspace("second").ok_or("second workspace")?.id;
        lab.rig.world.fail_fs(FsFault::SyncDir);
        lab.perform(Effect::DeleteWorkspace { id }).await?;
        let name = WorkspaceName::parse("second")?;
        assert!(lab.config().await?.workspace(&name).is_none());
        assert!(lab.workspace("second").is_none());
        warned(&mut lab);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn committed_global_save_applies_and_warns() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let base = lab.world().global.clone();
        let mut payload = base.clone();
        payload.env.push(EnvVar::plain("DURABILITY", "committed"));
        lab.rig.world.fail_fs(FsFault::SyncDir);
        lab.perform(Effect::SaveGlobal {
            base: Box::new(base),
            global: Box::new(payload),
        })
        .await?;
        let name = jackin_core::EnvName::parse("DURABILITY")?;
        assert!(lab.config().await?.env().contains_key(&name));
        warned(&mut lab);
        lab.reopen().await?;
        assert!(
            lab.world()
                .global
                .env
                .iter()
                .any(|entry| entry.key == "DURABILITY")
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn committed_offline_account_save_applies_and_warns() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let added = offline_account(&lab)?;
        lab.rig.world.fail_fs(FsFault::SyncDir);
        save_accounts(&mut lab, added).await?;
        let name = AccountName::parse("offline-zai")?;
        assert!(lab.config().await?.account(&name).is_some());
        assert!(
            lab.world()
                .accounts
                .accounts
                .iter()
                .any(|account| account.id == "offline-zai")
        );
        assert!(
            !lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("offline-zai")?)
        );
        warned(&mut lab);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn committed_gateway_account_save_keeps_remote_effects_and_warns() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let added = gateway_account(&lab)?;
        lab.rig.world.fail_fs(FsFault::SyncDir);
        save_accounts(&mut lab, added).await?;
        let name = AccountName::parse("newlogin")?;
        assert!(lab.config().await?.account(&name).is_some());
        assert!(
            lab.world()
                .accounts
                .accounts
                .iter()
                .any(|account| account.id == "newlogin")
        );
        assert!(
            lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("newlogin")?)
        );
        warned(&mut lab);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn committed_role_binding_applies_and_warns() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.edit(vec![ConfigEdit::AddAccount(jackin_core::Account::new(
            AccountName::parse("codex-work")?,
            CoreProvider::Codex,
            jackin_core::CredentialSource::CodexLogin {
                home: lab.rig.home.clone(),
                chatgpt_account_id: jackin_core::ChatgptAccountId::parse(
                    jackin_testkit::ACCOUNT_ID,
                )?,
            },
        )?)])
        .await?;
        lab.reopen().await?;
        lab.rig.world.fail_fs(FsFault::SyncDir);
        lab.perform(Effect::BindRole {
            role: "sentinel".to_owned(),
            agent: Agent::Codex,
            base: None,
            account: Some("codex-work".to_owned()),
        })
        .await?;
        let role = RoleName::parse("sentinel")?;
        assert_eq!(
            lab.config()
                .await?
                .role(&role)
                .and_then(|role| role.bindings.get(&CoreAgent::Codex)),
            Some(&AccountName::parse("codex-work")?)
        );
        warned(&mut lab);
        lab.reopen().await?;
        let bound = lab
            .world()
            .surfaces
            .roles
            .as_ref()
            .and_then(|roles| roles.role("sentinel"))
            .and_then(|role| role.bindings.get(&Agent::Codex))
            .map(String::as_str);
        assert_eq!(bound, Some("codex-work"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn committed_role_removal_applies_and_warns() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.add_role("other").await?;
        lab.reopen().await?;
        lab.rig.world.fail_fs(FsFault::SyncDir);
        lab.perform(Effect::RemoveRole {
            role: "other".to_owned(),
        })
        .await?;
        let role = RoleName::parse("other")?;
        assert!(lab.config().await?.role(&role).is_none());
        warned(&mut lab);
        lab.reopen().await?;
        assert!(!lab.world().roles.iter().any(|role| role.name == "other"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn committed_role_addition_applies_and_warns() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let fresh = RoleFixture::new("fresh")?;
        fresh.commit().await?;
        lab.rig.world.fail_fs(FsFault::SyncDir);
        lab.perform(Effect::AddRole {
            location: fresh.root().as_path().display().to_string(),
        })
        .await?;
        let role = RoleName::parse("fresh")?;
        assert_eq!(
            lab.config().await?.role(&role).map(|role| &role.source),
            Some(&RoleSource::Path(fresh.root().clone()))
        );
        warned(&mut lab);
        lab.reopen().await?;
        assert!(lab.world().roles.iter().any(|role| role.name == "fresh"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn ordinary_save_applies_without_a_warning() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let base = lab.world().global.clone();
        let mut payload = base.clone();
        payload.env.push(EnvVar::plain("ORDINARY", "saved"));
        lab.perform(Effect::SaveGlobal {
            base: Box::new(base),
            global: Box::new(payload),
        })
        .await?;
        let name = jackin_core::EnvName::parse("ORDINARY")?;
        assert!(lab.config().await?.env().contains_key(&name));
        unchanged(&mut lab);
        lab.reopen().await?;
        assert!(
            lab.world()
                .global
                .env
                .iter()
                .any(|entry| entry.key == "ORDINARY")
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn pre_commit_failures_keep_config_and_world_state() -> TestResult {
    guard(async {
        for fault in [FsFault::WriteNew, FsFault::Rename] {
            let mut lab = Lab::open().await?;
            let before = lab.console.config_text().await?;
            let before_global = lab.world().global.clone();
            let base = lab.world().global.clone();
            let mut payload = base.clone();
            payload.env.push(EnvVar::plain("UNCOMMITTED", "no"));
            lab.rig.world.fail_fs(fault);
            lab.perform(Effect::SaveGlobal {
                base: Box::new(base),
                global: Box::new(payload),
            })
            .await?;
            assert_eq!(lab.console.config_text().await?, before, "{fault:?}");
            assert_eq!(lab.world().global, before_global, "{fault:?}");
            unchanged(&mut lab);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn validation_failure_keeps_config_and_world_state() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let before = lab.console.config_text().await?;
        let before_workspaces = lab.world().workspaces.clone();
        let base = lab.workspace("platform-backend").ok_or("workspace")?;
        let mut payload = base.clone();
        payload.keep_awake = true;
        lab.perform(Effect::SaveWorkspace {
            id: Some(base.id),
            base: Some(Box::new(base)),
            workspace: Box::new(payload),
        })
        .await?;
        assert_eq!(lab.console.config_text().await?, before);
        assert_eq!(lab.world().workspaces, before_workspaces);
        unchanged(&mut lab);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn gateway_account_pre_commit_failure_rolls_back_remote_effects() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let before = lab.console.config_text().await?;
        let before_accounts = lab.world().accounts.clone();
        let added = gateway_account(&lab)?;
        lab.rig.world.fail_fs(FsFault::WriteNew);
        save_accounts(&mut lab, added).await?;
        assert_eq!(lab.console.config_text().await?, before);
        assert_eq!(lab.world().accounts, before_accounts);
        assert!(
            !lab.rig
                .world
                .gateway
                .providers()
                .contains(&provider_of("newlogin")?)
        );
        unchanged(&mut lab);
        Ok(())
    })
    .await
}
