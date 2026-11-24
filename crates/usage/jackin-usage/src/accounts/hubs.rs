// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use super::{
    AccountRegistry, ConnectionError, CredentialVersion, UsageProvider, uuid as read_uuid,
};
use jackin_core::SecretBytes;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Hub {
    pub id: Uuid,
    pub address: String,
    pub refused: bool,
}

#[derive(Debug)]
pub struct Mirror {
    pub hub: Uuid,
    pub index: String,
    pub workspace: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Listing {
    files: Vec<Login>,
}

#[derive(Debug, Deserialize)]
struct Login {
    id: Option<String>,
    auth_index: Option<String>,
    provider: Option<String>,
    r#type: Option<String>,
    account_type: Option<String>,
    email: Option<String>,
    account: Option<String>,
    label: Option<String>,
    name: Option<String>,
    disabled: Option<bool>,
    id_token: Option<Value>,
}

impl Login {
    fn provider(&self) -> Option<UsageProvider> {
        if self.disabled == Some(true)
            || self
                .account_type
                .as_ref()
                .is_some_and(|t| !t.eq_ignore_ascii_case("oauth"))
        {
            return None;
        }
        match self
            .provider
            .as_ref()
            .or(self.r#type.as_ref())?
            .to_ascii_lowercase()
            .as_str()
        {
            "claude" => Some(UsageProvider::Claude),
            "codex" => Some(UsageProvider::ChatGpt),
            "xai" => Some(UsageProvider::Grok),
            _ => None,
        }
    }
}

impl AccountRegistry {
    pub fn hubs(&self) -> Result<Vec<Hub>, ConnectionError> {
        let mut stmt = self
            .db
            .prepare("SELECT id,address,refused FROM hubs ORDER BY rowid")?;
        Ok(stmt
            .query_map([], |r| {
                Ok(Hub {
                    id: read_uuid(r, 0)?,
                    address: r.get(1)?,
                    refused: r.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?)
    }

    pub fn hub_link(&self, id: Uuid) -> Result<Option<Mirror>, ConnectionError> {
        Ok(self
            .db
            .query_row(
                "SELECT hub,auth_index,workspace FROM mirrors WHERE id=?1",
                [id.to_string()],
                |r| {
                    Ok(Mirror {
                        hub: read_uuid(r, 0)?,
                        index: r.get(1)?,
                        workspace: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn hub_version(&self, id: Uuid) -> Result<CredentialVersion, ConnectionError> {
        self.db
            .query_row("SELECT slot FROM hubs WHERE id=?1", [id.to_string()], |r| {
                Ok(CredentialVersion(read_uuid(r, 0)?))
            })
            .optional()?
            .ok_or(ConnectionError::Changed)
    }

    pub fn hub_key(
        &self,
        id: Uuid,
    ) -> Result<(Hub, CredentialVersion, SecretBytes), ConnectionError> {
        let hub = self
            .hubs()?
            .into_iter()
            .find(|hub| hub.id == id)
            .ok_or(ConnectionError::Changed)?;
        if hub.refused {
            return Err(ConnectionError::HubRefused);
        }
        let version = self.hub_version(id)?;
        let key = self.bound_hub_key(&hub, version)?;
        Ok((hub, version, key))
    }

    fn bound_hub_key(
        &self,
        hub: &Hub,
        version: CredentialVersion,
    ) -> Result<SecretBytes, ConnectionError> {
        let envelope = self.vault.load(version.0)?;
        let (address, key): (&str, &str) =
            serde_json::from_slice(envelope.expose()).map_err(|_| ConnectionError::Invalid)?;
        if address != hub.address {
            return Err(ConnectionError::Invalid);
        }
        Ok(SecretBytes::new(key.as_bytes().to_vec()))
    }

    pub fn check_hub_address(
        &self,
        id: Uuid,
        address: &str,
    ) -> Result<CredentialVersion, ConnectionError> {
        let hub = self
            .hubs()?
            .into_iter()
            .find(|h| h.id == id && h.address == address)
            .ok_or(ConnectionError::Changed)?;
        let slot = self.hub_version(id)?;
        self.bound_hub_key(&hub, slot)?;
        Ok(slot)
    }

    pub fn hub_lock(&self, id: Uuid) -> Result<std::fs::File, ConnectionError> {
        let f = super::private::file(&self.directory.join(format!("hub-{id}.lock")))?;
        f.lock()?;
        self.hub_version(id)?;
        Ok(f)
    }

    pub fn install_hub(
        &mut self,
        existing: Option<(Uuid, CredentialVersion)>,
        address: &str,
        key: &SecretBytes,
        listing: &Listing,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<Uuid, ConnectionError> {
        let _hub_lock = existing.map(|(id, _)| self.hub_lock(id)).transpose()?;
        let id = existing.map_or_else(Uuid::new_v4, |(id, _)| id);
        let key = std::str::from_utf8(key.expose()).map_err(|_| ConnectionError::Invalid)?;
        let envelope = SecretBytes::new(
            serde_json::to_vec(&(address, key)).map_err(|_| ConnectionError::Invalid)?,
        );
        self.install_cancellable(&envelope, cancel, |tx, slot| {
            if let Some((_, version)) = existing {
                if tx.execute(
                    "UPDATE hubs SET slot=?1,refused=0 WHERE id=?2 AND slot=?3 AND address=?4",
                    params![
                        slot.to_string(),
                        id.to_string(),
                        version.0.to_string(),
                        address
                    ],
                )? == 0
                {
                    return Err(ConnectionError::Changed);
                }
                tx.execute(
                    "INSERT INTO retired_slots VALUES (?1)",
                    [version.0.to_string()],
                )?;
                let mut ids = tx.prepare("SELECT id FROM mirrors WHERE hub=?1")?;
                for row in ids.query_map([id.to_string()], |r| r.get::<_, String>(0))? {
                    tx.execute(
                        "UPDATE accounts SET slot=?1 WHERE id=?2",
                        params![Uuid::new_v4().to_string(), row?],
                    )?;
                }
            } else if tx.execute(
                "INSERT OR IGNORE INTO hubs(id,address,slot) VALUES (?1,?2,?3)",
                params![id.to_string(), address, slot.to_string()],
            )? == 0
            {
                return Err(ConnectionError::Duplicate);
            }
            reconcile(tx, id, listing)
        })?;
        Ok(id)
    }

    pub fn sync_hub(
        &mut self,
        id: Uuid,
        version: CredentialVersion,
        listing: &Listing,
    ) -> Result<(), ConnectionError> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = tx
            .query_row("SELECT slot FROM hubs WHERE id=?1", [id.to_string()], |r| {
                read_uuid(r, 0)
            })
            .optional()?;
        if current != Some(version.0) {
            return Err(ConnectionError::Changed);
        }
        reconcile(&tx, id, listing)?;
        tx.commit()?;
        Ok(())
    }

    pub fn refuse_hub(&self, id: Uuid, version: CredentialVersion) -> Result<(), ConnectionError> {
        self.db.execute(
            "UPDATE hubs SET refused=1 WHERE id=?1 AND slot=?2",
            params![id.to_string(), version.0.to_string()],
        )?;
        Ok(())
    }

    pub fn remove_hub(&mut self, id: Uuid) -> Result<(), ConnectionError> {
        let _hub_lock = self.hub_lock(id)?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = id.to_string();
        if tx.execute(
            "INSERT INTO retired_slots SELECT slot FROM hubs WHERE id=?1",
            [&id],
        )? == 0
        {
            return Err(ConnectionError::Changed);
        }
        tx.execute(
            "DELETE FROM accounts WHERE id IN (SELECT id FROM mirrors WHERE hub=?1)",
            [&id],
        )?;
        tx.execute("DELETE FROM hubs WHERE id=?1", [&id])?;
        tx.commit()?;
        self.clean_retired()
            .map_err(|_| ConnectionError::RemovedPendingCleanup)
    }
}

fn reconcile(tx: &Connection, hub: Uuid, listing: &Listing) -> Result<(), ConnectionError> {
    let hub = hub.to_string();
    let mut stmt=tx.prepare("SELECT mirrors.login,accounts.id,accounts.provider FROM mirrors JOIN accounts USING(id) WHERE hub=?1")?;
    let mut old: BTreeMap<String, (String, String)> = stmt
        .query_map([&hub], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
        .collect::<Result<_, _>>()?;
    let mut seen = BTreeSet::new();
    for entry in &listing.files {
        let Some(login) = entry.id.as_deref().filter(|id| !id.is_empty()) else {
            continue;
        };
        if !seen.insert(login) {
            continue;
        }
        let Some(provider) = entry.provider() else {
            continue;
        };
        let Some(index) = entry.auth_index.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        let previous = old.remove(login);
        let id = match previous {
            Some((id, kind)) if kind == provider.as_str() => Some(id),
            Some((id, _)) => {
                tx.execute("DELETE FROM accounts WHERE id=?1", [id])?;
                None
            }
            None => None,
        };
        let workspace = entry
            .id_token
            .as_ref()
            .and_then(|v| v["chatgpt_account_id"].as_str());
        if let Some(id) = id {
            if tx.execute("UPDATE mirrors SET auth_index=?1,workspace=?2 WHERE id=?3 AND (auth_index<>?1 OR workspace IS NOT ?2)",params![index,workspace,id])?!=0{
                tx.execute("UPDATE accounts SET slot=?1, snapshot=NULL,fetched_at=NULL WHERE id=?2",params![Uuid::new_v4().to_string(),id])?;
            }
        } else {
            let label = [&entry.email, &entry.account, &entry.label, &entry.name]
                .into_iter()
                .filter_map(|t| t.as_deref())
                .map(str::trim)
                .find(|s| !s.is_empty())
                .unwrap_or(login);
            let id = Uuid::new_v4().to_string();
            tx.execute("INSERT INTO accounts(id,provider,label,kind,slot,position) SELECT ?1,?2,?3,'hub',?4,COALESCE(MAX(position),-1)+1 FROM accounts",params![id,provider.as_str(),label,Uuid::new_v4().to_string()])?;
            tx.execute(
                "INSERT INTO mirrors VALUES(?1,?2,?3,?4,?5)",
                params![id, hub, login, index, workspace],
            )?;
        }
    }
    for (id, _) in old.into_values() {
        tx.execute("DELETE FROM accounts WHERE id=?1", [id])?;
    }
    Ok(())
}

impl AccountRegistry {
    pub fn proxy(&self) -> Result<Option<SecretBytes>, ConnectionError> {
        self.db
            .query_row("SELECT slot FROM usage_proxy WHERE id=1", [], |r| {
                read_uuid(r, 0)
            })
            .optional()?
            .map(|id| self.vault.load(id))
            .transpose()
    }

    pub fn set_proxy(&mut self, secret: Option<&SecretBytes>) -> Result<(), ConnectionError> {
        let retire = |tx: &Connection| -> Result<(), ConnectionError> {
            tx.execute(
                "INSERT INTO retired_slots SELECT slot FROM usage_proxy WHERE id=1",
                [],
            )?;
            tx.execute("DELETE FROM usage_proxy WHERE id=1", [])?;
            Ok(())
        };
        if let Some(secret) = secret {
            self.install(secret, |tx, slot| {
                retire(tx)?;
                tx.execute("INSERT INTO usage_proxy VALUES(1,?1)", [slot.to_string()])?;
                Ok(())
            })?;
        } else {
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            retire(&tx)?;
            tx.commit()?;
            self.clean_retired()?;
        }
        Ok(())
    }
}
