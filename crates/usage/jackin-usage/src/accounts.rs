// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod claims;
pub(crate) mod claude_budget;
pub mod hubs;
mod private;
mod refresh;
mod settings;
pub use refresh::{FailureKind, RefreshFailure};
mod types;
mod vault;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jackin_core::SecretBytes;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, TransactionBehavior, params};
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub use types::{
    AccountPreferences, CandidateIdentity, ConnectedAccount, CredentialVersion, KeyFingerprint,
    UsageProvider,
};
pub use vault::CredentialVault;

pub const DATABASE_PATH: &str = "usage/accounts.sqlite3";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS accounts (
    id TEXT PRIMARY KEY, provider TEXT NOT NULL, label TEXT NOT NULL,
    kind TEXT NOT NULL, identity TEXT, slot TEXT UNIQUE NOT NULL, position INTEGER NOT NULL,
    preferences TEXT NOT NULL DEFAULT '{}', fetched_at INTEGER, snapshot TEXT,
    UNIQUE(provider, kind, identity)
);
CREATE TABLE IF NOT EXISTS refresh_failures (
    id TEXT PRIMARY KEY, slot TEXT NOT NULL, kind TEXT NOT NULL, failures INTEGER NOT NULL,
    retry_at INTEGER NOT NULL, message TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS clear_removed_failure AFTER DELETE ON accounts BEGIN
    DELETE FROM refresh_failures WHERE id = OLD.id;
END;
CREATE TRIGGER IF NOT EXISTS clear_old_failure AFTER UPDATE OF slot, snapshot ON accounts BEGIN
    DELETE FROM refresh_failures WHERE id = OLD.id;
END;
CREATE TABLE IF NOT EXISTS claude_windows (
    scope TEXT PRIMARY KEY, generation INTEGER NOT NULL, strikes INTEGER NOT NULL, until INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS claude_profile_scopes (
    account TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE, slot TEXT NOT NULL, scope TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS clear_claude_profile AFTER DELETE ON accounts BEGIN
    DELETE FROM claude_profile_scopes WHERE account=OLD.id;
END;
CREATE TABLE IF NOT EXISTS reset_claims (
    account TEXT NOT NULL, target TEXT NOT NULL, request TEXT NOT NULL, done INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(account,target)
);
CREATE TRIGGER IF NOT EXISTS clear_removed_claims AFTER DELETE ON accounts BEGIN
    DELETE FROM reset_claims WHERE account=OLD.id;
END;
CREATE TABLE IF NOT EXISTS dashboard_settings (id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS usage_proxy(id INTEGER PRIMARY KEY CHECK(id=1),slot TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS hubs(id TEXT PRIMARY KEY,address TEXT UNIQUE NOT NULL,slot TEXT NOT NULL,refused INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS mirrors(id TEXT PRIMARY KEY,hub TEXT NOT NULL,login TEXT NOT NULL,auth_index TEXT NOT NULL,workspace TEXT, UNIQUE(hub,login));
CREATE TRIGGER IF NOT EXISTS clear_mirror AFTER DELETE ON accounts BEGIN DELETE FROM mirrors WHERE id=OLD.id; END;
CREATE TABLE IF NOT EXISTS retired_slots (slot TEXT PRIMARY KEY);
";
const COLUMNS: &str = "id, provider, label, position, preferences";

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("usage account operation cancelled")]
    Cancelled,
    #[error("manage this login on its hub, or remove the local hub connection")]
    HubManaged,
    #[error("the hub refused its management key; replace the management key")]
    HubRefused,
    #[error("usage metadata could not be read or written")]
    Database(#[from] rusqlite::Error),
    #[error("usage metadata or lock is inaccessible")]
    Io(#[from] std::io::Error),
    #[error("usage metadata must be in a private user-owned directory")]
    Private,
    #[error("usage metadata is invalid")]
    Invalid,
    #[error("unknown usage provider")]
    Provider,
    #[error("this credential type is unavailable for this account")]
    CredentialKind,
    #[error("unlock the OS credential store and retry; on Linux start a Secret Service session")]
    Vault,
    #[error("usage account removed; unlock the credential store and retry cleanup")]
    RemovedPendingCleanup,
    #[error("this sign-in or key is already connected")]
    Duplicate,
    #[error("the sign-in belongs to a different account; use Add Account")]
    DifferentIdentity,
    #[error("the usage account changed or was removed; reopen it and retry")]
    Changed,
    #[error("the account label is empty")]
    EmptyLabel,
    #[error("the credential is missing; sign in again")]
    MissingCredential,
}

#[derive(Debug)]
pub struct AccountRegistry {
    db: Connection,
    directory: PathBuf,
    vault: Arc<dyn CredentialVault>,
}

#[derive(Debug)]
pub enum InitialRead<'a> {
    Pending,
    Snapshot(i64, &'a Value),
    Failed {
        at: i64,
        kind: FailureKind,
        retry_after: Option<u64>,
        message: String,
    },
}

pub type StoredSnapshot = (ConnectedAccount, Option<i64>, Option<Value>);

impl AccountRegistry {
    pub fn proxy_configured(&self) -> Result<bool, ConnectionError> {
        Ok(self
            .db
            .query_row("SELECT EXISTS(SELECT 1 FROM usage_proxy)", [], |row| {
                row.get(0)
            })?)
    }

    pub fn open(path: &Path) -> Result<Self, ConnectionError> {
        Self::with_vault(path, Arc::new(vault::SystemVault))
    }

    pub fn open_read_only(path: &Path) -> Result<Option<Self>, ConnectionError> {
        match std::fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
        let directory = path.parent().ok_or(ConnectionError::Private)?.to_owned();
        private::check_directory(&directory)?;
        private::check_metadata(&std::fs::symlink_metadata(path)?)?;
        let directory = directory.canonicalize()?;
        let db = Connection::open_with_flags(
            directory.join(path.file_name().ok_or(ConnectionError::Private)?),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(Duration::from_secs(10))?;
        Ok(Some(Self {
            db,
            directory,
            vault: Arc::new(vault::SystemVault),
        }))
    }

    pub fn with_vault(
        path: &Path,
        vault: Arc<dyn CredentialVault>,
    ) -> Result<Self, ConnectionError> {
        let directory = path.parent().ok_or(ConnectionError::Private)?.to_owned();
        private::directory(&directory)?;
        let _private_file = private::file(path)?;
        let directory = directory.canonicalize()?;
        let db = Connection::open_with_flags(
            directory.join(path.file_name().ok_or(ConnectionError::Private)?),
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(Duration::from_secs(10))?;
        db.execute_batch(SCHEMA)?;
        let registry = Self {
            db,
            directory,
            vault,
        };
        let _cleanup = registry.clean_idle_slots();
        Ok(registry)
    }

    pub fn list(&self) -> Result<Vec<ConnectedAccount>, ConnectionError> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT {COLUMNS} FROM accounts ORDER BY position,id"
        ))?;
        Ok(stmt.query_map([], account)?.collect::<Result<_, _>>()?)
    }

    pub fn version(&self, id: Uuid) -> Result<CredentialVersion, ConnectionError> {
        self.db
            .query_row(
                "SELECT slot FROM accounts WHERE id=?1",
                [id.to_string()],
                |r| uuid(r, 0),
            )
            .optional()?
            .map(CredentialVersion)
            .ok_or(ConnectionError::Changed)
    }

    pub fn uses_api_key(&self, id: Uuid) -> Result<bool, ConnectionError> {
        self.db
            .query_row(
                "SELECT kind='key' FROM accounts WHERE id=?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(ConnectionError::Changed)
    }

    pub fn credential(
        &self,
        id: Uuid,
    ) -> Result<(CredentialVersion, SecretBytes), ConnectionError> {
        let _lock = self.mutation_lock()?;
        let version = self.version(id)?;
        Ok((version, self.vault.load(version.0)?))
    }

    fn mutation_lock(&self) -> Result<File, ConnectionError> {
        let f = private::file(&self.directory.join("credentials.lock"))?;
        f.lock()?;
        Ok(f)
    }

    pub(crate) fn renewal_file(&self, id: Uuid) -> Result<File, ConnectionError> {
        self.version(id)?;
        private::file(&self.directory.join(format!("renew-{id}.lock")))
    }

    pub fn add(
        &mut self,
        provider: UsageProvider,
        label: &str,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
    ) -> Result<Uuid, ConnectionError> {
        self.add_candidate(
            provider,
            label,
            identity,
            secret,
            InitialRead::Pending,
            &CancellationToken::new(),
        )
    }

    pub fn add_with_snapshot(
        &mut self,
        provider: UsageProvider,
        label: &str,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
        fetched_at: i64,
        payload: &Value,
    ) -> Result<Uuid, ConnectionError> {
        self.add_candidate(
            provider,
            label,
            identity,
            secret,
            InitialRead::Snapshot(fetched_at, payload),
            &CancellationToken::new(),
        )
    }

    pub fn add_candidate(
        &mut self,
        provider: UsageProvider,
        label: &str,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
        initial: InitialRead<'_>,
        cancel: &CancellationToken,
    ) -> Result<Uuid, ConnectionError> {
        let label = nonempty(label)?;
        let (kind, identity) = identity.parts(provider)?;
        let id = Uuid::new_v4();
        self.install_cancellable(secret, cancel, |tx, slot| {
            reject_duplicate(tx, provider, kind, identity, id)?;
            tx.execute(
                "INSERT INTO accounts(id, provider, label, kind, identity, slot, position)
                SELECT ?1,?2,?3,?4,?5,?6,COALESCE(MAX(position),-1)+1 FROM accounts",
                params![
                    id.to_string(),
                    provider.as_str(),
                    label,
                    kind,
                    identity,
                    slot.to_string()
                ],
            )?;
            match initial {
                InitialRead::Pending => {}
                InitialRead::Snapshot(at, payload) => {
                    write_snapshot(tx, id, CredentialVersion(slot), at, payload)?;
                }
                InitialRead::Failed {
                    at,
                    kind,
                    retry_after,
                    message,
                } => refresh::write_failure(
                    tx,
                    id,
                    CredentialVersion(slot),
                    at,
                    kind,
                    retry_after,
                    &message,
                )?,
            }
            Ok(())
        })?;
        Ok(id)
    }

    pub fn replace(
        &mut self,
        id: Uuid,
        expected: CredentialVersion,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
    ) -> Result<(), ConnectionError> {
        self.replace_candidate(
            id,
            expected,
            identity,
            secret,
            None,
            &CancellationToken::new(),
        )
    }

    pub fn replace_with_snapshot(
        &mut self,
        id: Uuid,
        expected: CredentialVersion,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
        fetched_at: i64,
        payload: &Value,
    ) -> Result<(), ConnectionError> {
        self.replace_candidate(
            id,
            expected,
            identity,
            secret,
            Some((fetched_at, payload)),
            &CancellationToken::new(),
        )
    }

    pub fn replace_candidate(
        &mut self,
        id: Uuid,
        expected: CredentialVersion,
        identity: &CandidateIdentity,
        secret: &SecretBytes,
        snapshot: Option<(i64, &Value)>,
        cancel: &CancellationToken,
    ) -> Result<(), ConnectionError> {
        self.install_cancellable(secret, cancel, |tx, slot| {
            let (provider, old_kind, known): (String, String, Option<String>) = tx
                .query_row(
                    "SELECT provider,kind,identity FROM accounts WHERE id=?1 AND slot=?2",
                    params![id.to_string(), expected.0.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?
                .ok_or(ConnectionError::Changed)?;
            let provider = provider.parse()?;
            let (kind, candidate) = identity.parts(provider)?;
            if kind != old_kind {
                return Err(ConnectionError::CredentialKind);
            }
            if kind == "oauth" && known.is_some() && candidate != known.as_deref() {
                return Err(ConnectionError::DifferentIdentity);
            }
            reject_duplicate(tx, provider, kind, candidate, id)?;
            tx.execute(
                "UPDATE accounts SET identity=?1, fetched_at=NULL, snapshot=NULL WHERE id=?2",
                params![candidate, id.to_string()],
            )?;
            replace_slot(tx, id, expected, slot)?;
            if let Some((at, payload)) = snapshot {
                write_snapshot(tx, id, CredentialVersion(slot), at, payload)?;
            }
            Ok(())
        })
        .map(|_| ())
    }

    pub fn rotate(
        &mut self,
        id: Uuid,
        expected: CredentialVersion,
        secret: &SecretBytes,
    ) -> Result<CredentialVersion, ConnectionError> {
        self.install(secret, |tx, slot| replace_slot(tx, id, expected, slot))
    }

    fn install(
        &mut self,
        secret: &SecretBytes,
        change: impl FnOnce(&Connection, Uuid) -> Result<(), ConnectionError>,
    ) -> Result<CredentialVersion, ConnectionError> {
        self.install_cancellable(secret, &CancellationToken::new(), change)
    }

    fn install_cancellable(
        &mut self,
        secret: &SecretBytes,
        cancel: &CancellationToken,
        change: impl FnOnce(&Connection, Uuid) -> Result<(), ConnectionError>,
    ) -> Result<CredentialVersion, ConnectionError> {
        let _lock = self.mutation_lock()?;
        if cancel.is_cancelled() {
            return Err(ConnectionError::Cancelled);
        }
        let slot = Uuid::new_v4();
        self.db
            .execute("INSERT INTO retired_slots VALUES (?1)", [slot.to_string()])?;
        let result = (|| {
            self.vault.store(slot, secret)?;
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            change(&tx, slot)?;
            if cancel.is_cancelled() {
                return Err(ConnectionError::Cancelled);
            }
            tx.execute(
                "DELETE FROM retired_slots WHERE slot=?1",
                [slot.to_string()],
            )?;
            tx.commit()?;
            Ok(CredentialVersion(slot))
        })();
        let _cleanup = self.drain_retired();
        result
    }

    pub fn rename(&self, id: Uuid, label: &str) -> Result<(), ConnectionError> {
        changed(self.db.execute(
            "UPDATE accounts SET label=?1 WHERE id=?2",
            params![nonempty(label)?, id.to_string()],
        )?)
    }

    pub fn set_preferences(
        &self,
        id: Uuid,
        prefs: &AccountPreferences,
    ) -> Result<(), ConnectionError> {
        let mut normalized = prefs.clone();
        normalized
            .pinned
            .retain(|id| !normalized.hidden.contains(id));
        let mut seen = std::collections::BTreeSet::new();
        normalized.metric_order.retain(|id| seen.insert(id.clone()));
        let json = serde_json::to_string(&normalized).map_err(|_| ConnectionError::Invalid)?;
        changed(self.db.execute(
            "UPDATE accounts SET preferences=?1 WHERE id=?2",
            params![json, id.to_string()],
        )?)
    }

    pub fn reorder(&mut self, ids: &[Uuid]) -> Result<(), ConnectionError> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 = tx.query_row("SELECT count(*) FROM accounts", [], |r| r.get(0))?;
        let count = usize::try_from(count).map_err(|_| ConnectionError::Invalid)?;
        if ids.len() != count
            || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != count
        {
            return Err(ConnectionError::Changed);
        }
        for (i, id) in (0_i64..).zip(ids) {
            changed(tx.execute(
                "UPDATE accounts SET position=?1 WHERE id=?2",
                params![i, id.to_string()],
            )?)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove(&mut self, id: Uuid) -> Result<(), ConnectionError> {
        let _lock = self.mutation_lock()?;
        if self.hub_link(id)?.is_some() {
            return Err(ConnectionError::HubManaged);
        }
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = id.to_string();
        changed(tx.execute(
            "INSERT INTO retired_slots SELECT slot FROM accounts WHERE id=?1",
            [&id],
        )?)?;
        tx.execute("DELETE FROM accounts WHERE id=?1", [&id])?;
        tx.commit()?;
        self.drain_retired()
            .map_err(|_| ConnectionError::RemovedPendingCleanup)
    }

    fn clean_idle_slots(&self) -> Result<(), ConnectionError> {
        if !self
            .db
            .query_row("SELECT EXISTS(SELECT 1 FROM retired_slots)", [], |row| {
                row.get::<_, bool>(0)
            })?
        {
            return Ok(());
        }
        let lock = private::file(&self.directory.join("credentials.lock"))?;
        match lock.try_lock() {
            Ok(()) => self.drain_retired(),
            Err(std::fs::TryLockError::WouldBlock) => Ok(()),
            Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub fn clean_retired(&mut self) -> Result<(), ConnectionError> {
        let _lock = self.mutation_lock()?;
        self.drain_retired()
    }

    fn drain_retired(&self) -> Result<(), ConnectionError> {
        let mut stmt = self.db.prepare("SELECT slot FROM retired_slots")?;
        let slots = stmt
            .query_map([], |r| uuid(r, 0))?
            .collect::<Result<Vec<_>, _>>()?;
        for slot in slots {
            self.vault.delete(slot)?;
            self.db.execute(
                "DELETE FROM retired_slots WHERE slot=?1",
                [slot.to_string()],
            )?;
        }
        Ok(())
    }

    pub fn put_snapshot(
        &self,
        id: Uuid,
        expected: CredentialVersion,
        fetched_at: i64,
        payload: &Value,
    ) -> Result<(), ConnectionError> {
        write_snapshot(&self.db, id, expected, fetched_at, payload)
    }

    pub fn snapshots(&self) -> Result<Vec<StoredSnapshot>, ConnectionError> {
        let mut stmt = self.db.prepare(&format!(
            "SELECT {COLUMNS},fetched_at,snapshot FROM accounts ORDER BY position,id"
        ))?;
        Ok(stmt
            .query_map([], |r| {
                let raw: Option<String> = r.get(6)?;
                let snapshot = raw
                    .map(|s| serde_json::from_str(&s).map_err(|e| bad(6, e)))
                    .transpose()?;
                Ok((account(r)?, r.get(5)?, snapshot))
            })?
            .collect::<Result<_, _>>()?)
    }
}

fn write_snapshot(
    db: &Connection,
    id: Uuid,
    expected: CredentialVersion,
    at: i64,
    payload: &Value,
) -> Result<(), ConnectionError> {
    let count=db.execute("UPDATE accounts SET fetched_at=?1,snapshot=?2 WHERE id=?3 AND slot=?4 AND (fetched_at IS NULL OR fetched_at<=?1)",params![at,payload.to_string(),id.to_string(),expected.0.to_string()])?;
    changed(count)
}

fn replace_slot(
    tx: &Connection,
    id: Uuid,
    expected: CredentialVersion,
    slot: Uuid,
) -> Result<(), ConnectionError> {
    changed(tx.execute(
        "UPDATE accounts SET slot=?1 WHERE id=?2 AND slot=?3",
        params![slot.to_string(), id.to_string(), expected.0.to_string()],
    )?)?;
    tx.execute(
        "INSERT INTO retired_slots VALUES (?1)",
        [expected.0.to_string()],
    )?;
    Ok(())
}

fn reject_duplicate(
    db: &Connection,
    provider: UsageProvider,
    kind: &str,
    identity: Option<&str>,
    except: Uuid,
) -> Result<(), ConnectionError> {
    let found:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE provider=?1 AND kind=?2 AND identity=?3 AND id<>?4)",params![provider.as_str(),kind,identity,except.to_string()],|r|r.get(0))?;
    if found {
        return Err(ConnectionError::Duplicate);
    }
    Ok(())
}

fn nonempty(label: &str) -> Result<&str, ConnectionError> {
    let label = label.trim();
    if label.is_empty() {
        return Err(ConnectionError::EmptyLabel);
    }
    Ok(label)
}

fn changed(n: usize) -> Result<(), ConnectionError> {
    if n == 0 {
        return Err(ConnectionError::Changed);
    }
    Ok(())
}

fn bad(column: usize, e: impl std::error::Error + Send + Sync + 'static) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(e))
}

fn uuid(row: &Row<'_>, column: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&row.get::<_, String>(column)?).map_err(|e| bad(column, e))
}

fn account(row: &Row<'_>) -> rusqlite::Result<ConnectedAccount> {
    Ok(ConnectedAccount {
        id: uuid(row, 0)?,
        provider: row.get::<_, String>(1)?.parse().map_err(|e| bad(1, e))?,
        label: row.get(2)?,
        position: row.get(3)?,
        preferences: serde_json::from_str(&row.get::<_, String>(4)?).map_err(|e| bad(4, e))?,
    })
}
