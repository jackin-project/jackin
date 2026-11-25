// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Secret;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    AccountRegistry, BrowserProvider, ConnectedAccount, KeyConnection, LoginError, OwnedLogin,
    UsageProvider, core_provider, grants, pack, unpack,
};
use crate::accounts::CredentialVersion;
use crate::resets::{ClaimOutcome, ResetError, ResetSelection};

impl OwnedLogin {
    pub async fn redeem_reset(
        &self,
        selection: ResetSelection,
        cancel: &CancellationToken,
    ) -> Result<ClaimOutcome, LoginError> {
        let id = selection.account;
        let lease = self.account_lease(id, cancel).await?;
        let mut db = self.open_registry()?;
        let account = db
            .list()?
            .into_iter()
            .find(|a| a.id == id && a.provider == selection.provider)
            .ok_or(ResetError::Changed)?;
        if let Some(f) = db.failure(id)?
            && f.holds(self.clock.now().0, true)
        {
            return Err(LoginError::Wait(f.retry_at));
        }
        let result = tokio::select! {
            biased;
            ()=cancel.cancelled()=>return Err(LoginError::Cancelled),
            result=self.claim(&mut db,&account,&selection)=>result,
        };
        drop(lease);
        let refreshed = self
            .refresh_for(&account, super::RefreshTrigger::Redemption, cancel)
            .await;
        match (result, refreshed) {
            (Ok(ClaimOutcome::Spent), Err(_)) => Ok(ClaimOutcome::SpentRefreshFailed),
            (result, _) => result,
        }
    }

    async fn claim(
        &self,
        db: &mut AccountRegistry,
        account: &ConnectedAccount,
        selection: &ResetSelection,
    ) -> Result<ClaimOutcome, LoginError> {
        if let Some(link) = db.hub_link(account.id)? {
            let hub = self.mirror_session(db, account, link).await?;
            let access = Secret::new("$TOKEN$".into());
            let org = if account.provider == UsageProvider::Claude {
                let profile = hub
                    .http
                    .get(
                        "https://api.anthropic.com/api/oauth/profile",
                        jackin_usage_provider_core::Auth::Bearer(access.expose()),
                        &[("anthropic-beta", "oauth-2025-04-20")],
                    )
                    .await;
                match profile {
                    Ok(body) => body
                        .pointer("/organization/uuid")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    Err(jackin_usage_provider_core::ProviderError::HubRefused) => {
                        db.refuse_hub(hub.id, hub.slot)?;
                        return Err(jackin_usage_provider_core::ProviderError::HubRefused.into());
                    }
                    Err(e) => return Err(e.into()),
                }
            } else {
                hub.org.clone()
            };
            let version = db.version(account.id)?;
            let result = self
                .send_claim(db, selection, &hub.http, &access, org.as_deref(), version)
                .await;
            if matches!(
                result,
                Err(LoginError::Reset(ResetError::Provider(
                    jackin_usage_provider_core::ProviderError::HubRefused
                )))
            ) {
                db.refuse_hub(hub.id, hub.slot)?;
            }
            return result;
        }
        let (access, org, version) = self.claim_access(db, account, false).await?;
        let http = self.account_http_budget(account, self.http(account.provider)?)?;
        let result = self
            .send_claim(db, selection, &http, &access, org.as_deref(), version)
            .await;
        if matches!(
            result,
            Err(LoginError::Reset(ResetError::Provider(
                jackin_usage_provider_core::ProviderError::Unauthorized
            )))
        ) && matches!(
            account.provider,
            UsageProvider::Claude | UsageProvider::ChatGpt
        ) {
            let (access, org, version) = self.claim_access(db, account, true).await?;
            return self
                .send_claim(db, selection, &http, &access, org.as_deref(), version)
                .await;
        }
        result
    }

    async fn send_claim(
        &self,
        db: &mut AccountRegistry,
        selection: &ResetSelection,
        http: &jackin_usage_provider_core::Http,
        access: &Secret,
        org: Option<&str>,
        version: CredentialVersion,
    ) -> Result<ClaimOutcome, LoginError> {
        let key = selection.key();
        let existing = db.prior_claim(
            selection.account,
            &key,
            selection.pending_scope().as_deref(),
            selection.unresolved_expiry().as_deref(),
        )?;
        let (key, request, confirmed) = match existing {
            Some((_, _, true)) => return Ok(ClaimOutcome::Spent),
            Some((_, _, false)) if selection.provider == UsageProvider::Zai => {
                return Err(ResetError::Unconfirmed.into());
            }
            Some((original, request, false)) => {
                let confirmed = selection.bind_replay(&original)?;
                (original, request, confirmed)
            }
            None => {
                let confirmed = selection
                    .recheck(http, access, org, self.clock.now())
                    .await?;
                let request = Uuid::new_v4();
                let key = confirmed.key();
                db.record_claim(selection.account, version, &key, request)?;
                (key, request, confirmed)
            }
        };
        let outcome = confirmed.submit(http, access, org, request).await?;
        if outcome != ClaimOutcome::Unconfirmed {
            db.settle_claim(selection.account, &key, outcome == ClaimOutcome::Spent)?;
        }
        Ok(outcome)
    }

    async fn claim_access(
        &self,
        db: &mut AccountRegistry,
        account: &ConnectedAccount,
        force: bool,
    ) -> Result<(Secret, Option<String>, CredentialVersion), LoginError> {
        let (mut version, bytes) = db.credential(account.id)?;
        if account.provider == UsageProvider::Zai {
            let (_, key) = KeyConnection::decode(core_provider(account.provider), &bytes)?;
            return Ok((key, None, version));
        }
        let provider = match account.provider {
            UsageProvider::Claude => BrowserProvider::Claude,
            UsageProvider::ChatGpt => BrowserProvider::ChatGpt,
            _ => return Err(ResetError::Ineligible.into()),
        };
        let (mut tokens, org) = unpack(&bytes)?;
        let now = self.clock.now();
        if let Some(refresh) = &tokens.refresh
            && (force
                || tokens
                    .expires_at
                    .is_some_and(|at| at.0 <= now.0.saturating_add(300)))
        {
            tokens = grants::renew(&self.http(account.provider)?, provider, refresh, now).await?;
            version = db.rotate(account.id, version, &pack(&tokens, org.as_deref())?)?;
        }
        Ok((tokens.access, org, version))
    }
}
