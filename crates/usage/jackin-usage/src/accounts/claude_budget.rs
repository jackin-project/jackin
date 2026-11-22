// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AccountRegistry, ConnectionError, CredentialVersion};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::Clock;
use jackin_usage_provider_core::{ClaudeRequestBudget, ProviderError};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const STORE: ProviderError =
    ProviderError::Decode("could not read or update the Claude request budget");

pub(crate) struct Budget {
    db: Mutex<AccountRegistry>,
    clock: Arc<dyn Clock>,
    scope: String,
    mirror: Option<(Uuid, CredentialVersion)>,
}

fn identity_key(identity: &str) -> String {
    format!(
        "identity/{}",
        URL_SAFE_NO_PAD.encode(Sha256::digest(identity.as_bytes()))
    )
}

impl Budget {
    pub(crate) fn candidate(db: AccountRegistry, clock: Arc<dyn Clock>, identity: &str) -> Self {
        Self {
            db: Mutex::new(db),
            clock,
            scope: identity_key(identity),
            mirror: None,
        }
    }

    pub(crate) fn saved(
        db: AccountRegistry,
        clock: Arc<dyn Clock>,
        id: Uuid,
    ) -> Result<Self, ConnectionError> {
        let (kind, identity): (String, Option<String>) = db.db.query_row(
            "SELECT kind,identity FROM accounts WHERE id=?1 AND provider='claude'",
            [id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let slot = db.version(id)?;
        let scope = identity
            .as_deref()
            .map_or_else(|| format!("card/{id}/{}", slot.0), identity_key);
        Ok(Self {
            db: Mutex::new(db),
            clock,
            scope,
            mirror: (kind == "hub").then_some((id, slot)),
        })
    }

    fn scope(&self, db: &AccountRegistry) -> Result<String, ProviderError> {
        if let Some((id, slot)) = self.mirror {
            let learned = db
                .db
                .query_row(
                    "SELECT scope FROM claude_profile_scopes WHERE account=?1 AND slot=?2",
                    params![id.to_string(), slot.0.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|_| STORE)?;
            if let Some(scope) = learned {
                return Ok(scope);
            }
        }
        Ok(self.scope.clone())
    }
}

impl ClaudeRequestBudget for Budget {
    fn before(&self) -> Result<(String, i64), ProviderError> {
        let db = self.db.lock().map_err(|_| STORE)?;
        let scope = self.scope(&db)?;
        let (generation, until): (i64, i64) = db
            .db
            .query_row(
                "SELECT generation,until FROM claude_windows WHERE scope=?1",
                [&scope],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|_| STORE)?
            .unwrap_or_default();
        let now = self.clock.now().0;
        if until > now {
            return Err(ProviderError::RateLimited {
                retry_after_seconds: Some(until.abs_diff(now)),
            });
        }
        Ok((scope, generation))
    }

    fn after(
        &self,
        (scope, generation): (String, i64),
        profile: bool,
        response: &Result<Value, ProviderError>,
    ) -> Result<(), ProviderError> {
        if !matches!(response, Ok(_) | Err(ProviderError::RateLimited { .. })) {
            return Ok(());
        }
        let mut db = self.db.lock().map_err(|_| STORE)?;
        let tx = db
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| STORE)?;
        if let Err(ProviderError::RateLimited {
            retry_after_seconds,
        }) = response
        {
            let now = self.clock.now().0;
            let (old, strikes): (i64, u32) = tx
                .query_row(
                    "SELECT until,strikes FROM claude_windows WHERE scope=?1",
                    [&scope],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|_| STORE)?
                .unwrap_or_default();
            let delay = (300_u64 << strikes.min(4))
                .max(retry_after_seconds.unwrap_or_default())
                .min(3600);
            let until = old.max(now.saturating_add(i64::try_from(delay).unwrap_or(3600)));
            tx.execute("INSERT INTO claude_windows VALUES (?1,1,1,?2) ON CONFLICT(scope) DO UPDATE SET generation=generation+1,strikes=min(strikes+1,16),until=excluded.until",params![scope,until]).map_err(|_|STORE)?;
            tx.commit().map_err(|_| STORE)?;
            return Err(ProviderError::RateLimited {
                retry_after_seconds: Some(until.abs_diff(now)),
            });
        }
        tx.execute("UPDATE claude_windows SET until=0,strikes=0,generation=generation+1 WHERE scope=?1 AND generation=?2 AND strikes>0",params![scope,generation]).map_err(|_|STORE)?;
        if let (true, Some((id, slot)), Ok(body)) = (profile, self.mirror, response)
            && let Some((identity, _)) = crate::grants::claude_identity(body)
        {
            tx.execute("INSERT INTO claude_profile_scopes(account,slot,scope) SELECT id,slot,?3 FROM accounts WHERE id=?1 AND slot=?2 ON CONFLICT(account) DO UPDATE SET slot=excluded.slot,scope=excluded.scope",params![id.to_string(),slot.0.to_string(),identity_key(&identity)]).map_err(|_|STORE)?;
        }
        tx.commit().map_err(|_| STORE)
    }
}
