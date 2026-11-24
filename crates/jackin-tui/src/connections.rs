// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod board;

use futures::{StreamExt, stream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use jackin_console::backend::Ticket;
use jackin_console::usage_connections::{UsageAction, UsageConnections, UsageEntry, UsageUpdate};
use jackin_console::world::Msg;
use jackin_core::Secret;
use jackin_usage::accounts::{AccountRegistry, ConnectedAccount, ConnectionError};
use jackin_usage::onboarding::{LoginPrompt, OwnedLogin, open_browser};
use jackin_usage::presentation as display;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

use crate::backend::JackinBackend;
use crate::observe::locked;

#[derive(Debug, thiserror::Error)]
enum AdapterError {
    #[error("usage database needs a config directory")]
    ConfigDirectory,
    #[error("invalid account ID")]
    AccountId,
    #[error("Sign-in cancelled")]
    Cancelled,
    #[error("Usage dashboard interrupted")]
    Dashboard,
    #[error("Sign-in interrupted")]
    SignIn,
    #[error("Usage account operation interrupted")]
    Operation,
    #[error("invalid hub ID")]
    HubId,
    #[error("account was removed")]
    Removed,
    #[error("Reset inventory unavailable")]
    NoResets,
    #[error("Reset inventory changed. Review the latest inventory before confirming.")]
    InventoryChanged,
    #[error("Unsupported account action")]
    Unsupported,
}

#[derive(Default)]
pub(crate) struct Schedule {
    next: AtomicI64,
    next_hub: AtomicI64,
    active: AtomicBool,
}

pub(crate) struct Attempt {
    ticket: Ticket,
    cancel: CancellationToken,
    reply: Option<oneshot::Sender<Secret>>,
    url: Option<Url>,
}

impl Drop for Attempt {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn message(ticket: Ticket, update: UsageUpdate) -> Msg {
    Msg::UsageConnection { ticket, update }
}

fn failure(ticket: Ticket, e: impl std::fmt::Display) -> Msg {
    message(ticket, UsageUpdate::Failed(e.to_string()))
}

fn entries(service: &OwnedLogin) -> Result<Vec<UsageEntry>, ConnectionError> {
    Ok(display::entries(service)?
        .into_iter()
        .map(|a| UsageEntry {
            id: a.id,
            provider: a.provider,
            label: a.label,
            summary: a.summary.into_boxed_slice(),
            details: a.details,
            hub: a.hub,
            region: a.region,
            device_login: a.device_login,
            resets: a
                .resets
                .into_iter()
                .map(|r| jackin_console::usage_connections::ResetItem {
                    index: r.index,
                    identity: r.identity,
                    description: r.description,
                    usable: r.usable,
                })
                .collect(),
        })
        .collect())
}

fn saved(service: &OwnedLogin, id: Uuid) -> Result<UsageUpdate, ConnectionError> {
    Ok(UsageUpdate::Saved(
        entries(service)?
            .into_iter()
            .find(|entry| entry.id == id.to_string())
            .ok_or(ConnectionError::Changed)?,
    ))
}

impl JackinBackend {
    pub(crate) fn schedule_usage(&self, now: i64) {
        let schedule = &self.usage_schedule;
        if now < schedule.next.load(Ordering::Relaxed)
            || schedule.active.swap(true, Ordering::AcqRel)
        {
            return;
        }
        schedule
            .next
            .store(now.saturating_add(300), Ordering::Relaxed);
        let check_hubs = now >= schedule.next_hub.load(Ordering::Relaxed);
        if check_hubs {
            schedule
                .next_hub
                .store(now.saturating_add(3600), Ordering::Relaxed);
        }
        let backend = self.clone();
        let schedule = Arc::clone(schedule);
        let refresh = self.bridge.handle().spawn(async move {
            let Ok(service) = backend.usage_service() else {
                return;
            };
            let Ok(()) =
                refresh_accounts(&service, check_hubs, false, None, &backend.inputs.cancel).await
            else {
                return;
            };
            let Ok(Some(db)) = AccountRegistry::open_read_only(service.database()) else {
                return;
            };
            if let Ok(data) = jackin_usage::dashboard::Dashboard::load(
                &db,
                &mut locked(&backend.dashboard),
                service.now().0,
            ) {
                backend.bridge.send(Msg::UsageUpdated {
                    alerts: data.alerts,
                });
            }
        });
        self.bridge.handle().spawn(async move {
            let _settled = refresh.await;
            schedule.active.store(false, Ordering::Release);
        });
    }

    fn usage_service(&self) -> Result<OwnedLogin, String> {
        let dir = self
            .inputs
            .config
            .path()
            .as_path()
            .parent()
            .ok_or(AdapterError::ConfigDirectory.to_string())?;
        Ok(OwnedLogin::new(
            dir.join(jackin_usage::accounts::DATABASE_PATH),
            self.inputs.usage.endpoints().clone(),
            Arc::clone(&self.inputs.host.clock),
        )
        .with_google_registration(self.inputs.google_registration.clone()))
    }

    fn prompt(&self, ticket: Ticket, prompt: LoginPrompt) {
        let mut guard = locked(&self.usage_attempt);
        let Some(active) = guard
            .as_mut()
            .filter(|a| a.ticket == ticket && !a.cancel.is_cancelled())
        else {
            return;
        };
        let update = match prompt {
            LoginPrompt::Key { url, reply } => {
                active.url = Url::parse(url).ok();
                active.reply = Some(reply);
                UsageUpdate::Key { url: url.into() }
            }
            LoginPrompt::Browser { url, paste } => {
                active.reply = Some(paste);
                active.url = Some(url.clone());
                UsageUpdate::Browser {
                    url: url.into(),
                    accepts_paste: true,
                }
            }
            LoginPrompt::Device { url, code } => {
                active.url = Some(url.clone());
                UsageUpdate::Device {
                    url: url.into(),
                    code: code.expose().to_owned(),
                }
            }
            LoginPrompt::Field {
                label,
                optional,
                masked,
                reply,
            } => {
                active.reply = Some(reply);
                UsageUpdate::Field {
                    label: label.into(),
                    optional,
                    masked,
                }
            }
            LoginPrompt::Callback { url } => {
                active.url = Some(url.clone());
                UsageUpdate::Browser {
                    url: url.into(),
                    accepts_paste: false,
                }
            }
            LoginPrompt::AntigravityStatusLine { command } => {
                UsageUpdate::AntigravityStatusLine { command }
            }
            LoginPrompt::BrowserUnavailable => UsageUpdate::OpenFailed,
        };
        self.bridge.send(message(ticket, update));
    }

    async fn run_login(
        &self,
        service: &OwnedLogin,
        ticket: Ticket,
        choice: jackin_console::usage_connections::LoginChoice,
        cancel: &CancellationToken,
    ) -> Result<UsageUpdate, String> {
        let provider = choice
            .provider
            .parse()
            .map_err(|e: ConnectionError| e.to_string())?;
        let id = choice
            .replace
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| AdapterError::AccountId.to_string())?;
        let (sender, mut rx) = mpsc::channel(4);
        use jackin_usage::onboarding::UsageAuth;
        let method = choice.device_login.map(|device| {
            if device {
                UsageAuth::Device
            } else {
                UsageAuth::Key
            }
        });
        let future = service.connect_with_auth(
            provider,
            method,
            choice.region.as_deref(),
            id,
            sender,
            cancel,
        );
        tokio::pin!(future);
        let id = loop {
            tokio::select! {
                biased;
                ()=cancel.cancelled()=>return Err(AdapterError::Cancelled.to_string()),
                result=&mut future=>break result.map_err(|e|e.to_string())?,
                Some(prompt)=rx.recv()=>self.prompt(ticket,prompt),
            }
        };
        saved(service, id).map_err(|e| e.to_string())
    }
}

impl UsageConnections for JackinBackend {
    fn has_connected_accounts(&self) -> bool {
        self.usage_service()
            .ok()
            .and_then(|service| {
                AccountRegistry::open_read_only(service.database())
                    .ok()
                    .flatten()
            })
            .is_some_and(|db| db.list().is_ok_and(|accounts| !accounts.is_empty()))
    }

    fn accounts_opened(&self) {
        self.usage_schedule.next_hub.store(0, Ordering::Release);
        self.usage_schedule.next.store(0, Ordering::Release);
        self.schedule_usage(self.inputs.host.clock.now().0);
    }

    fn request(&self, ticket: Ticket, action: UsageAction) {
        let service = match self.usage_service() {
            Ok(service) => service,
            Err(e) => {
                self.bridge.send(failure(ticket, e));
                return;
            }
        };
        match action {
            UsageAction::Board { priority, edit } => {
                let state = Arc::clone(&self.dashboard);
                let cancel = self.inputs.cancel.child_token();
                self.bridge.spawn(
                    failure(ticket, AdapterError::Dashboard.to_string()),
                    async move {
                        if let jackin_console::usage_connections::BoardEdit::Refresh(provider) =
                            &edit
                            && let Err(error) = refresh_accounts(
                                &service,
                                provider.is_none(),
                                true,
                                provider.as_deref(),
                                &cancel,
                            )
                            .await
                        {
                            return failure(ticket, error);
                        }
                        if let jackin_console::usage_connections::BoardEdit::Proxy(url) = &edit
                            && let Err(e) = service
                                .configure_proxy(
                                    url.as_ref()
                                        .map(|k| Secret::new(k.expose().to_owned()))
                                        .as_ref(),
                                )
                                .await
                        {
                            return failure(ticket, e.to_string());
                        }
                        let now = service.now().0;
                        let output = tokio::task::spawn_blocking(move || {
                            board::load(&service, priority, edit, &mut locked(&state), now)
                        })
                        .await;
                        match output {
                            Ok(Ok(board)) => message(ticket, UsageUpdate::Board(Box::new(board))),
                            Ok(Err(e)) => failure(ticket, e),
                            Err(_) => failure(ticket, AdapterError::Dashboard.to_string()),
                        }
                    },
                );
            }
            UsageAction::Begin(choice) => {
                let cancel = self.inputs.cancel.child_token();
                *locked(&self.usage_attempt) = Some(Attempt {
                    ticket,
                    cancel: cancel.clone(),
                    reply: None,
                    url: None,
                });
                let backend = self.clone();
                self.bridge.spawn(
                    failure(ticket, AdapterError::SignIn.to_string()),
                    async move {
                        let result = backend.run_login(&service, ticket, choice, &cancel).await;
                        let mut attempt = locked(&backend.usage_attempt);
                        if attempt.as_ref().is_some_and(|a| a.ticket == ticket) {
                            *attempt = None;
                        }
                        match result {
                            Ok(update) => message(ticket, update),
                            Err(e) => failure(ticket, e),
                        }
                    },
                );
            }
            UsageAction::Submit(_) | UsageAction::Skip => {
                let mut active = locked(&self.usage_attempt);
                if let Some(reply) = active
                    .as_mut()
                    .filter(|a| a.ticket == ticket)
                    .and_then(|a| a.reply.take())
                {
                    let value = if let UsageAction::Submit(key) = action {
                        key.expose().to_owned()
                    } else {
                        String::new()
                    };
                    let _sent = reply.send(Secret::new(value));
                }
            }
            UsageAction::Cancel => {
                let mut active = locked(&self.usage_attempt);
                if active.as_ref().is_some_and(|a| a.ticket == ticket) {
                    *active = None;
                }
            }
            UsageAction::OpenBrowser => {
                let url = locked(&self.usage_attempt)
                    .as_ref()
                    .filter(|a| a.ticket == ticket)
                    .and_then(|a| a.url.clone());
                if let Some(url) = url {
                    let bridge = self.bridge.clone();
                    self.bridge.detach(open_sign_in(bridge, ticket, url), || {});
                }
            }
            action => {
                let cancel = self.inputs.cancel.child_token();
                *locked(&self.usage_attempt) = Some(Attempt {
                    ticket,
                    cancel: cancel.clone(),
                    reply: None,
                    url: None,
                });

                self.bridge.spawn(
                    failure(ticket, AdapterError::Operation.to_string()),
                    async move {
                        let result = account_action(&service, action, &cancel).await;
                        match result {
                            Ok(update) => message(ticket, update),
                            Err(e) => failure(ticket, e),
                        }
                    },
                );
            }
        }
    }
}

async fn open_sign_in(bridge: crate::bridge::Bridge, ticket: Ticket, url: Url) {
    if open_browser(&url).await.is_err() {
        bridge.send(message(ticket, UsageUpdate::OpenFailed));
    }
}

async fn refresh_accounts(
    service: &OwnedLogin,
    check_hubs: bool,
    manual: bool,
    provider: Option<&str>,
    cancel: &CancellationToken,
) -> Result<(), ConnectionError> {
    let Some(db) = AccountRegistry::open_read_only(service.database())? else {
        return Ok(());
    };
    if check_hubs {
        for hub in db.hubs()? {
            let _settled = service.sync_hub(hub.id, cancel).await;
        }
    }
    let accounts = db
        .list()?
        .into_iter()
        .filter(|a| provider.is_none_or(|p| a.provider.as_str() == p));
    stream::iter(accounts)
        .for_each_concurrent(4, |account| async move {
            let _settled = service.refresh_if_due(&account, manual, cancel).await;
        })
        .await;
    Ok(())
}

pub async fn reload_usage_board(
    service: &OwnedLogin,
) -> Result<Vec<(Option<String>, String, String, Option<String>)>, ConnectionError> {
    refresh_accounts(service, true, true, None, &CancellationToken::new()).await?;
    Ok(read_usage_board(service, false)?
        .rows
        .iter()
        .map(|row| {
            (
                row.account.clone(),
                row.title.clone(),
                row.exact_value.clone(),
                row.reset.clone(),
            )
        })
        .collect())
}

pub fn read_usage_board(
    service: &OwnedLogin,
    priority: bool,
) -> Result<jackin_console::usage_connections::UsageBoard, ConnectionError> {
    board::load(
        service,
        priority,
        jackin_console::usage_connections::BoardEdit::Read,
        &mut jackin_usage::dashboard::DashboardMemory::default(),
        service.now().0,
    )
}

fn hubs(service: &OwnedLogin) -> Result<UsageUpdate, ConnectionError> {
    let Some(db) = AccountRegistry::open_read_only(service.database())? else {
        return Ok(UsageUpdate::Hubs(Vec::new()));
    };
    Ok(UsageUpdate::Hubs(
        db.hubs()?
            .into_iter()
            .map(|h| jackin_console::usage_connections::HubRow {
                id: h.id.to_string(),
                address: h.address,
                refused: h.refused,
            })
            .collect(),
    ))
}

async fn account_action(
    service: &OwnedLogin,
    action: UsageAction,
    cancel: &CancellationToken,
) -> Result<UsageUpdate, String> {
    match action {
        UsageAction::Hubs => hubs(service).map_err(|e| e.to_string()),
        UsageAction::ConnectHub {
            address,
            key,
            replace,
        } => {
            let id = replace
                .as_deref()
                .map(Uuid::parse_str)
                .transpose()
                .map_err(|_| AdapterError::HubId.to_string())?;
            service
                .connect_hub(&address, &Secret::new(key.expose().to_owned()), id, cancel)
                .await
                .map_err(|e| e.to_string())?;
            hubs(service).map_err(|e| e.to_string())
        }
        UsageAction::SyncHub(id) => {
            let id = Uuid::parse_str(&id).map_err(|_| AdapterError::HubId.to_string())?;
            service
                .sync_hub(id, cancel)
                .await
                .map_err(|e| e.to_string())?;
            hubs(service).map_err(|e| e.to_string())
        }
        UsageAction::RemoveHub(id) => {
            let id = Uuid::parse_str(&id).map_err(|_| AdapterError::HubId.to_string())?;
            service.remove_hub(id).await.map_err(|e| e.to_string())?;
            hubs(service).map_err(|e| e.to_string())
        }
        UsageAction::List => entries(service)
            .map(UsageUpdate::Accounts)
            .map_err(|e| e.to_string()),
        UsageAction::Redeem {
            account,
            index,
            identity,
        } => {
            let id = Uuid::parse_str(&account).map_err(|_| AdapterError::AccountId.to_string())?;
            let (row, _, payload) = service
                .with_registry(move |db| db.snapshots())
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|(a, _, _)| a.id == id)
                .ok_or(AdapterError::Removed.to_string())?;
            let payload = payload.ok_or(AdapterError::NoResets.to_string())?;
            if payload
                .get(display::RESETS)
                .and_then(|resets| resets.get(index))
                .map(Value::to_string)
                .as_deref()
                != Some(identity.as_str())
            {
                return Err(AdapterError::InventoryChanged.to_string());
            }
            let selection = jackin_usage::resets::ResetSelection::from_snapshot(
                id,
                row.provider,
                &payload,
                index,
                service.now(),
            )
            .map_err(|e| e.to_string())?;
            let outcome = service
                .redeem_reset(selection, cancel)
                .await
                .map_err(|e| e.to_string())?;
            let mut row = entries(service)
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|r| r.id == account)
                .ok_or(AdapterError::Removed.to_string())?;
            row.details.insert(0, outcome.to_string());
            Ok(UsageUpdate::Saved(row))
        }
        UsageAction::Remove(id) => {
            let id = Uuid::parse_str(&id).map_err(|_| AdapterError::AccountId.to_string())?;
            service
                .with_registry(move |mut db| db.remove(id))
                .await
                .map_err(|e| e.to_string())?;
            Ok(UsageUpdate::Accounts(
                entries(service).map_err(|e| e.to_string())?,
            ))
        }
        UsageAction::Rename { id, name } => {
            let id = Uuid::parse_str(&id).map_err(|_| AdapterError::AccountId.to_string())?;
            let suffix = format!(" #{}", jackin_usage::view::short_id(id));
            let name = match name.strip_suffix(&suffix) {
                Some(rest) => rest.to_owned(),
                None => name,
            };
            service
                .with_registry(move |db| db.rename(id, &name))
                .await
                .map_err(|e| e.to_string())?;
            saved(service, id).map_err(|e| e.to_string())
        }
        UsageAction::Refresh(id) => {
            let id = Uuid::parse_str(&id).map_err(|_| AdapterError::AccountId.to_string())?;
            let account: ConnectedAccount = service
                .with_registry(move |db| db.list())
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|a| a.id == id)
                .ok_or(AdapterError::Removed.to_string())?;
            service
                .refresh(&account, cancel)
                .await
                .map_err(|e| e.to_string())?;
            saved(service, id).map_err(|e| e.to_string())
        }
        _ => Err(AdapterError::Unsupported.to_string()),
    }
}

#[cfg(test)]
mod tui_refresh {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use jackin_console::usage_connections::BoardEdit;
    use jackin_core::{Clock, Provider, SecretBytes};
    use jackin_testkit::{FakeClock, Method, Route, UsageServer};
    use jackin_usage::accounts::{CandidateIdentity, CredentialVault, UsageProvider};
    use jackin_usage::dashboard::DashboardMemory;
    use serde_json::json;

    use super::*;

    #[derive(Debug, Default)]
    struct Memory {
        values: Mutex<HashMap<Uuid, Vec<u8>>>,
    }

    impl CredentialVault for Memory {
        fn store(&self, id: Uuid, secret: &SecretBytes) -> Result<(), ConnectionError> {
            self.values
                .lock()
                .map_err(|_| ConnectionError::Vault)?
                .insert(id, secret.expose().to_vec());
            Ok(())
        }

        fn load(&self, id: Uuid) -> Result<SecretBytes, ConnectionError> {
            self.values
                .lock()
                .map_err(|_| ConnectionError::Vault)?
                .get(&id)
                .cloned()
                .map(SecretBytes::new)
                .ok_or(ConnectionError::MissingCredential)
        }

        fn delete(&self, id: Uuid) -> Result<(), ConnectionError> {
            self.values
                .lock()
                .map_err(|_| ConnectionError::Vault)?
                .remove(&id);
            Ok(())
        }
    }

    fn reading() -> String {
        r#"{"groups":[{"displayName":"Gemini","buckets":[
            {"bucketId":"gemini-5h","remainingFraction":0.99965,"resetTime":"2027-03-02T14:00:00Z"},
            {"bucketId":"gemini-weekly","remainingFraction":0.9742,"resetTime":"2027-03-08T09:00:00Z"},
            {"bucketId":"3p-5h","remainingFraction":0.5,"resetTime":"2027-03-02T15:00:00Z"},
            {"bucketId":"3p-weekly","remainingFraction":0.4,"resetTime":"2027-03-08T09:00:00Z"}
        ]},{"displayName":"Gemini 4 Argon","buckets":[
            {"bucketId":"gemini-argon-wtu-weekly","window":"weekly","remainingFraction":0.8,"resetTime":"2027-03-09T00:00:00Z"}
        ]}]}"#
        .to_owned()
    }

    #[tokio::test]
    async fn board_reload_shows_the_same_snapshot_as_cli_refresh() {
        let server = UsageServer::start().await.expect("fixture server");
        let load = r#"{"paidTier":{"id":"g1-ultra-tier","name":"ignored"},"cloudaicompanionProject":"proj-1"}"#;
        server.serve(
            Provider::Antigravity,
            Method::Post,
            "/production/v1internal:loadCodeAssist",
            Route::json(200, load),
        );
        server.serve(
            Provider::Antigravity,
            Method::Post,
            "/daily/v1internal:retrieveUserQuotaSummary",
            Route::json(200, &reading()).requires_json_body(json!({"project": "proj-1"})),
        );
        let world = jackin_testkit::World::builder().build().expect("world");
        let path = world
            .config
            .path()
            .as_path()
            .parent()
            .expect("config parent")
            .join("usage/accounts.sqlite3");
        let clock = Arc::new(FakeClock::new());
        let vault: Arc<dyn CredentialVault> = Arc::new(Memory::default());
        let mut db = AccountRegistry::with_vault(&path, Arc::clone(&vault)).expect("registry");
        let now = clock.now().0;
        let id = db
            .add_with_snapshot(
                UsageProvider::Antigravity,
                "Personal",
                &CandidateIdentity::OAuth(Some("ultra@example.com".into())),
                &SecretBytes::new(
                    serde_json::to_vec(&json!({
                        "product": "antigravity",
                        "client_id": "jackin-fixture-desktop.apps.googleusercontent.com",
                        "client_secret": "fixture-not-a-secret",
                        "access": "old-access",
                        "refresh": "refresh-1",
                        "expires_at": now + 10_000
                    }))
                    .expect("grant"),
                ),
                now - 60,
                &json!({"plan":"stale","resources":{"gemini-5h":{"kind":"quota","unit":"percent","remaining_fraction":0.1,"window_seconds":18000}}}),
            )
            .expect("seed");
        drop(db);
        let clock_port = Arc::<FakeClock>::clone(&clock);
        let clock_port: Arc<dyn Clock> = clock_port;
        let service = OwnedLogin::new(path.clone(), server.endpoints(), clock_port)
            .with_vault(Arc::clone(&vault));
        refresh_accounts(&service, true, true, None, &CancellationToken::new())
            .await
            .expect("tui refresh");
        let registry = AccountRegistry::with_vault(&path, vault).expect("reopen");
        let (_row, _at, snapshot) = registry
            .snapshots()
            .expect("snapshots")
            .into_iter()
            .next()
            .expect("snapshot");
        let snapshot = snapshot.expect("snapshot body");
        assert_eq!(snapshot["plan"], "Google AI Ultra");
        let resources = snapshot["resources"].as_object().expect("resources");
        let expect = [
            ("gemini-5h", 0.99965, 1_803_996_000_i64),
            ("gemini-weekly", 0.9742, 1_804_496_400_i64),
            ("gemini-argon-wtu-weekly", 0.8, 1_804_550_400_i64),
            ("3p-5h", 0.5, 1_803_999_600_i64),
            ("3p-weekly", 0.4, 1_804_496_400_i64),
        ];
        for (key, fraction, reset) in expect {
            assert_eq!(resources[key]["remaining_fraction"], fraction, "{key}");
            assert_eq!(resources[key]["resets_at"], reset, "{key}");
        }
        let mut memory = DashboardMemory::default();
        let board = board::load(&service, false, BoardEdit::Read, &mut memory, clock.now().0)
            .expect("board");
        let rendered: Vec<_> = board
            .rows
            .iter()
            .map(|row| {
                format!(
                    "{} {} {} {}",
                    row.account.as_deref().unwrap_or("-"),
                    row.title,
                    row.exact_value,
                    row.reset.as_deref().unwrap_or("-")
                )
            })
            .collect();
        assert!(
            rendered.iter().any(|line| line.contains(&id.to_string())),
            "{rendered:?}"
        );
        assert!(
            rendered.iter().any(|line| line.contains("Gemini weekly")),
            "{rendered:?}"
        );
        assert!(
            rendered.iter().any(|line| line.contains("Argon weekly")),
            "{rendered:?}"
        );
        assert!(
            rendered.iter().any(|line| line.contains("Claude")),
            "{rendered:?}"
        );
    }
}
