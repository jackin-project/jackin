// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AccountName, Agent, ConfigEdit, ConfigError};

use crate::app::App;
use crate::cli::{AccountArg, AccountCommand, BindArgs, BindScope, JsonArg};
use crate::error::CommandError;
use crate::views::AccountLs;

pub(crate) async fn run(app: &mut App, command: AccountCommand) -> Result<(), CommandError> {
    match command {
        AccountCommand::Ls(arg) => ls(app, &arg).await,
        AccountCommand::Default(arg) => default(app, arg).await,
        AccountCommand::Bind(args) => bind(app, &args).await,
    }
}

async fn ls(app: &mut App, arg: &JsonArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let view = AccountLs::of(&config);
    if arg.json {
        return app.io.json(&view);
    }
    for row in &view.accounts {
        let default = if row.default { "\tdefault" } else { "" };
        app.io.line(&format!(
            "{}\t{}\t{}{default}",
            row.name, row.provider, row.credential
        ))?;
    }
    Ok(())
}

async fn default(app: &App, arg: AccountArg) -> Result<(), CommandError> {
    app.store
        .edit(vec![ConfigEdit::SetDefaultAccount {
            account: arg.account,
        }])
        .await?;
    Ok(())
}

fn unknown_account(name: &AccountName) -> CommandError {
    CommandError::Config(ConfigError::MissingReference {
        kind: "account",
        name: name.to_string(),
    })
}

async fn bind(app: &App, args: &BindArgs) -> Result<(), CommandError> {
    let edit = match args.scope()? {
        BindScope::Workspace(workspace) => ConfigEdit::BindWorkspaceAccount {
            workspace,
            account: args.account.clone(),
        },
        BindScope::Role(role) => {
            let config = app.store.load().await?;
            let account = config
                .account(&args.account)
                .ok_or_else(|| unknown_account(&args.account))?;
            let agent = account.provider().agent().ok_or_else(|| {
                CommandError::Config(ConfigError::ProviderMismatch {
                    account: args.account.clone(),
                    expected: Agent::Codex.provider(),
                    found: account.provider(),
                })
            })?;
            ConfigEdit::BindRoleAccount {
                role,
                agent,
                account: args.account.clone(),
            }
        }
    };
    app.store.edit(vec![edit]).await?;
    Ok(())
}
