// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use super::{LoginError, OwnedLogin, RefreshTrigger};
use crate::accounts::hubs::{Listing, Mirror};
use crate::accounts::{
    AccountRegistry, ConnectedAccount, ConnectionError, CredentialVersion, UsageProvider,
};
use jackin_core::{Secret, SecretBytes};
use jackin_usage_provider_core::{Auth, Http, HubAddress, HubProvider, ProviderError};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

impl OwnedLogin {
    pub async fn connect_hub(
        &self,
        address: &str,
        key: &Secret,
        replace: Option<Uuid>,
        cancel: &CancellationToken,
    ) -> Result<Uuid, LoginError> {
        let address = HubAddress::parse(address)?;
        let http = self.base_http()?;
        let mut db = self.open_registry()?;
        let existing = replace
            .map(|id| db.check_hub_address(id, address.as_str()).map(|v| (id, v)))
            .transpose()?;
        let listing = tokio::select! {
            biased;
            ()=cancel.cancelled()=>return Err(LoginError::Cancelled),
            result=list(&http,&address,key.expose())=>result?,
        };
        if cancel.is_cancelled() {
            return Err(LoginError::Cancelled);
        }
        let key = SecretBytes::new(key.expose().as_bytes().to_vec());
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            db.install_hub(existing, address.as_str(), &key, &listing, &cancel)
        })
        .await
        .map_err(|_| LoginError::Worker)?
        .map_err(Into::into)
    }

    pub async fn remove_hub(&self, id: Uuid) -> Result<(), LoginError> {
        let service = self.clone();
        tokio::task::spawn_blocking(move || service.open_registry()?.remove_hub(id))
            .await
            .map_err(|_| LoginError::Worker)?
            .map_err(Into::into)
    }

    pub async fn sync_hub(&self, id: Uuid, cancel: &CancellationToken) -> Result<(), LoginError> {
        let mut db = self.open_registry()?;
        let service = self.clone();
        let _lease = tokio::task::spawn_blocking(move || service.open_registry()?.hub_lock(id))
            .await
            .map_err(|_| LoginError::Worker)??;
        let (hub, version, key) = db.hub_key(id)?;
        let http = self.base_http()?;
        let address = HubAddress::parse(&hub.address)?;
        let key = std::str::from_utf8(key.expose()).map_err(|_| LoginError::Encoding)?;
        let listing = tokio::select! {
            biased;
            ()=cancel.cancelled()=>return Err(LoginError::Cancelled),
            result=list(&http,&address,key)=>result,
        };
        if matches!(
            listing,
            Err(ProviderError::Forbidden | ProviderError::Unauthorized)
        ) {
            db.refuse_hub(id, version)?;
        }
        Ok(db.sync_hub(id, version, &listing?)?)
    }

    pub(super) async fn mirror_session(
        &self,
        db: &mut AccountRegistry,
        account: &ConnectedAccount,
        link: Mirror,
    ) -> Result<MirrorSession, LoginError> {
        let service = self.clone();
        let id = link.hub;
        let lease = tokio::task::spawn_blocking(move || service.open_registry()?.hub_lock(id))
            .await
            .map_err(|_| LoginError::Worker)??;
        let link = db
            .hub_link(account.id)?
            .filter(|link| link.hub == id)
            .ok_or(ConnectionError::Changed)?;
        let (hub, slot, key) = db.hub_key(id)?;
        let provider = match account.provider {
            UsageProvider::Claude => HubProvider::Claude,
            UsageProvider::ChatGpt => HubProvider::ChatGpt,
            UsageProvider::Grok => HubProvider::Grok,
            _ => return Err(ConnectionError::CredentialKind.into()),
        };
        let http = self.base_http()?.routed_via_hub(
            HubAddress::parse(&hub.address)?,
            std::str::from_utf8(key.expose())
                .map_err(|_| LoginError::Encoding)?
                .to_owned(),
            link.index,
            provider,
        );
        Ok(MirrorSession {
            _lease: lease,
            http: self.account_http_budget(account, http)?,
            id,
            slot,
            org: link.workspace,
        })
    }

    pub(super) async fn mirror_usage(
        &self,
        db: &mut AccountRegistry,
        account: &ConnectedAccount,
        link: Mirror,
        trigger: RefreshTrigger,
    ) -> Result<Value, LoginError> {
        let session = self.mirror_session(db, account, link).await?;
        let previous = trigger.previous(db, account.id)?;
        let result = crate::quota::read_with_policy(
            account.provider,
            &session.http,
            &Secret::new("$TOKEN$".into()),
            session.org.as_deref(),
            self.clock.now(),
            previous.as_ref(),
            trigger != RefreshTrigger::Scheduled,
        )
        .await;
        if matches!(result, Err(ProviderError::HubRefused)) {
            db.refuse_hub(session.id, session.slot)?;
        }
        Ok(result?)
    }
}

pub(super) struct MirrorSession {
    _lease: std::fs::File,
    pub(super) http: Http,
    pub(super) id: Uuid,
    pub(super) slot: CredentialVersion,
    pub(super) org: Option<String>,
}

async fn list(http: &Http, address: &HubAddress, key: &str) -> Result<Listing, ProviderError> {
    let body = http
        .get(&address.logins_url(), Auth::Bearer(key), &[])
        .await?;
    serde_json::from_value(body)
        .map_err(|_| ProviderError::Decode("invalid CLIProxyAPI login listing"))
}
