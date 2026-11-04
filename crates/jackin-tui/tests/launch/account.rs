// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::app::Route;
use jackin_core::{
    Account, AccountName, AccountQuery, Agent, ChatgptAccountId, ConfigEdit, CredentialSource,
    Provider, RoleName, WorkspaceName,
};
use jackin_testkit::{ACCOUNT_ID, Rig, TestResult, guard};

use super::support::{answer, begin, console, labels, only, until_route};

type Failure = Box<dyn std::error::Error>;

const WORKSPACE: &str = "platform-backend";

fn name(text: &str) -> Result<AccountName, Failure> {
    Ok(AccountName::parse(text)?)
}

fn login(rig: &Rig, account: &str) -> Result<ConfigEdit, Failure> {
    Ok(ConfigEdit::AddAccount(Account::new(
        name(account)?,
        Provider::Codex,
        CredentialSource::CodexLogin {
            home: rig.home.clone(),
            chatgpt_account_id: ChatgptAccountId::parse(ACCOUNT_ID)?,
        },
    )?))
}

async fn launched(rig: &Rig) -> Result<(AccountName, String), Failure> {
    let mut console = console(rig).await?;
    begin(&mut console)?;
    let picked = console.screen();
    answer(&mut console, None).await?;
    until_route(&mut console, Route::Capsule).await?;
    Ok((labels(&only(rig)?)?.planned.account, picked))
}

#[tokio::test]
async fn the_launched_account_follows_the_d26_order() -> TestResult {
    guard(async {
        let cases: [(&str, bool, bool, bool); 3] = [
            ("by-workspace", true, true, true),
            ("by-role", false, true, true),
            ("by-default", false, false, true),
        ];
        for (expected, workspace, role, default) in cases {
            let rig = Rig::new().await?;
            rig.world.seed_all(&rig.config).await?;
            let mut edits = Vec::new();
            for account in ["by-workspace", "by-role", "by-default"] {
                edits.push(login(&rig, account)?);
            }
            if workspace {
                edits.push(ConfigEdit::BindWorkspaceAccount {
                    workspace: WorkspaceName::parse(WORKSPACE)?,
                    account: name("by-workspace")?,
                });
            }
            if role {
                edits.push(ConfigEdit::BindRoleAccount {
                    role: RoleName::parse("sentinel")?,
                    agent: Agent::Codex,
                    account: name("by-role")?,
                });
            }
            if default {
                edits.push(ConfigEdit::SetDefaultAccount {
                    account: name("by-default")?,
                });
            }
            rig.world.config.edit(edits).await?;
            let config = rig.world.config.load().await?;
            let workspace_name = WorkspaceName::parse(WORKSPACE)?;
            let role_name = RoleName::parse("sentinel")?;
            let selected = config.select_account(&AccountQuery {
                flag: None,
                workspace: &workspace_name,
                role: &role_name,
                agent: Agent::Codex,
            })?;
            assert_eq!(selected.account.as_str(), expected);
            let (account, picked) = launched(&rig).await?;
            assert_eq!(account.as_str(), expected, "{picked}");
            let flat = picked.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(
                flat.contains(&format!("account Codex · {expected}")),
                "{expected}\n{picked}"
            );
        }
        Ok(())
    })
    .await
}
