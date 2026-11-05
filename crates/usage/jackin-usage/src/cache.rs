// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use jackin_core::{
    CredentialSource, FileMode, FsError, HostPath, PollError, PollPlan, hold_lock, lock_path,
    write_atomic_locked,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::deps::UsageDeps;
use crate::model::{LimitWindow, UsageError};
use crate::request::UsageAccount;

const CACHE_FILE: &str = "usage.json";
const LOCK_PLAN: PollPlan = PollPlan {
    interval: Duration::from_millis(20),
    timeout: Duration::from_secs(10),
};
const CACHE_MODE: FileMode = FileMode(0o600);
const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Good {
    pub(crate) fetched_at: i64,
    pub(crate) windows: Vec<LimitWindow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) fingerprint: String,
    pub(crate) good: Option<Good>,
    pub(crate) retry_at: Option<i64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    #[serde(default)]
    entries: BTreeMap<String, Entry>,
}

#[derive(Debug)]
pub(crate) struct Cache {
    path: HostPath,
    file: CacheFile,
    edits: Vec<(String, Option<Entry>)>,
}

impl Cache {
    pub(crate) async fn load(deps: &UsageDeps) -> Result<Cache, UsageError> {
        let path = HostPath::resolve(Path::new(CACHE_FILE), &deps.state_dir)
            .map_err(|error| UsageError::Internal(error.to_string()))?;
        let file = read(deps, &path).await?;
        Ok(Cache {
            path,
            file,
            edits: Vec::new(),
        })
    }

    pub(crate) fn entry(&self, account: &UsageAccount) -> Option<Entry> {
        self.file.entries.get(account.name.as_str()).cloned()
    }

    pub(crate) fn put(&mut self, account: &UsageAccount, entry: Entry) {
        self.edits
            .push((account.name.as_str().to_owned(), Some(entry)));
    }

    pub(crate) fn remove(&mut self, account: &UsageAccount) {
        self.edits.push((account.name.as_str().to_owned(), None));
    }

    pub(crate) async fn save(
        &self,
        deps: &UsageDeps,
        cancel: &CancellationToken,
    ) -> Result<(), UsageError> {
        deps.fs
            .create_private_dir_all(&deps.state_dir)
            .await
            .map_err(|error| store(&self.path, &error))?;
        let lock_path = lock_path(&self.path).map_err(|error| store(&self.path, &error))?;
        let lock = hold_lock(&*deps.fs, &*deps.clock, &lock_path, LOCK_PLAN, cancel)
            .await
            .map_err(|error| match error {
                PollError::Cancelled => UsageError::Cancelled,
                PollError::Timeout { after } => UsageError::Store {
                    path: lock_path.clone(),
                    message: format!("timed out after {after:?} waiting for the cache lock"),
                },
                PollError::Probe(error) => store(&lock_path, &error),
            })?;
        let mut file = read(deps, &self.path).await?;
        for (key, edit) in &self.edits {
            match edit {
                Some(entry) if newer_than(file.entries.get(key), entry) => None,
                Some(entry) => file.entries.insert(key.clone(), entry.clone()),
                None => file.entries.remove(key),
            };
        }
        let bytes =
            serde_json::to_vec(&file).map_err(|error| UsageError::Internal(error.to_string()))?;
        write_atomic_locked(&*deps.fs, &self.path, &bytes, CACHE_MODE, &lock)
            .await
            .map_err(|error| store(&self.path, &error))
    }
}

fn newer_than(stored: Option<&Entry>, edit: &Entry) -> bool {
    let fetched = |entry: &Entry| entry.good.as_ref().map(|good| good.fetched_at);
    stored.is_some_and(|stored| {
        stored.fingerprint == edit.fingerprint && fetched(stored) > fetched(edit)
    })
}

async fn read(deps: &UsageDeps, path: &HostPath) -> Result<CacheFile, UsageError> {
    let bytes = deps
        .fs
        .read(path)
        .await
        .map_err(|error| store(path, &error))?;
    Ok(bytes
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default())
}

fn store(path: &HostPath, error: &FsError) -> UsageError {
    UsageError::Store {
        path: path.clone(),
        message: error.to_string(),
    }
}

pub(crate) fn fingerprint(account: &UsageAccount) -> String {
    let source = match &account.credential {
        CredentialSource::Env { var } => format!("env:{}", var.as_str()),
        CredentialSource::OnePassword { reference, account } => format!(
            "op:{}\0{}",
            account.as_ref().map_or("", |account| account.as_str()),
            reference.as_str()
        ),
        CredentialSource::File { path } => format!("file:{}", path.as_path().display()),
        CredentialSource::CodexLogin {
            home,
            chatgpt_account_id,
        } => format!(
            "codex:{}:{}",
            home.as_path().display(),
            chatgpt_account_id.as_str()
        ),
    };
    let digest = Sha256::digest(format!("{}\0{source}", account.provider.as_str()).as_bytes());
    digest
        .iter()
        .flat_map(|byte| [HEX[usize::from(byte >> 4)], HEX[usize::from(byte & 15)]])
        .map(char::from)
        .collect()
}
