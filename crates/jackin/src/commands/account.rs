// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, ConfigEdit, ConfigError, CredentialSource, read_credential_file,
};
use jackin_session::{
    AccountPlan, plan_account_edits, plan_account_edits_for_agent, read_chatgpt_login,
    read_omp_host_key,
};

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
    if args.from_host_login {
        let provider = parse_provider(&args.provider)?;
        let home = app
            .env
            .get(
                &jackin_core::EnvName::parse("HOME")
                    .map_err(|_| CommandError::Argument("HOME is not set".to_owned()))?,
            )
            .ok_or_else(|| CommandError::Argument("HOME is not set".to_owned()))?;
        let relative = match provider {
            jackin_core::Provider::Grok => ".grok/auth.json",
            jackin_core::Provider::Muse => ".config/muse/auth.json",
            jackin_core::Provider::Claude => ".claude/.credentials.json",
            jackin_core::Provider::Antigravity => ".gemini/antigravity-cli/settings.json",
            _ => {
                return Err(CommandError::Argument(
                    "--from-host-login needs the grok, muse, claude, or antigravity provider"
                        .to_owned(),
                ));
            }
        };
        let path =
            jackin_core::HostPath::parse(std::path::Path::new(&format!("{home}/{relative}")))
                .map_err(|_| CommandError::Argument("HOME is not usable".to_owned()))?;
        match provider {
            jackin_core::Provider::Grok => {
                jackin_session::read_grok_login(app.deps.host.fs.as_ref(), &path)
                    .await
                    .map_err(|_| {
                        CommandError::Argument("no grok host login is available".to_owned())
                    })?;
            }
            jackin_core::Provider::Claude => {
                jackin_session::read_claude_host_login(&app.deps.host, &path, &app.cancel)
                    .await
                    .map_err(|_| {
                        CommandError::Argument("no claude host login is available".to_owned())
                    })?;
            }
            jackin_core::Provider::Antigravity => {
                jackin_session::read_antigravity_host_login(&app.deps.host, &app.cancel)
                    .await
                    .map_err(|_| {
                        CommandError::Argument("no antigravity host login is available".to_owned())
                    })?;
            }
            _ => {
                jackin_session::read_muse_host_metadata(app.deps.host.fs.as_ref(), &path)
                    .await
                    .map_err(|_| {
                        CommandError::Argument("no muse host login is available".to_owned())
                    })?;
            }
        }
        return Ok(CredentialSource::HostLogin { path });
    }
    if matches!(
        args.credential.as_deref().map(CredentialArg::parse),
        Some(Ok(CredentialArg::OmpHost))
    ) {
        let provider = parse_provider(&args.provider)?;
        let tuple = jackin_core::lookup_tuple(Agent::Omp, provider).map_err(|_| {
            CommandError::Argument("omp-host needs an omp tuple provider".to_owned())
        })?;
        let provider_id = tuple.omp_provider_id().ok_or_else(|| {
            CommandError::Argument("omp-host needs an omp tuple provider".to_owned())
        })?;
        let home = app
            .env
            .get(
                &jackin_core::EnvName::parse("HOME")
                    .map_err(|_| CommandError::Argument("HOME is not set".to_owned()))?,
            )
            .ok_or_else(|| CommandError::Argument("HOME is not set".to_owned()))?;
        let db = jackin_core::HostPath::parse(std::path::Path::new(&format!(
            "{home}/.omp/agent/agent.db"
        )))
        .map_err(|_| CommandError::Argument("HOME is not usable".to_owned()))?;
        let _key =
            read_omp_host_key(app.deps.host.fs.as_ref(), &db, provider_id).map_err(|_| {
                CommandError::Argument(
                    "no omp host credential for the selected provider".to_owned(),
                )
            })?;
        return Ok(CredentialSource::OmpHost);
    }
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
        Some(Ok(CredentialArg::OmpHost)) => Ok(CredentialSource::OmpHost),
        Some(Err(error)) => Err(error),
        None => Err(CommandError::Argument(
            "pass --chatgpt-from or --credential".to_owned(),
        )),
    }
}

fn default_agent(provider: jackin_core::Provider) -> Agent {
    match provider {
        jackin_core::Provider::Grok => Agent::GrokBuild,
        jackin_core::Provider::Muse => Agent::MuseCode,
        jackin_core::Provider::Claude => Agent::ClaudeCode,
        jackin_core::Provider::Antigravity => Agent::Antigravity,
        jackin_core::Provider::Kimi => Agent::KimiCode,
        jackin_core::Provider::Mimo => Agent::MimoCode,
        jackin_core::Provider::Codex => Agent::Codex,
        _ => Agent::Omp,
    }
}

async fn add(app: &App, args: AccountAddArgs) -> Result<(), CommandError> {
    let provider = parse_provider(&args.provider)?;
    let agent = match &args.agent {
        Some(text) => Some(crate::cli::parse_agent(text)?),
        None => None,
    };
    let credential = source(app, &args).await?;
    if agent.is_none()
        && matches!(
            provider,
            jackin_core::Provider::Claude | jackin_core::Provider::Codex
        )
        && matches!(
            credential,
            CredentialSource::Env { .. }
                | CredentialSource::OnePassword { .. }
                | CredentialSource::File { .. }
                | CredentialSource::OmpHost
        )
    {
        return Err(CommandError::ApiKeyNeedsAgent { provider });
    }
    let account = Account::new(args.name.clone(), provider, credential)?;
    let agent = agent.unwrap_or(default_agent(provider));
    match plan_account_edits_for_agent(
        &app.desk(),
        vec![ConfigEdit::AddAccount(account)],
        agent,
        &app.cancel,
    )
    .await?
    {
        AccountPlan::Offline(plan) => {
            plan.apply().await?;
        }
        AccountPlan::Gateway(plan) => {
            let gateways = app.gateways(plan.config()).await?;
            plan.apply(&gateways).await?;
        }
    }
    Ok(())
}

async fn apply(app: &App, edits: Vec<ConfigEdit>) -> Result<(), CommandError> {
    match plan_account_edits(&app.desk(), edits, &app.cancel).await? {
        AccountPlan::Offline(plan) => {
            plan.apply().await?;
        }
        AccountPlan::Gateway(plan) => {
            let gateways = app.gateways(plan.config()).await?;
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
            let agent = account
                .provider()
                .tuples()
                .first()
                .map(|tuple| tuple.agent)
                .filter(|agent| *agent == Agent::Codex)
                .ok_or_else(|| {
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
