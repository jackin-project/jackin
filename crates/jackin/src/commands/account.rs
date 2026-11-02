// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, ConfigEdit, ConfigError, CredentialSource, Provider, ProviderName,
};
use jackin_openshell::GatewayError;
use jackin_session::read_chatgpt_login;

use crate::app::App;
use crate::cli::{
    AccountAddArgs, AccountArg, AccountCommand, BindArgs, BindScope, CredentialArg, JsonArg,
};
use crate::error::CommandError;
use crate::paths::absolutize;
use crate::views::AccountLs;

pub(crate) async fn run(app: &mut App, command: AccountCommand) -> Result<(), CommandError> {
    match command {
        AccountCommand::Add(args) => add(app, args).await,
        AccountCommand::Ls(arg) => ls(app, &arg).await,
        AccountCommand::Rm(arg) => rm(app, arg).await,
        AccountCommand::Default(arg) => default(app, arg).await,
        AccountCommand::Bind(args) => bind(app, &args).await,
    }
}

async fn source(app: &App, args: &AccountAddArgs) -> Result<CredentialSource, CommandError> {
    if let Some(path) = &args.chatgpt_from {
        let home = absolutize(path, &app.cwd)?;
        let login = read_chatgpt_login(app.deps.host.fs.as_ref(), &home)
            .await
            .map_err(|source| CommandError::Login {
                home: home.clone(),
                source,
            })?;
        return Ok(CredentialSource::CodexLogin {
            home,
            chatgpt_account_id: login.account_id,
        });
    }
    match &args.credential {
        Some(CredentialArg::Env(var)) => Ok(CredentialSource::Env { var: var.clone() }),
        Some(CredentialArg::OnePassword(reference)) => Ok(CredentialSource::OnePassword {
            reference: reference.clone(),
        }),
        Some(CredentialArg::File(path)) => Ok(CredentialSource::File {
            path: absolutize(path, &app.cwd)?,
        }),
        None => Err(CommandError::Argument(
            "pass --chatgpt-from or --credential".to_owned(),
        )),
    }
}

async fn add(app: &App, args: AccountAddArgs) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let account = Account::new(args.name.clone(), args.provider, source(app, &args).await?)?;
    let add = ConfigEdit::AddAccount(account.clone());
    config.clone().apply(add.clone())?;
    if account.provider() == Provider::Codex {
        let session = app.session(&config).await?;
        session.sync_account(&account, &app.cancel).await?;
    }
    app.store.edit(vec![add]).await?;
    Ok(())
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

fn unknown_account(name: &AccountName) -> CommandError {
    CommandError::Config(ConfigError::MissingReference {
        kind: "account",
        name: name.to_string(),
    })
}

async fn rm(app: &App, arg: AccountArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let account = config
        .account(&arg.account)
        .ok_or_else(|| unknown_account(&arg.account))?;
    let remove = ConfigEdit::RemoveAccount(arg.account.clone());
    config.clone().apply(remove.clone())?;
    if matches!(account.credential(), CredentialSource::CodexLogin { .. }) {
        let session = app.session(&config).await?;
        let provider = ProviderName::for_account(account.name(), Agent::Codex);
        match session.gateway().delete_provider(&provider).await {
            Ok(()) | Err(GatewayError::NotFound { .. }) => {}
            Err(error) => return Err(error.into()),
        }
    }
    app.store.edit(vec![remove]).await?;
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
