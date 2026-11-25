// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod details;
mod providers;

pub(super) use providers::show as providers;

use std::path::PathBuf;

use jackin_core::{Secret, TerminalText};
use jackin_usage::accounts::{AccountRegistry, ConnectedAccount, UsageProvider};
use jackin_usage::dashboard::DashboardMemory;
use jackin_usage::onboarding::{LoginError, LoginPrompt, OwnedLogin, open_browser};
use serde_json::json;
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
    let path = database(app)?;
    if let UsageAccountCommand::Ls(args) = command {
        let rows = match AccountRegistry::open_read_only(&path).map_err(failure)? {
            Some(db) => db.list().map_err(failure)?,
            None => Vec::new(),
        };
        if args.json {
            return app.io.json(&rows);
        }
        for row in rows {
            app.io.line(&format!(
                "{}\t{}\t{}",
                row.id,
                row.provider.as_str(),
                TerminalText::sanitize(row.label.as_bytes())
            ))?;
        }
        return Ok(());
    }
    if let UsageAccountCommand::Connect {
        provider,
        region,
        auth,
    } = command
    {
        let provider = providers::connector(&provider)?;
        if provider == UsageProvider::Gemini {
            app.io.note("Connecting Gemini Code Assist for a Google Cloud project. For a personal Gemini subscription, run jackin usage providers google-ai-ultra.");
        }
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
    Ok(OwnedLogin::new(
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
    ))
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
        LoginPrompt::Callback { url } => app.io.note(&format!(
            "Authorize your Google account in the browser: {url}"
        )),
        LoginPrompt::BrowserUnavailable => {
            app.io.note("Open the displayed URL to continue.");
        }
    }
    Ok(())
}

pub(super) fn overview(app: &mut App, json: bool) -> Result<(), CommandError> {
    let db = AccountRegistry::open_read_only(&database(app)?).map_err(failure)?;
    if json {
        let snapshots = db
            .as_ref()
            .map(AccountRegistry::snapshots)
            .transpose()
            .map_err(failure)?
            .unwrap_or_default();
        let now = app.deps.host.clock.now().0;
        let catalog = providers::catalog(snapshots.iter().map(|(account, _, _)| account.provider));
        let accounts: Vec<_> = snapshots
            .iter()
            .map(|(account, at, payload)| {
                jackin_usage::export::snapshot_account(account, *at, payload.as_ref(), now)
            })
            .collect();
        app.io.json(
            &json!({"schema":1,"generated_at":now,"accounts":accounts,"providers":catalog}),
        )?;
        return if snapshots.iter().any(|(_, _, payload)| payload.is_some()) {
            Ok(())
        } else {
            Err(CommandError::NoUsageSnapshot)
        };
    }
    let accounts = db
        .as_ref()
        .map(AccountRegistry::list)
        .transpose()
        .map_err(failure)?
        .unwrap_or_default();
    let catalog = providers::catalog(accounts.iter().map(|a| a.provider));
    let Some(db) = db else {
        for provider in catalog {
            app.io.line(&provider.to_string())?;
        }
        app.io.note("To connect your first provider, open Accounts in the console or run jackin account usage connect <provider>.");
        return Err(CommandError::NoUsageSnapshot);
    };
    let board = jackin_usage::dashboard::Dashboard::load(
        &db,
        &mut DashboardMemory::default(),
        app.deps.host.clock.now().0,
    )
    .map_err(failure)?;
    let mut group = None;
    for row in &board.all {
        let next = (row.provider, row.account, row.label.as_str());
        if group != Some(next) {
            let heading = format!("{} · {}", row.provider, row.label);
            app.io
                .line(&TerminalText::sanitize(heading.as_bytes()).to_string())?;
            group = Some(next);
        }
        let mut columns = vec![
            jackin_core::plain(&row.metric_label),
            jackin_usage::presentation::metric_value(row, board.settings.remaining),
        ];
        if let Some(reset) = &row.reset {
            columns.push(jackin_usage::presentation::reset_caption(
                reset,
                board.settings.exact_resets,
            ));
        }
        if row.stale {
            columns.push("stale".into());
        }
        let line = format!("  {}", columns.join(" · "));
        app.io
            .line(&TerminalText::sanitize(line.as_bytes()).to_string())?;
    }
    if board.all.is_empty() && board.waiting.is_empty() {
        app.io.line(&board.summary.to_string())?;
    }
    for (index, account) in accounts.iter().enumerate() {
        let notice = board
            .notices
            .iter()
            .find(|notice| notice.account == account.id);
        if board
            .all
            .iter()
            .any(|row| row.account == Some(account.id) || row.members.contains(&account.id))
        {
            if let Some(notice) = notice {
                app.io.note(
                    &TerminalText::sanitize(
                        format!("{}: {}", notice.label, notice.message).as_bytes(),
                    )
                    .to_string(),
                );
            }
            continue;
        }
        let label = if board.settings.conceal_names {
            jackin_usage::dashboard::concealed_label(index)
        } else {
            account.label.clone()
        };
        let status = if let Some(notice) = notice {
            notice.message.as_str()
        } else if board.waiting.contains(&account.id) {
            "Waiting for a first usage reading"
        } else {
            "No displayed readings"
        };
        let line = format!("{} · {label} · {status}", account.provider);
        app.io
            .line(&TerminalText::sanitize(line.as_bytes()).to_string())?;
    }
    for provider in catalog.iter().filter(|p| p.account_count == 0) {
        app.io.line(&provider.to_string())?;
    }
    if board.has_snapshot {
        Ok(())
    } else {
        Err(CommandError::NoUsageSnapshot)
    }
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
    let active = matches!(command, UsageCommand::Refresh(_));
    let filter = match command {
        UsageCommand::Refresh(f) | UsageCommand::Snapshot(f) => f,
        UsageCommand::Details { filter, json } => return details::show(app, &filter, json),
        UsageCommand::Providers { .. }
        | UsageCommand::Host(_)
        | UsageCommand::Share
        | UsageCommand::Proxy { .. } => {
            return Err(CommandError::Argument("choose a usage filter".into()));
        }
        UsageCommand::Redeem { .. } => {
            return Err(CommandError::Argument("invalid reset action".into()));
        }
    };
    let provider = filter
        .provider
        .as_deref()
        .map(providers::connector)
        .transpose()?;
    let path = database(app)?;
    let mut db = if active {
        Some(service(app)?.with_registry(Ok).await.map_err(failure)?)
    } else {
        AccountRegistry::open_read_only(&path).map_err(failure)?
    };
    let mut rows = Vec::new();
    let mut first_error = None;
    if let Some(db) = db.as_mut() {
        if active {
            first_error = refresh_selected(app, db, &filter, provider).await?;
        }
        let now = app.deps.host.clock.now().0;
        for (account, fetched_at, payload) in db.snapshots().map_err(failure)? {
            if selected(&account, &filter, provider) {
                rows.push(jackin_usage::export::snapshot_account(
                    &account,
                    fetched_at,
                    payload.as_ref(),
                    now,
                ));
            }
        }
    }
    if rows.is_empty() && (provider.is_some() || filter.account.is_some()) {
        return Err(CommandError::Argument("no matching usage account".into()));
    }
    app.io
        .json(&json!({"schema":1,"generated_at":app.deps.host.clock.now().0,"accounts":rows}))?;
    if let Some(error) = first_error {
        return Err(error);
    }
    if rows.iter().all(|r| r["usage"].is_null()) {
        return Err(CommandError::NoUsageSnapshot);
    }
    Ok(())
}

async fn refresh_selected(
    app: &mut App,
    db: &AccountRegistry,
    filter: &UsageFilter,
    provider: Option<UsageProvider>,
) -> Result<Option<CommandError>, CommandError> {
    let mut first_error = None;
    let owned = service(app)?;
    if provider.is_none() && filter.account.is_none() {
        for hub in db.hubs().map_err(failure)? {
            if let Err(error) = owned.sync_hub(hub.id, &app.cancel).await {
                if app.cancel.is_cancelled() {
                    return Err(cancelled());
                }
                app.io.note(&format!("{}: {error}", hub.id));
                first_error.get_or_insert_with(|| failure(error));
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
            app.io.note(&format!("{id}: {error}"));
            first_error.get_or_insert_with(|| failure(error));
        }
    }
    Ok(first_error)
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
