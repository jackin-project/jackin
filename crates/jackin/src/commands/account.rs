// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, ConfigEdit, ConfigError, CredentialSource, read_credential_file,
};
use jackin_session::{AccountPlan, Gateways, plan_account_edits, read_chatgpt_login};

use crate::app::App;
use crate::cli::{
    AccountAddArgs, AccountArg, AccountCommand, BindArgs, BindScope, CredentialArg, JsonArg,
    parse_provider,
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
    match args.credential.as_deref().map(CredentialArg::parse) {
        Some(Ok(CredentialArg::Env(var))) => {
            if app.env.get(&var).is_none() {
                return Err(CommandError::Argument(
                    "--credential env:NAME needs a variable that is set in this environment"
                        .to_owned(),
                ));
            }
            Ok(CredentialSource::Env { var })
        }
        Some(Ok(CredentialArg::OnePassword(reference))) => Ok(CredentialSource::OnePassword {
            reference,
            account: None,
        }),
        Some(Ok(CredentialArg::File(path))) => {
            let refused = || {
                CommandError::Argument(
                    "--credential file:PATH needs an existing regular file of at most 1 MiB"
                        .to_owned(),
                )
            };
            let path = absolutize(&path, &app.cwd).map_err(|_| refused())?;
            match read_credential_file(app.deps.host.fs.as_ref(), &path).await {
                Ok(Ok(_)) => Ok(CredentialSource::File { path }),
                Ok(Err(_)) | Err(_) => Err(refused()),
            }
        }
        Some(Err(error)) => Err(error),
        None => Err(CommandError::Argument(
            "pass --chatgpt-from or --credential".to_owned(),
        )),
    }
}

async fn add(app: &App, args: AccountAddArgs) -> Result<(), CommandError> {
    let provider = parse_provider(&args.provider)?;
    let account = Account::new(args.name.clone(), provider, source(app, &args).await?)?;
    apply(app, vec![ConfigEdit::AddAccount(account)]).await
}

async fn apply(app: &App, edits: Vec<ConfigEdit>) -> Result<(), CommandError> {
    match plan_account_edits(&app.desk(), edits, &app.cancel).await? {
        AccountPlan::Offline(plan) => {
            plan.apply().await?;
        }
        AccountPlan::Gateway(plan) => {
            let gateways = if plan.needs_every_gateway() {
                app.gateways(plan.config()).await?
            } else {
                Gateways::single(app.session(plan.config()).await?)
            };
            plan.apply(&gateways).await?;
        }
    }
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
    apply(app, vec![ConfigEdit::RemoveAccount(arg.account)]).await
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
