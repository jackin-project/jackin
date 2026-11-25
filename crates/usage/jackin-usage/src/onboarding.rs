// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod antigravity;
mod antigravity_direct;
pub use antigravity::receive_antigravity_statusline;
mod google_registration;
pub use google_registration::GoogleRegistrationFile;
mod hub;
mod kimi_connection;
mod minimax_connection;
mod redemption;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jackin_core::{Clock, Provider, Secret, SecretBytes, Timestamp};
use jackin_usage_provider_core::{Http, ProviderError};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

use crate::ProviderEndpoints;
use crate::accounts::{
    AccountRegistry, CandidateIdentity, ConnectedAccount, ConnectionError, CredentialVersion,
    FailureKind, UsageProvider,
};
use crate::browser::{BrowserError, BrowserLogin, BrowserProvider};
use crate::connected::KeyConnection;
use crate::device_login::{ChatGptLogin, DeviceLogin, DeviceLoginError, DeviceProvider};
use crate::grants::{self, GrantError, SignedIn, Tokens};

#[derive(Clone, Debug)]
pub struct OwnedLogin {
    path: PathBuf,
    endpoints: ProviderEndpoints,
    clock: Arc<dyn Clock>,
    google_registration: Option<GoogleRegistrationFile>,
    vault: Option<Arc<dyn crate::accounts::CredentialVault>>,
    store_wait: std::time::Duration,
}

#[derive(Debug, Error)]
pub enum LoginError {
    #[error("the provider asked jackin to wait until Unix time {0}")]
    Wait(i64),
    #[error("refresh timed out after 120 seconds")]
    Timeout,
    #[error(
        "the OS credential store did not answer in time; approve the Keychain access prompt or unlock the credential store, then retry"
    )]
    CredentialStoreTimeout,
    #[error(transparent)]
    Reset(#[from] crate::resets::ResetError),
    #[error(
        "JACKIN_GOOGLE_OAUTH_CLIENT_FILE must name a readable Google Desktop app JSON file containing installed.client_id"
    )]
    GoogleRegistration,
    #[error("sign-in cancelled")]
    Cancelled,
    #[error(transparent)]
    Store(#[from] ConnectionError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Browser(#[from] BrowserError),
    #[error(transparent)]
    Device(#[from] DeviceLoginError),
    #[error(transparent)]
    Grant(#[from] GrantError),
    #[error("could not open the default browser; use the displayed URL")]
    OpenBrowser,
    #[error("the stored credential cannot be decoded; sign in again")]
    Encoding,
    #[error(
        "this Antigravity account is updated by the Antigravity status-line hook, so refresh cannot read it from Google; sign in directly to refresh it"
    )]
    NotDirect,
    #[error("usage account operation was interrupted")]
    Worker,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageAuth {
    Key,
    Device,
}

impl std::str::FromStr for UsageAuth {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "key" => Ok(Self::Key),
            "device" => Ok(Self::Device),
            _ => Err("choose key or device"),
        }
    }
}

pub enum LoginPrompt {
    Field {
        label: &'static str,
        optional: bool,
        masked: bool,
        reply: oneshot::Sender<Secret>,
    },
    Callback {
        url: Url,
    },
    Key {
        url: &'static str,
        reply: oneshot::Sender<Secret>,
    },
    Browser {
        url: Url,
        paste: oneshot::Sender<Secret>,
    },
    Device {
        url: Url,
        code: Secret,
    },
    AntigravityStatusLine {
        command: String,
    },
    BrowserUnavailable,
}

impl fmt::Debug for LoginPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Field { .. } => "Field",
            Self::Callback { .. } => "Callback",
            Self::Key { .. } => "Key",
            Self::Browser { .. } => "Browser",
            Self::Device { .. } => "Device",
            Self::AntigravityStatusLine { .. } => "AntigravityStatusLine",
            Self::BrowserUnavailable => "BrowserUnavailable",
        })
    }
}

pub async fn open_browser(url: &Url) -> Result<(), LoginError> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(LoginError::OpenBrowser);
    }
    let url = url.clone();
    tokio::task::spawn_blocking(move || {
        webbrowser::open(url.as_str()).map_err(|_| LoginError::OpenBrowser)
    })
    .await
    .map_err(|_| LoginError::Worker)?
}

pub(super) async fn field(
    prompts: &mpsc::Sender<LoginPrompt>,
    label: &'static str,
    optional: bool,
    masked: bool,
) -> Result<Secret, LoginError> {
    let (reply, input) = oneshot::channel();
    prompts
        .send(LoginPrompt::Field {
            label,
            optional,
            masked,
            reply,
        })
        .await
        .map_err(|_| LoginError::Cancelled)?;
    input.await.map_err(|_| LoginError::Cancelled)
}

pub(super) async fn text_field(
    prompts: &mpsc::Sender<LoginPrompt>,
    label: &'static str,
    optional: bool,
) -> Result<String, LoginError> {
    Ok(field(prompts, label, optional, false)
        .await?
        .expose()
        .trim()
        .to_owned())
}

fn core_provider(provider: UsageProvider) -> Provider {
    use UsageProvider as U;
    match provider {
        U::Claude => Provider::Claude,
        U::ChatGpt => Provider::Codex,
        U::Grok => Provider::Grok,
        U::Muse => Provider::Muse,
        U::Kimi => Provider::Kimi,
        U::MiniMax => Provider::Minimax,
        U::Mimo => Provider::Mimo,
        U::Antigravity => Provider::Antigravity,
        U::Zai => Provider::Zai,
        U::OpenCode => Provider::Opencode,
        U::OpenRouter => Provider::Openrouter,
    }
}

impl OwnedLogin {
    pub fn new(path: PathBuf, endpoints: ProviderEndpoints, clock: Arc<dyn Clock>) -> Self {
        Self {
            path,
            endpoints,
            clock,
            vault: None,
            google_registration: None,
            store_wait: std::time::Duration::from_secs(30),
        }
    }

    #[must_use]
    pub fn with_store_wait(mut self, wait: std::time::Duration) -> Self {
        self.store_wait = wait;
        self
    }
    #[must_use]
    pub fn with_google_registration(
        mut self,
        registration: Option<GoogleRegistrationFile>,
    ) -> Self {
        self.google_registration = registration;
        self
    }

    async fn account_http_budget(
        &self,
        account: &ConnectedAccount,
        http: Http,
    ) -> Result<Http, LoginError> {
        if account.provider != UsageProvider::Claude {
            return Ok(http);
        }
        let budget = crate::accounts::claude_budget::Budget::saved(
            self.with_registry(Ok).await?,
            Arc::clone(&self.clock),
            account.id,
        )?;
        Ok(http.with_claude_budget(Arc::new(budget)))
    }

    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    pub fn utc_offset_secs(&self) -> i32 {
        self.clock.utc_offset_secs()
    }

    #[must_use]
    pub fn with_vault(mut self, vault: Arc<dyn crate::accounts::CredentialVault>) -> Self {
        self.vault = Some(vault);
        self
    }

    pub(crate) fn open_registry(&self) -> Result<AccountRegistry, ConnectionError> {
        match &self.vault {
            Some(vault) => AccountRegistry::with_vault(&self.path, Arc::clone(vault)),
            None => AccountRegistry::open(&self.path),
        }
        .map(|registry| registry.with_lock_wait(self.store_wait))
    }

    pub async fn with_registry<T: Send + 'static>(
        &self,
        operation: impl FnOnce(AccountRegistry) -> Result<T, ConnectionError> + Send + 'static,
    ) -> Result<T, LoginError> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || operation(service.open_registry()?))
            .await
            .map_err(|_| LoginError::Worker)?
            .map_err(Into::into)
    }

    async fn read_credential(
        &self,
        id: Uuid,
    ) -> Result<(CredentialVersion, SecretBytes), LoginError> {
        tokio::time::timeout(
            self.store_wait,
            self.with_registry(move |db| db.credential(id)),
        )
        .await
        .map_err(|_| LoginError::CredentialStoreTimeout)?
    }

    async fn rotate(
        &self,
        id: Uuid,
        expected: CredentialVersion,
        material: SecretBytes,
        lease: &std::fs::File,
    ) -> Result<CredentialVersion, LoginError> {
        let guard = lease.try_clone().map_err(ConnectionError::from)?;
        self.with_registry(move |mut db| {
            let _lease = guard;
            db.rotate(id, expected, &material)
        })
        .await
    }

    async fn account_lease(
        &self,
        id: Uuid,
        cancel: &CancellationToken,
    ) -> Result<std::fs::File, LoginError> {
        let file = self.with_registry(move |db| db.renewal_file(id)).await?;
        loop {
            if cancel.is_cancelled() {
                return Err(LoginError::Cancelled);
            }
            match file.try_lock() {
                Ok(()) => return Ok(file),
                Err(std::fs::TryLockError::Error(error)) => {
                    return Err(ConnectionError::from(error).into());
                }
                Err(std::fs::TryLockError::WouldBlock) => tokio::select! {
                    () = cancel.cancelled() => return Err(LoginError::Cancelled),
                    () = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
                },
            }
        }
    }

    pub fn database(&self) -> &Path {
        &self.path
    }

    async fn base_http(&self) -> Result<Http, LoginError> {
        let http = Http::new(self.endpoints.timeout());
        let Some(proxy) = self.with_registry(|db| db.proxy()).await? else {
            return Ok(http);
        };
        Ok(http
            .with_proxy(std::str::from_utf8(proxy.expose()).map_err(|_| LoginError::Encoding)?)?)
    }

    async fn http(&self, provider: UsageProvider) -> Result<Http, LoginError> {
        let http = self.base_http().await?;
        Ok(match self.endpoints.base(core_provider(provider)) {
            Some(base) => http.with_base(base)?,
            None => http,
        })
    }

    pub async fn configure_proxy(&self, proxy: Option<&Secret>) -> Result<(), LoginError> {
        if let Some(url) = proxy {
            Http::new(self.endpoints.timeout()).with_proxy(url.expose())?;
        }
        let encoded = proxy.map(|p| SecretBytes::new(p.expose().as_bytes().to_vec()));
        self.with_registry(move |mut db| db.set_proxy(encoded.as_ref()))
            .await
    }

    pub async fn connect(
        &self,
        provider: UsageProvider,
        region: Option<&str>,
        replace: Option<Uuid>,
        prompts: mpsc::Sender<LoginPrompt>,
        cancel: &CancellationToken,
    ) -> Result<Uuid, LoginError> {
        self.connect_with_auth(provider, None, region, replace, prompts, cancel)
            .await
    }

    pub async fn connect_with_auth(
        &self,
        provider: UsageProvider,
        auth: Option<UsageAuth>,
        region: Option<&str>,
        replace: Option<Uuid>,
        prompts: mpsc::Sender<LoginPrompt>,
        cancel: &CancellationToken,
    ) -> Result<Uuid, LoginError> {
        if auth.is_some() && !matches!(provider, UsageProvider::Kimi | UsageProvider::MiniMax) {
            return Err(ProviderError::Unsupported(
                "auth method selection applies to Kimi or MiniMax",
            )
            .into());
        }
        let lease = match replace {
            Some(id) => Some(self.account_lease(id, cancel).await?),
            None => None,
        };
        let mut db = self.with_registry(Ok).await?;
        let existing = replace
            .map(|id| db.version(id).map(|v| (id, v)))
            .transpose()?;
        if let Some(id) = replace
            && db.hub_link(id)?.is_some()
        {
            return Err(ConnectionError::HubManaged.into());
        }
        let candidate = tokio::select! {
            biased;
            ()=cancel.cancelled()=>return Err(LoginError::Cancelled),
            validated=self.candidate(provider,auth,region,replace,prompts,cancel)=>validated?,
        };
        if cancel.is_cancelled() {
            return Err(LoginError::Cancelled);
        }
        let fetched = self.clock.now().0;
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            persist_candidate(&mut db, provider, candidate, existing, fetched, &cancel)
        })
        .await
        .map_err(|_| LoginError::Worker)?
    }

    async fn previous_material(
        &self,
        replace: Option<Uuid>,
    ) -> Result<Option<(CredentialVersion, SecretBytes)>, LoginError> {
        let Some(id) = replace else { return Ok(None) };
        match self.read_credential(id).await {
            Ok(value) => Ok(Some(value)),
            Err(LoginError::Store(
                ConnectionError::MissingCredential | ConnectionError::CredentialKind,
            )) => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn key_candidate(
        &self,
        provider: UsageProvider,
        region: Option<&str>,
        prompts: mpsc::Sender<LoginPrompt>,
    ) -> Result<Candidate, LoginError> {
        let method = KeyConnection::for_provider(core_provider(provider), region)?;
        let (reply, read) = oneshot::channel();
        prompts
            .send(LoginPrompt::Key {
                url: method.key_page(),
                reply,
            })
            .await
            .map_err(|_| LoginError::Cancelled)?;
        let key = read.await.map_err(|_| LoginError::Cancelled)?;
        let key = Secret::new(key.expose().trim().to_owned());
        let snapshot = method
            .read_using_http(
                self.http(provider).await?,
                &key,
                self.clock.now(),
                None,
                false,
            )
            .await?;
        Ok(Candidate {
            label: snapshot["label"]
                .as_str()
                .unwrap_or_else(|| method.default_label())
                .into(),
            identity: CandidateIdentity::api_key(&key),
            material: method.encode(&key)?,
            snapshot: Ok(snapshot),
        })
    }

    async fn candidate(
        &self,
        provider: UsageProvider,
        auth: Option<UsageAuth>,
        region: Option<&str>,
        replace: Option<Uuid>,
        prompts: mpsc::Sender<LoginPrompt>,
        cancel: &CancellationToken,
    ) -> Result<Candidate, LoginError> {
        use UsageProvider as U;
        let saved = if let Some(id) =
            replace.filter(|_| matches!(provider, U::Kimi | U::MiniMax | U::Mimo))
        {
            self.with_registry(|db| db.snapshots())
                .await?
                .into_iter()
                .find(|(a, _, _)| a.id == id)
                .and_then(|(_, _, v)| v)
        } else {
            None
        };
        let region = region.or_else(|| saved.as_ref()?.get("region")?.as_str());
        let region = if region.is_none() && provider == U::Mimo && replace.is_some() {
            self.previous_material(replace)
                .await?
                .and_then(|(_, bytes)| KeyConnection::decode(Provider::Mimo, &bytes).ok())
                .and_then(|(method, _)| method.region())
        } else {
            region
        };
        let auth = auth.or_else(|| saved.as_ref()?.get("authMethod")?.as_str()?.parse().ok());
        if provider == U::MiniMax && auth == Some(UsageAuth::Device) {
            let old = self
                .previous_material(replace)
                .await?
                .as_ref()
                .and_then(|(_, bytes)| minimax_connection::Credential::decode(bytes));
            return minimax_connection::connect(
                &self.http(provider).await?,
                self.clock.now(),
                region,
                old,
                prompts,
                cancel,
            )
            .await;
        }
        if provider == U::Kimi && auth == Some(UsageAuth::Device) {
            let old = self
                .previous_material(replace)
                .await?
                .as_ref()
                .and_then(|(_, bytes)| kimi_connection::Credential::decode(bytes));
            return kimi_connection::connect(
                &self.http(provider).await?,
                self.clock.as_ref(),
                region,
                old,
                prompts,
                cancel,
            )
            .await;
        }
        if matches!(
            provider,
            U::Zai | U::OpenRouter | U::Mimo | U::MiniMax | U::Kimi
        ) {
            return self.key_candidate(provider, region, prompts).await;
        }
        let http = self.http(provider).await?;
        if provider == U::Antigravity {
            let current = self.previous_material(replace).await?;
            return antigravity_direct::connect(
                &http,
                self.clock.now(),
                current.as_ref().map(|(_, bytes)| bytes),
                self.google_registration.as_ref(),
                prompts,
                cancel,
            )
            .await;
        }
        let login = match provider {
            U::Claude => ChatGptLogin::Browser(BrowserLogin::start(BrowserProvider::Claude).await?),
            U::ChatGpt => ChatGptLogin::start(&http, cancel).await?,
            U::Grok | U::Muse | U::OpenCode => ChatGptLogin::Device(
                DeviceLogin::start(device_provider(provider)?, &http, cancel).await?,
            ),
            _ => {
                return Err(
                    ProviderError::Unsupported("this usage provider has no sign-in flow").into(),
                );
            }
        };
        let signed = match login {
            ChatGptLogin::Browser(browser) => {
                let url = browser.authorization_url().clone();
                let (paste, input) = oneshot::channel();
                prompts
                    .send(LoginPrompt::Browser {
                        url: url.clone(),
                        paste,
                    })
                    .await
                    .map_err(|_| LoginError::Cancelled)?;
                if open_browser(&url).await.is_err() {
                    prompts
                        .send(LoginPrompt::BrowserUnavailable)
                        .await
                        .map_err(|_| LoginError::Cancelled)?;
                }
                let grant = browser.wait(input, cancel).await?;
                grants::exchange(
                    &http,
                    if provider == U::Claude {
                        BrowserProvider::Claude
                    } else {
                        BrowserProvider::ChatGpt
                    },
                    grant,
                    self.clock.now(),
                )
                .await?
            }
            ChatGptLogin::Device(device) => {
                prompts
                    .send(LoginPrompt::Device {
                        url: device.verification_url().clone(),
                        code: Secret::new(device.user_code().expose().to_owned()),
                    })
                    .await
                    .map_err(|_| LoginError::Cancelled)?;
                device.finish(&http, self.clock.as_ref(), cancel).await?
            }
        };
        self.subscription_candidate(provider, http, signed).await
    }

    async fn subscription_candidate(
        &self,
        provider: UsageProvider,
        http: Http,
        signed: SignedIn,
    ) -> Result<Candidate, LoginError> {
        let SignedIn {
            label,
            identity,
            organization,
            tokens,
        } = signed;
        let http = if provider == UsageProvider::Claude
            && let Some(scope) = identity.as_deref()
        {
            http.with_claude_budget(Arc::new(crate::accounts::claude_budget::Budget::candidate(
                self.with_registry(Ok).await?,
                Arc::clone(&self.clock),
                scope,
            )))
        } else {
            http
        };
        let snapshot = crate::quota::read(
            provider,
            &http,
            &tokens.access,
            organization.as_deref(),
            self.clock.now(),
            None,
        )
        .await;
        let snapshot = snapshot.map_err(LoginError::from);
        Ok(Candidate {
            label,
            identity: CandidateIdentity::OAuth(identity),
            material: pack(&tokens, organization.as_deref())?,
            snapshot,
        })
    }

    pub async fn refresh(
        &self,
        account: &ConnectedAccount,
        cancel: &CancellationToken,
    ) -> Result<(), LoginError> {
        self.refresh_if_due(account, true, cancel).await
    }

    pub async fn refresh_if_due(
        &self,
        account: &ConnectedAccount,
        manual: bool,
        cancel: &CancellationToken,
    ) -> Result<(), LoginError> {
        self.refresh_for(
            account,
            if manual {
                RefreshTrigger::Manual
            } else {
                RefreshTrigger::Scheduled
            },
            cancel,
        )
        .await
    }

    async fn refresh_for(
        &self,
        account: &ConnectedAccount,
        trigger: RefreshTrigger,
        cancel: &CancellationToken,
    ) -> Result<(), LoginError> {
        if account.provider == UsageProvider::Antigravity
            && matches!(
                self.read_credential(account.id).await,
                Err(LoginError::Store(ConnectionError::CredentialKind))
            )
        {
            return Err(LoginError::NotDirect);
        }
        let manual = trigger != RefreshTrigger::Scheduled;
        let id = account.id;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
        let lease = tokio::time::timeout_at(deadline, self.account_lease(id, cancel))
            .await
            .map_err(|_| LoginError::Timeout)??;
        let mut db = self.with_registry(Ok).await?;
        let now = self.clock.now().0;
        if let Some(failure) = db.failure(id)?
            && failure.holds(now, manual)
        {
            return if manual {
                Err(LoginError::Wait(failure.retry_at))
            } else {
                Ok(())
            };
        }
        if !manual
            && db
                .snapshots()?
                .iter()
                .any(|(row, at, _)| row.id == id && at.is_some_and(|t| now < t.saturating_add(300)))
        {
            return Ok(());
        }
        let mut version = db.version(id)?;
        let attempt = tokio::time::timeout_at(deadline, async {
            let mut delays = [2, 4, 8, 16, 32].into_iter();
            loop {
                let result = self
                    .fetch_into(&mut db, account, &mut version, trigger, &lease)
                    .await;
                if matches!(
                    result,
                    Err(LoginError::Provider(ProviderError::Network(
                        "no network path is available"
                    )))
                ) && let Some(seconds) = delays.next()
                {
                    self.clock
                        .sleep(std::time::Duration::from_secs(seconds))
                        .await;
                } else {
                    return result;
                }
            }
        });
        let result = tokio::select! {
            biased;
            ()=cancel.cancelled()=>return Err(LoginError::Cancelled),
            result=attempt=>result.unwrap_or(Err(LoginError::Timeout)),
        };
        if let Err(error) = &result
            && let Some((kind, delay)) = error.failure_class()
        {
            db.record_failure(
                id,
                version,
                self.clock.now().0,
                kind,
                delay,
                &error.to_string(),
            )?;
        }
        result
    }

    async fn fetch_into(
        &self,
        db: &mut AccountRegistry,
        account: &ConnectedAccount,
        version: &mut CredentialVersion,
        trigger: RefreshTrigger,
        lease: &std::fs::File,
    ) -> Result<(), LoginError> {
        let manual = trigger != RefreshTrigger::Scheduled;
        let id = account.id;
        if let Some(link) = db.hub_link(id)? {
            let payload = self.mirror_usage(db, account, link, trigger).await?;
            return Ok(db.put_snapshot(id, *version, self.clock.now().0, &payload)?);
        }
        let (read_version, bytes) = self.read_credential(id).await?;
        if read_version != *version {
            return Err(ConnectionError::Changed.into());
        }
        use UsageProvider as U;
        let payload = if account.provider == U::Antigravity {
            let http = self.http(account.provider).await?;
            let mut credential = antigravity_direct::Credential::decode(&bytes)?;
            let previous = trigger.previous(db, id)?;
            let now = self.clock.now();
            if let Some(material) = credential.renew_if_due(&http, now).await? {
                *version = self.rotate(id, *version, material, lease).await?;
            }
            match credential.read(&http, previous.as_ref(), now).await {
                Ok(snapshot) => snapshot,
                Err(antigravity_direct::ReadError::Failed(error)) => return Err(error),
                Err(antigravity_direct::ReadError::Refused) => {
                    let material = credential.force_renew(&http, now).await?;
                    *version = self.rotate(id, *version, material, lease).await?;
                    match credential.read(&http, previous.as_ref(), now).await {
                        Ok(snapshot) => snapshot,
                        Err(antigravity_direct::ReadError::Refused) => {
                            return Err(ProviderError::Unauthorized.into());
                        }
                        Err(antigravity_direct::ReadError::Failed(error)) => return Err(error),
                    }
                }
            }
        } else if account.provider == U::MiniMax
            && let Some(mut credential) = minimax_connection::Credential::decode(&bytes)
        {
            let http = self.http(account.provider).await?;
            if let Some(rotated) = credential.renew(&http, self.clock.now()).await? {
                *version = self.rotate(id, *version, rotated, lease).await?;
            }
            credential.snapshot(&http).await?
        } else if account.provider == U::Kimi
            && let Some(mut credential) = kimi_connection::Credential::decode(&bytes)
        {
            let http = self.http(account.provider).await?;
            if let Some(rotated) = credential.renew(&http, self.clock.now()).await? {
                *version = self.rotate(id, *version, rotated, lease).await?;
            }
            credential.snapshot(&http).await?
        } else if matches!(
            account.provider,
            U::Zai | U::OpenRouter | U::Mimo | U::MiniMax | U::Kimi
        ) {
            let (method, key) = KeyConnection::decode(core_provider(account.provider), &bytes)?;
            let previous = trigger.previous(db, id)?;
            method
                .read_using_http(
                    self.http(account.provider).await?,
                    &key,
                    self.clock.now(),
                    previous.as_ref(),
                    manual,
                )
                .await?
        } else {
            let http = self
                .account_http_budget(account, self.http(account.provider).await?)
                .await?;
            let (mut tokens, org) = unpack(&bytes)?;
            let now = self.clock.now();
            let mut renewed = false;
            if tokens.refresh.is_some()
                && tokens
                    .expires_at
                    .is_some_and(|at| at.0 <= now.0.saturating_add(300))
            {
                match renew_tokens(&http, account.provider, &tokens, now).await {
                    Ok(new) => {
                        *version = self
                            .rotate(id, *version, pack(&new, org.as_deref())?, lease)
                            .await?;
                        tokens = new;
                        renewed = true;
                    }
                    Err(LoginError::Grant(GrantError::Provider(_)))
                        if tokens.expires_at.is_some_and(|at| at.0 > now.0) => {}
                    Err(error) => return Err(error),
                }
            }
            let prev = trigger.previous(db, id)?;
            let result = crate::quota::read_with_policy(
                account.provider,
                &http,
                &tokens.access,
                org.as_deref(),
                now,
                prev.as_ref(),
                manual,
            )
            .await;
            if matches!(result, Err(ProviderError::Unauthorized))
                && !renewed
                && tokens.refresh.is_some()
                && matches!(
                    account.provider,
                    U::Claude | U::ChatGpt | U::Grok | U::OpenCode
                )
            {
                let fresh = renew_tokens(&http, account.provider, &tokens, now).await?;
                *version = self
                    .rotate(id, *version, pack(&fresh, org.as_deref())?, lease)
                    .await?;
                crate::quota::read_with_policy(
                    account.provider,
                    &http,
                    &fresh.access,
                    org.as_deref(),
                    now,
                    prev.as_ref(),
                    manual,
                )
                .await?
            } else {
                result?
            }
        };
        db.put_snapshot(id, *version, self.clock.now().0, &payload)?;
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RefreshTrigger {
    Scheduled,
    Manual,
    Redemption,
}

impl RefreshTrigger {
    fn previous(self, db: &AccountRegistry, id: Uuid) -> Result<Option<Value>, ConnectionError> {
        let mut snapshot = db
            .snapshots()?
            .into_iter()
            .find(|(row, _, _)| row.id == id)
            .and_then(|(_, _, v)| v);
        if self == Self::Redemption
            && let Some(obj) = snapshot.as_mut().and_then(Value::as_object_mut)
        {
            for field in ["resetsReadAt", "secondaryReadAt"] {
                obj.remove(field);
            }
        }
        Ok(snapshot)
    }
}

fn persist_candidate(
    db: &mut AccountRegistry,
    provider: UsageProvider,
    candidate: Candidate,
    existing: Option<(Uuid, CredentialVersion)>,
    fetched: i64,
    cancel: &CancellationToken,
) -> Result<Uuid, LoginError> {
    use crate::accounts::{InitialRead, KeptGrant};
    let read = candidate.snapshot;
    if let Some((id, version)) = existing {
        return match read {
            Ok(payload) => {
                db.replace_candidate(
                    id,
                    version,
                    &candidate.identity,
                    &candidate.material,
                    Some((fetched, &payload)),
                    cancel,
                )?;
                Ok(id)
            }
            Err(error) if provider == UsageProvider::Antigravity => {
                let (kind, retry_after) = error
                    .failure_class()
                    .unwrap_or((FailureKind::Service, None));
                let message = error.to_string();
                db.replace_grant_keeping_reading(
                    id,
                    version,
                    KeptGrant {
                        identity: &candidate.identity,
                        secret: &candidate.material,
                        now: fetched,
                        kind,
                        retry_after,
                        message: &message,
                    },
                    cancel,
                )?;
                Err(error)
            }
            Err(error) => Err(error),
        };
    }
    let initial = match &read {
        Ok(payload) => InitialRead::Snapshot(fetched, payload),
        Err(error) => {
            let (kind, retry_after) = error
                .failure_class()
                .unwrap_or((FailureKind::Service, None));
            InitialRead::Failed {
                at: fetched,
                kind,
                retry_after,
                message: error.to_string(),
            }
        }
    };
    let id = db.add_candidate(
        provider,
        &candidate.label,
        &candidate.identity,
        &candidate.material,
        initial,
        cancel,
    )?;
    if provider == UsageProvider::Antigravity {
        read.map(|_| id)
    } else {
        Ok(id)
    }
}

struct Candidate {
    label: String,
    identity: CandidateIdentity,
    material: SecretBytes,
    snapshot: Result<Value, LoginError>,
}

fn device_provider(provider: UsageProvider) -> Result<DeviceProvider, LoginError> {
    Ok(match provider {
        UsageProvider::Grok => DeviceProvider::Grok,
        UsageProvider::Muse => DeviceProvider::Muse,
        UsageProvider::OpenCode => DeviceProvider::OpenCode,
        _ => {
            return Err(
                ProviderError::Unsupported("no device authorization for this provider").into(),
            );
        }
    })
}

fn pack(tokens: &Tokens, org: Option<&str>) -> Result<SecretBytes, LoginError> {
    serde_json::to_vec(&(
        tokens.access.expose(),
        tokens.refresh.as_ref().map(Secret::expose),
        tokens.expires_at.map(|t| t.0),
        org,
    ))
    .map(SecretBytes::new)
    .map_err(|_| LoginError::Encoding)
}

fn unpack(bytes: &SecretBytes) -> Result<(Tokens, Option<String>), LoginError> {
    let (access, refresh, expiry, org): (String, Option<String>, Option<i64>, Option<String>) =
        serde_json::from_slice(bytes.expose()).map_err(|_| LoginError::Encoding)?;
    Ok((
        Tokens {
            access: Secret::new(access),
            refresh: refresh.map(Secret::new),
            expires_at: expiry.map(Timestamp),
        },
        org,
    ))
}

impl LoginError {
    fn failure_class(&self) -> Option<(FailureKind, Option<u64>)> {
        let kind = match self {
            Self::Provider(error) | Self::Grant(GrantError::Provider(error)) => match error {
                ProviderError::Unauthorized | ProviderError::Forbidden => {
                    FailureKind::Authentication
                }
                ProviderError::Unsupported(_) => FailureKind::Limitation,
                ProviderError::Denied { reason, .. } if reason == "SERVICE_DISABLED" => {
                    FailureKind::Limitation
                }
                ProviderError::Network(_) => FailureKind::Network,
                ProviderError::RateLimited {
                    retry_after_seconds,
                } => return Some((FailureKind::RateLimited, *retry_after_seconds)),
                _ => FailureKind::Service,
            },
            Self::Store(ConnectionError::MissingCredential)
            | Self::Grant(GrantError::SignInAgain) => FailureKind::Authentication,
            Self::Timeout
            | Self::CredentialStoreTimeout
            | Self::Store(ConnectionError::StoreBusy) => FailureKind::Timeout,
            _ => return None,
        };
        Some((kind, None))
    }
}

async fn renew_tokens(
    http: &Http,
    provider: UsageProvider,
    tokens: &Tokens,
    now: Timestamp,
) -> Result<Tokens, LoginError> {
    let refresh = tokens.refresh.as_ref().ok_or(GrantError::SignInAgain)?;
    Ok(match provider {
        UsageProvider::Claude => grants::renew(http, BrowserProvider::Claude, refresh, now).await?,
        UsageProvider::ChatGpt => {
            grants::renew(http, BrowserProvider::ChatGpt, refresh, now).await?
        }
        _ => {
            crate::device_login::renew_device(http, device_provider(provider)?, refresh, now)
                .await?
        }
    })
}
