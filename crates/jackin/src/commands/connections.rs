// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod details;
mod ls;
mod overview;
mod providers;

pub(super) use overview::overview;
pub(super) use providers::show as providers;

use std::path::PathBuf;

use jackin_core::{Secret, TerminalText};
use jackin_usage::accounts::{AccountRegistry, ConnectedAccount, UsageProvider};
use jackin_usage::dashboard::DashboardMemory;
use jackin_usage::onboarding::{LoginError, LoginPrompt, OwnedLogin, open_browser};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::app::App;
use crate::cli::{UsageAccountCommand, UsageCommand, UsageFilter};
use crate::error::CommandError;
use crate::terminal::TerminalInput;

fn database(app: &App) -> Result<PathBuf, CommandError> {
    Ok(app
        .store
        .path()
        .as_path()
        .parent()
        .ok_or_else(|| CommandError::Environment("usage database needs a config directory".into()))?
        .join(jackin_usage::accounts::DATABASE_PATH))
}

fn failure(e: impl std::fmt::Display) -> CommandError {
    CommandError::Session(e.to_string())
}

fn cancelled() -> CommandError {
    CommandError::Cancelled("usage operation cancelled".into())
}

fn input_secret(bytes: &[u8], optional: bool) -> Result<Secret, CommandError> {
    let value = std::str::from_utf8(bytes)
        .map_err(|_| CommandError::Argument("invalid key encoding".into()))?
        .trim();
    if value.is_empty() && !optional {
        return Err(CommandError::Argument("enter a value".into()));
    }
    Ok(Secret::new(value.to_owned()))
}

async fn hidden_input(app: &mut App, prompt: &str, optional: bool) -> Result<Secret, CommandError> {
    let terminal = app.terminal.as_mut().ok_or(CommandError::NoTerminal)?;
    let _restore = terminal.enter_raw().map_err(CommandError::Terminal)?;
    app.io.ask(prompt);
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    loop {
        let input = tokio::select! {
            biased;
            () = app.cancel.cancelled() => return Err(cancelled()),
            input = terminal.next_input() => input,
        };
        match input {
            TerminalInput::Bytes(chunk) => {
                for byte in zeroize::Zeroizing::new(chunk).iter().copied() {
                    match byte {
                        3 | 4 | 27 => return Err(cancelled()),
                        b'\n' | b'\r' => {
                            app.io.ask("\r\n");
                            return input_secret(&bytes, optional);
                        }
                        8 | 127 => {
                            bytes.pop();
                        }
                        32..=126 if bytes.len() < 16384 => bytes.push(byte),
                        _ => return Err(CommandError::Argument("invalid key input".into())),
                    }
                }
            }
            TerminalInput::Resized(_) => {}
            TerminalInput::Closed => return Err(cancelled()),
        }
    }
}

pub(super) async fn account(
    app: &mut App,
    command: UsageAccountCommand,
) -> Result<(), CommandError> {
    if let UsageAccountCommand::Ls(args) = &command {
        return ls::show(app, args);
    }
    if let UsageAccountCommand::Connect {
        provider,
        region,
        auth,
    } = command
    {
        let provider = providers::connector(&provider)?;
        return connect(app, provider, auth, region.as_deref(), None).await;
    }
    let mut db = service(app)?.with_registry(Ok).await.map_err(failure)?;
    match command {
        UsageAccountCommand::Ls(_) | UsageAccountCommand::Connect { .. } => Ok(()),
        UsageAccountCommand::Hub(command) => hub(app, &mut db, command).await,
        UsageAccountCommand::ReplaceKey { id, region } => {
            if !db
                .uses_api_key(id)
                .map_err(|e| CommandError::Argument(e.to_string()))?
            {
                return Err(CommandError::Argument(
                    "this account uses provider sign-in; use reconnect".into(),
                ));
            }
            reconnect(app, &db, id, region.as_deref(), None).await
        }
        UsageAccountCommand::Reconnect { id, region, auth } => {
            reconnect(app, &db, id, region.as_deref(), auth).await
        }

        UsageAccountCommand::Rename { id, label } => service(app)?
            .with_registry(move |db| db.rename(id, &label))
            .await
            .map_err(failure),
        UsageAccountCommand::Rm { id } => service(app)?
            .with_registry(move |mut db| db.remove(id))
            .await
            .map_err(failure),
    }
}

async fn reconnect(
    app: &mut App,
    db: &AccountRegistry,
    id: Uuid,
    region: Option<&str>,
    auth: Option<jackin_usage::onboarding::UsageAuth>,
) -> Result<(), CommandError> {
    let current = db
        .list()
        .map_err(failure)?
        .into_iter()
        .find(|row| row.id == id)
        .ok_or_else(|| CommandError::Argument("unknown usage account".into()))?;
    connect(app, current.provider, auth, region, Some(id)).await
}

fn service(app: &App) -> Result<OwnedLogin, CommandError> {
    let login = OwnedLogin::new(
        database(app)?,
        app.deps.usage_endpoints.clone(),
        Arc::clone(&app.deps.host.clock),
    )
    .with_google_registration(
        jackin_usage::onboarding::GoogleRegistrationFile::from_environment(
            &app.env,
            &app.cwd,
            Arc::clone(&app.deps.host.fs),
        ),
    );
    Ok(match &app.deps.usage_vault {
        Some(vault) => login.with_vault(Arc::clone(vault)),
        None => login,
    })
}

async fn connect(
    app: &mut App,
    provider: UsageProvider,
    auth: Option<jackin_usage::onboarding::UsageAuth>,
    region: Option<&str>,
    replace: Option<Uuid>,
) -> Result<(), CommandError> {
    if app.terminal.is_none() {
        return Err(CommandError::NoTerminal);
    }
    let service = service(app)?;
    let cancel = app.cancel.child_token();
    let (send, mut prompts) = mpsc::channel(4);
    let drive = async {
        while let Some(prompt) = prompts.recv().await {
            handle_prompt(app, prompt).await?;
        }
        std::future::pending::<Result<(), CommandError>>().await
    };
    let id = tokio::select! {
        biased;
        result=service.connect_with_auth(provider,auth,region,replace,send,&cancel)=>result.map_err(|e|if matches!(e,LoginError::Cancelled){cancelled()}else{failure(e)})?,
        result=drive=>{cancel.cancel();return result;}
    };
    app.io.line(&id.to_string())
}

async fn handle_prompt(app: &mut App, prompt: LoginPrompt) -> Result<(), CommandError> {
    match prompt {
        LoginPrompt::Key { url, reply } => {
            app.io.note(&format!("Create a key at {url}"));
            let key = hidden_input(app, "API key (hidden, Esc cancels): ", false).await?;
            reply.send(key).map_err(|_| cancelled())?;
        }
        LoginPrompt::Browser { url, paste } => {
            app.io
                .note(&format!("Authorize this account in your browser: {url}"));
            let code = loop {
                let code = hidden_input(
                    app,
                    "Waiting for the browser. Paste a signed response only if needed (Esc cancels): ",
                    true,
                )
                .await?;
                if !code.expose().is_empty() {
                    break code;
                }
            };
            paste.send(code).map_err(|_| cancelled())?;
        }
        LoginPrompt::Device { url, code } => {
            app.io.note(&format!("Enter {} at {url}", code.expose()));
            app.io
                .ask("Press Enter to open the browser. Waiting for authorization… ");
            let terminal = app.terminal.as_mut().ok_or(CommandError::NoTerminal)?;
            let _restore = terminal.enter_raw().map_err(CommandError::Terminal)?;
            loop {
                match terminal.next_input().await {
                    TerminalInput::Bytes(bytes)
                        if bytes.iter().any(|b| matches!(b, 3 | 4 | 27)) =>
                    {
                        return Err(cancelled());
                    }
                    TerminalInput::Bytes(bytes) if bytes.iter().any(|b| matches!(b, 10 | 13)) => {
                        if let Err(error) = open_browser(&url).await {
                            app.io.note(&error.to_string());
                        }
                    }
                    TerminalInput::Closed => return Err(cancelled()),
                    _ => {}
                }
            }
        }
        LoginPrompt::Field {
            label,
            optional,
            masked,
            reply,
        } => {
            let value = if masked {
                hidden_input(app, &format!("{label} (hidden): "), optional).await?
            } else {
                app.io.ask(&format!("{label}: "));
                let value = app
                    .terminal
                    .as_mut()
                    .ok_or(CommandError::NoTerminal)?
                    .read_line()
                    .await
                    .map_err(CommandError::Terminal)?
                    .ok_or_else(cancelled)?;
                Secret::new(value.trim().to_owned())
            };
            reply.send(value).map_err(|_| cancelled())?;
        }
        LoginPrompt::AntigravityStatusLine { command } => {
            app.io.note("In another terminal, run this command and sign in to Antigravity with the selected Google account:");
            app.io.note(&command);
            app.io
                .note("Waiting for the first quota report. Press Esc to cancel.");
            let terminal = app.terminal.as_mut().ok_or(CommandError::NoTerminal)?;
            let _restore = terminal.enter_raw().map_err(CommandError::Terminal)?;
            loop {
                match terminal.next_input().await {
                    TerminalInput::Closed => return Err(cancelled()),
                    TerminalInput::Bytes(bytes)
                        if bytes.iter().any(|b| matches!(b, 3 | 4 | 27)) =>
                    {
                        return Err(cancelled());
                    }
                    _ => {}
                }
            }
        }
        LoginPrompt::Callback { url } => app.io.note(&format!(
            "Authorize your Google account in the browser: {url}"
        )),
        LoginPrompt::BrowserUnavailable => {
            app.io.note("Open the displayed URL to continue.");
        }
    }
    Ok(())
}

pub(super) async fn usage(app: &mut App, command: UsageCommand) -> Result<(), CommandError> {
    if let UsageCommand::Proxy { off } = command {
        let proxy = if off {
            None
        } else {
            Some(
                hidden_input(
                    app,
                    "Usage proxy URL including optional credentials (hidden): ",
                    false,
                )
                .await?,
            )
        };
        return service(app)?
            .configure_proxy(proxy.as_ref())
            .await
            .map_err(failure);
    }
    if matches!(command, UsageCommand::Share) {
        let Some(db) = AccountRegistry::open_read_only(&database(app)?).map_err(failure)? else {
            return Err(CommandError::NoUsageSnapshot);
        };
        let dashboard = jackin_usage::dashboard::Dashboard::load(
            &db,
            &mut DashboardMemory::default(),
            app.deps.host.clock.now().0,
        )
        .map_err(failure)?;
        return app.io.json(&dashboard.redacted_export());
    }

    if let UsageCommand::Redeem { account, reset } = command {
        return redeem(app, account, reset.get() - 1).await;
    }
    match command {
        UsageCommand::AntigravityStatusline {
            database: destination,
        } => {
            use std::io::{IsTerminal, Read};
            if std::io::stdin().is_terminal() {
                return Err(CommandError::Argument(
                    "Pipe an Antigravity status-line payload on stdin. Direct quota sign-in is jackin account usage connect antigravity.".into(),
                ));
            }
            const LIMIT: u64 = 1024 * 1024;
            let mut input = zeroize::Zeroizing::new(Vec::new());
            std::io::stdin()
                .lock()
                .take(LIMIT + 1)
                .read_to_end(&mut input)
                .map_err(failure)?;
            if input.len() as u64 > LIMIT {
                return Err(CommandError::Argument(
                    "Antigravity status line exceeds 1 MiB".into(),
                ));
            }
            let destination = destination.map_or_else(|| database(app), Ok)?;
            let stored = jackin_usage::onboarding::receive_antigravity_statusline(
                &destination,
                &input,
                app.deps.host.clock.now().0,
            )
            .map_err(failure)?;
            app.io.line(if stored {
                "Jackin · quota received"
            } else {
                "Jackin · waiting for a connected Google account"
            })
        }
        UsageCommand::Refresh { filter, json } => refresh(app, filter, json).await,
        UsageCommand::Snapshot { filter, json } => overview::snapshot(app, &filter, json),
        UsageCommand::Details { filter, json } => details::show(app, &filter, json),
        UsageCommand::Providers { .. }
        | UsageCommand::Host(_)
        | UsageCommand::Share
        | UsageCommand::Proxy { .. } => Err(CommandError::Argument("choose a usage filter".into())),
        UsageCommand::Redeem { .. } => Err(CommandError::Argument("invalid reset action".into())),
    }
}

async fn refresh(app: &mut App, filter: UsageFilter, json: bool) -> Result<(), CommandError> {
    let provider = filter
        .provider
        .as_deref()
        .map(providers::connector)
        .transpose()?;
    let db = service(app)?.with_registry(Ok).await.map_err(failure)?;
    let outcome = refresh_selected(app, &db, &filter, provider, json).await?;
    if json {
        return overview::emit(
            app,
            Some(&db),
            &filter,
            provider,
            &outcome.messages,
            outcome.first_error,
        );
    }
    overview::refreshed(app, &db, &filter, provider, &outcome)?;
    let accounts = outcome.messages.len() - outcome.skipped.len();
    match outcome.first_error {
        None => Ok(()),
        Some(_) => Err(failure(failure_summary(accounts, outcome.hubs_failed))),
    }
}

fn failure_summary(accounts: usize, hubs: usize) -> String {
    let count = |n: usize, noun: &str| format!("{n} {noun}{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if accounts > 0 {
        parts.push(count(accounts, "account"));
    }
    if hubs > 0 {
        parts.push(count(hubs, "hub"));
    }
    format!("refresh failed for {}", parts.join(" and "))
}

#[derive(Default)]
pub(super) struct RefreshOutcome {
    first_error: Option<CommandError>,
    messages: Vec<(Uuid, String)>,
    skipped: Vec<Uuid>,
    hubs_failed: usize,
}

impl RefreshOutcome {
    fn message(&self, id: Uuid) -> Option<&str> {
        self.messages
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map(|(_, message)| message.as_str())
    }

    fn skipped(&self, id: Uuid) -> bool {
        self.skipped.contains(&id)
    }

    fn failed(&self, id: Uuid) -> bool {
        self.message(id).is_some() && !self.skipped(id)
    }
}

async fn refresh_selected(
    app: &mut App,
    db: &AccountRegistry,
    filter: &UsageFilter,
    provider: Option<UsageProvider>,
    noisy: bool,
) -> Result<RefreshOutcome, CommandError> {
    let mut outcome = RefreshOutcome::default();
    let owned = service(app)?;
    if provider.is_none() && filter.account.is_none() {
        for hub in db.hubs().map_err(failure)? {
            if let Err(error) = owned.sync_hub(hub.id, &app.cancel).await {
                if app.cancel.is_cancelled() {
                    return Err(cancelled());
                }
                let text = format!("{}: {error}", hub.id);
                if noisy {
                    app.io.note(&text);
                } else {
                    overview::note(app, &text);
                }
                outcome.hubs_failed += 1;
                outcome.first_error.get_or_insert_with(|| failure(error));
            }
        }
    }
    let accounts = db.list().map_err(failure)?;
    let mut accounts = accounts
        .into_iter()
        .filter(|a| selected(a, filter, provider));
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        while tasks.len() < 4 {
            let Some(account) = accounts.next() else {
                break;
            };
            let service = owned.clone();
            let cancel = app.cancel.clone();
            tasks.spawn(async move { (account.id, service.refresh(&account, &cancel).await) });
        }
        let next = tokio::select! {
            biased;
            () = app.cancel.cancelled() => return Err(cancelled()),
            next = tasks.join_next() => next,
        };
        let Some(result) = next else { break };
        let (id, result) = result.map_err(|_| failure(LoginError::Worker))?;
        if let Err(error) = result {
            outcome.messages.push((id, error.to_string()));
            if noisy {
                app.io.note(&format!("{id}: {error}"));
            }
            if matches!(error, LoginError::NotDirect) {
                outcome.skipped.push(id);
            } else {
                outcome.first_error.get_or_insert_with(|| failure(error));
            }
        }
    }
    Ok(outcome)
}

fn selected(
    account: &ConnectedAccount,
    filter: &UsageFilter,
    provider: Option<UsageProvider>,
) -> bool {
    provider.is_none_or(|p| p == account.provider)
        && filter.account.is_none_or(|id| id == account.id)
}

async fn hub(
    app: &mut App,
    db: &mut AccountRegistry,
    command: crate::cli::HubCommand,
) -> Result<(), CommandError> {
    use crate::cli::HubCommand;
    let service = service(app)?;
    match command {
        HubCommand::Ls => app.io.json(&db.hubs().map_err(failure)?),
        HubCommand::Rm { id } => service.remove_hub(id).await.map_err(failure),
        HubCommand::Sync { id } => service.sync_hub(id, &app.cancel).await.map_err(failure),
        HubCommand::Connect { .. } | HubCommand::ReplaceKey { .. } => {
            let (address, id) = match command {
                HubCommand::Connect { address } => (address, None),
                HubCommand::ReplaceKey { id } => {
                    let current = db
                        .hubs()
                        .map_err(failure)?
                        .into_iter()
                        .find(|hub| hub.id == id)
                        .ok_or_else(|| CommandError::Argument("unknown hub".into()))?;
                    (current.address, Some(id))
                }
                _ => {
                    return Err(CommandError::Argument(
                        "choose Connect or Replace Key".into(),
                    ));
                }
            };
            let key =
                hidden_input(app, "Hub management key (hidden, Esc cancels): ", false).await?;
            let id = service
                .connect_hub(&address, &key, id, &app.cancel)
                .await
                .map_err(failure)?;
            app.io.line(&id.to_string())
        }
    }
}

async fn redeem(app: &mut App, id: Uuid, index: usize) -> Result<(), CommandError> {
    let db = AccountRegistry::open_read_only(&database(app)?)
        .map_err(failure)?
        .ok_or_else(|| CommandError::Argument("no saved usage accounts".into()))?;
    let (account, _, snapshot) = db
        .snapshots()
        .map_err(failure)?
        .into_iter()
        .find(|(a, _, _)| a.id == id)
        .ok_or_else(|| CommandError::Argument("unknown usage account".into()))?;
    let snapshot = snapshot.ok_or_else(|| {
        CommandError::Argument("refresh this account to read its reset inventory".into())
    })?;
    let selection = jackin_usage::resets::ResetSelection::from_snapshot(
        id,
        account.provider,
        &snapshot,
        index,
        app.deps.host.clock.now(),
    )
    .map_err(failure)?;
    let terminal = app.terminal.as_mut().ok_or(CommandError::NoTerminal)?;
    app.io.ask(&format!(
        "{} ({}) — {} Type RESET to confirm: ",
        TerminalText::sanitize(account.label.as_bytes()),
        id,
        selection.description()
    ));
    let response = tokio::select! {
        biased;
        ()=app.cancel.cancelled()=>return Err(cancelled()),
        line=terminal.read_line()=>line.map_err(CommandError::Terminal)?,
    };
    if response.as_deref().map(str::trim) != Some("RESET") {
        return Err(cancelled());
    }
    let result = service(app)?
        .redeem_reset(selection, &app.cancel)
        .await
        .map_err(failure)?;
    app.io.line(&result.to_string())
}
