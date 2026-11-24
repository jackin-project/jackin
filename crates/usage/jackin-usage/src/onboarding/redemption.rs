// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Secret;
use jackin_usage_provider_core::ProviderError;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    AccountRegistry, BrowserProvider, ConnectedAccount, GrantError, KeyConnection, LoginError,
    OwnedLogin, UsageProvider, core_provider, grants, pack, unpack,
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
        let mut db = self.with_registry(Ok).await?;
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
            result=self.claim(&mut db,&account,&selection,&lease)=>result,
        };
        drop(lease);
        if result.as_ref().is_err_and(ambiguous_claim) {
            return result;
        }
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
        lease: &std::fs::File,
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
                    Err(ProviderError::HubRefused) => {
                        db.refuse_hub(hub.id, hub.slot)?;
                        return Err(ProviderError::HubRefused.into());
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
                    ProviderError::HubRefused
                )))
            ) {
                db.refuse_hub(hub.id, hub.slot)?;
            }
            return result;
        }
        let (access, org, version) = self.claim_access(account, false, lease).await?;
        let http = self
            .account_http_budget(account, self.http(account.provider).await?)
            .await?;
        let result = self
            .send_claim(db, selection, &http, &access, org.as_deref(), version)
            .await;
        if matches!(
            result,
            Err(LoginError::Reset(ResetError::Provider(
                ProviderError::Unauthorized
            )))
        ) && matches!(
            account.provider,
            UsageProvider::Claude | UsageProvider::ChatGpt
        ) {
            let (access, org, version) = self.claim_access(account, true, lease).await?;
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
        account: &ConnectedAccount,
        force: bool,
        lease: &std::fs::File,
    ) -> Result<(Secret, Option<String>, CredentialVersion), LoginError> {
        let (mut version, bytes) = self.read_credential(account.id).await?;
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
            tokens =
                grants::renew(&self.http(account.provider).await?, provider, refresh, now).await?;
            version = self
                .rotate(account.id, version, pack(&tokens, org.as_deref())?, lease)
                .await?;
        }
        Ok((tokens.access, org, version))
    }
}

fn ambiguous_claim(error: &LoginError) -> bool {
    let provider = match error {
        LoginError::Timeout => return true,
        LoginError::Provider(error)
        | LoginError::Grant(GrantError::Provider(error))
        | LoginError::Reset(ResetError::Provider(error)) => error,
        _ => return false,
    };
    match provider {
        ProviderError::Network(_) => true,
        ProviderError::Status(status) | ProviderError::Denied { status, .. } => *status >= 500,
        _ => false,
    }
}
