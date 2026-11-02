// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::Path;

use jackin_core::{CredentialSource, FileMode, HostPath, write_atomic};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::deps::UsageDeps;
use crate::model::{LimitWindow, UsageError};
use crate::request::UsageAccount;

const CACHE_FILE: &str = "usage.json";
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
}

impl Cache {
    pub(crate) async fn load(deps: &UsageDeps) -> Result<Cache, UsageError> {
        let path = HostPath::resolve(Path::new(CACHE_FILE), &deps.state_dir)
            .map_err(|error| UsageError::Internal(error.to_string()))?;
        let bytes = deps
            .fs
            .read(&path)
            .await
            .map_err(|error| store(&path, &error))?;
        let file = bytes
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Ok(Cache { path, file })
    }

    pub(crate) fn entry(&self, account: &UsageAccount) -> Option<Entry> {
        self.file.entries.get(account.name.as_str()).cloned()
    }

    pub(crate) fn put(&mut self, account: &UsageAccount, entry: Entry) {
        let key = account.name.as_str().to_owned();
        self.file.entries.insert(key, entry);
    }

    pub(crate) fn remove(&mut self, account: &UsageAccount) {
        self.file.entries.remove(account.name.as_str());
    }

    pub(crate) async fn save(&self, deps: &UsageDeps) -> Result<(), UsageError> {
        let bytes = serde_json::to_vec(&self.file)
            .map_err(|error| UsageError::Internal(error.to_string()))?;
        deps.fs
            .create_private_dir_all(&deps.state_dir)
            .await
            .map_err(|error| store(&self.path, &error))?;
        write_atomic(&*deps.fs, &self.path, &bytes, CACHE_MODE)
            .await
            .map_err(|error| store(&self.path, &error))
    }
}

fn store(path: &HostPath, error: &jackin_core::FsError) -> UsageError {
    UsageError::Store {
        path: path.clone(),
        message: error.to_string(),
    }
}

pub(crate) fn fingerprint(account: &UsageAccount) -> String {
    let source = match &account.credential {
        CredentialSource::Env { var } => format!("env:{}", var.as_str()),
        CredentialSource::OnePassword { reference } => format!("op:{}", reference.as_str()),
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
