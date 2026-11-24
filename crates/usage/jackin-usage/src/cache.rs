// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use jackin_core::{
    CredentialSource, FileMode, FsError, HostPath, PollError, PollPlan, RegularFileProblem,
    RepoPath, hold_lock, lock_path, write_atomic_locked,
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
const CACHE_MAX_BYTES: usize = 1024 * 1024;
const HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Good {
    pub(crate) fetched_at: i64,
    pub(crate) windows: Vec<LimitWindow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Hold {
    pub(crate) observed_at: i64,
    pub(crate) until: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) fingerprint: String,
    pub(crate) good: Option<Good>,
    pub(crate) hold: Hold,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cleared_at: Option<i64>,
}

impl Entry {
    pub(crate) fn cleared(fingerprint: String, at: i64) -> Entry {
        Entry {
            fingerprint,
            good: None,
            hold: Hold {
                observed_at: at,
                until: None,
            },
            cleared_at: Some(at),
        }
    }

    fn not_after(mut self, now: i64) -> Entry {
        self.good = self.good.filter(|good| good.fetched_at <= now);
        self.hold.observed_at = self.hold.observed_at.min(now);
        self.cleared_at = self.cleared_at.map(|at| at.min(now));
        self
    }

    fn merged(&self, edit: &Entry) -> Entry {
        let mut good = match (&self.good, &edit.good) {
            (Some(stored), Some(edited)) if stored.fetched_at > edited.fetched_at => {
                Some(stored.clone())
            }
            (Some(stored), None) => Some(stored.clone()),
            (_, edited) => edited.clone(),
        };
        let mut hold = if self.hold.observed_at > edit.hold.observed_at {
            self.hold
        } else {
            edit.hold
        };
        let cleared_at = self.cleared_at.max(edit.cleared_at);
        if let Some(at) = cleared_at {
            good = good.filter(|good| good.fetched_at > at);
            if hold.observed_at <= at {
                hold = Hold {
                    observed_at: at,
                    until: None,
                };
            }
        }
        Entry {
            fingerprint: edit.fingerprint.clone(),
            good,
            hold,
            cleared_at,
        }
    }
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
    edits: Vec<(String, Entry)>,
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
        self.edits.push((account.name.as_str().to_owned(), entry));
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
            apply(&mut file.entries, key, edit);
        }
        let bytes =
            serde_json::to_vec(&file).map_err(|error| UsageError::Internal(error.to_string()))?;
        write_atomic_locked(&*deps.fs, &self.path, &bytes, CACHE_MODE, &lock)
            .await
            .map_err(|error| store(&self.path, &error))
    }
}

fn apply(entries: &mut BTreeMap<String, Entry>, key: &str, edit: &Entry) {
    let next = match entries.get(key) {
        Some(stored) if stored.fingerprint == edit.fingerprint => stored.merged(edit),
        Some(stored) if stored.hold.observed_at > edit.hold.observed_at => return,
        _ => edit.clone(),
    };
    entries.insert(key.to_owned(), next);
}

async fn read(deps: &UsageDeps, path: &HostPath) -> Result<CacheFile, UsageError> {
    let refuse = |message: &str| UsageError::Store {
        path: path.clone(),
        message: message.to_owned(),
    };
    let name =
        RepoPath::parse(CACHE_FILE).map_err(|error| UsageError::Internal(error.to_string()))?;
    let bytes = match deps
        .fs
        .read_regular(&deps.state_dir, &name, CACHE_MAX_BYTES)
        .await
    {
        Ok(Ok(file)) if file.bytes.len() <= CACHE_MAX_BYTES => Some(file.bytes),
        Ok(Ok(_) | Err(RegularFileProblem::Missing)) => None,
        Ok(Err(RegularFileProblem::Symlink | RegularFileProblem::NotARegularFile)) => {
            return Err(refuse("the cache is not a regular file"));
        }
        Err(FsError::Io {
            kind: ErrorKind::NotFound,
            ..
        }) => None,
        Err(error) => return Err(store(path, &error)),
    };
    let now = deps.clock.now().0;
    let mut file: CacheFile = bytes
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    file.entries = file
        .entries
        .into_iter()
        .map(|(key, entry)| (key, entry.not_after(now)))
        .collect();
    Ok(file)
}

fn store(path: &HostPath, error: &FsError) -> UsageError {
    UsageError::Store {
        path: path.clone(),
        message: error.to_string(),
    }
}

pub(crate) fn fingerprint(
    account: &UsageAccount,
    identity: Option<&str>,
    revision: Option<&str>,
) -> String {
    let source = match &account.credential {
        CredentialSource::Env { var } => format!("env:{}", var.as_str()),
        CredentialSource::OnePassword { reference, account } => format!(
            "op:{}\0{}",
            account.as_ref().map_or("", |account| account.as_str()),
            reference.as_str()
        ),
        CredentialSource::OmpHost => "omp-host".to_owned(),
        CredentialSource::HostLogin { path } => format!("host-login:{}", path.as_path().display()),
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
    let mut text = format!("{}\0{source}", account.provider.as_str());
    if let Some(identity) = identity {
        text.push_str("\0identity:");
        text.push_str(identity);
    }
    if let Some(revision) = revision {
        text.push_str("\0revision:");
        text.push_str(revision);
    }
    let digest = Sha256::digest(text.as_bytes());
    digest
        .iter()
        .flat_map(|byte| [HEX[usize::from(byte >> 4)], HEX[usize::from(byte & 15)]])
        .map(char::from)
        .collect()
}
