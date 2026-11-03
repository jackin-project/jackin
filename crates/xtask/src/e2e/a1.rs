// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use jackin_core::{Secret, plain};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::redact::hex;
use thiserror::Error;

pub const A1_LOCK_FILE: &str = "jackin-e2e-a1.lock";
const NO_SOURCE: &str = "A1 has no usable source; write blockers/A1.md";

#[derive(Debug, Error)]
pub enum A1Error {
    #[error("{NO_SOURCE}: {} is missing", .0.display())]
    Missing(PathBuf),
    #[error("{NO_SOURCE}: auth_mode is not chatgpt")]
    NotChatgpt,
    #[error("{NO_SOURCE}: the login file is not readable JSON")]
    Malformed,
    #[error("{NO_SOURCE}: {0} is empty or absent")]
    EmptyField(&'static str),
    #[error("another e2e run holds the A1 lock at {}", .0.display())]
    LockHeld(PathBuf),
    #[error("cannot take the A1 lock at {}: {}", .path.display(), plain(.message))]
    LockIo { path: PathBuf, message: String },
    #[error("cannot write a fixture login at {}: {}", .path.display(), plain(.message))]
    Write { path: PathBuf, message: String },
    #[error("no entropy for the fixture login: {}", plain(.0))]
    Entropy(String),
}

#[derive(Deserialize)]
struct AuthFile {
    auth_mode: Option<String>,
    tokens: Option<Tokens>,
}

#[derive(Deserialize)]
struct Tokens {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    account_id: Option<String>,
    oauth_provenance: Option<Provenance>,
}

#[derive(Deserialize)]
struct Provenance {
    client_id: Option<String>,
}

pub struct A1Login {
    access_token: Secret,
    refresh_token: Secret,
    id_token: Secret,
    client_id: String,
    digest: [u8; 32],
}

impl fmt::Debug for A1Login {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("A1Login")
            .field("client_id", &self.client_id)
            .finish_non_exhaustive()
    }
}

fn need(value: Option<String>, name: &'static str) -> Result<String, A1Error> {
    value
        .filter(|text| !text.is_empty())
        .ok_or(A1Error::EmptyField(name))
}

impl A1Login {
    pub fn read(codex_home: &Path) -> Result<A1Login, A1Error> {
        let path = codex_home.join("auth.json");
        let bytes = fs::read(&path).map_err(|_| A1Error::Missing(path.clone()))?;
        let file: AuthFile = serde_json::from_slice(&bytes).map_err(|_| A1Error::Malformed)?;
        if file.auth_mode.as_deref() != Some("chatgpt") {
            return Err(A1Error::NotChatgpt);
        }
        let tokens = file.tokens.ok_or(A1Error::EmptyField("tokens"))?;
        let access = need(tokens.access_token, "access_token")?;
        let refresh = need(tokens.refresh_token, "refresh_token")?;
        let id = tokens.id_token.unwrap_or_default();
        let client_id = need(
            tokens.oauth_provenance.and_then(|p| p.client_id),
            "oauth_provenance.client_id",
        )?;
        drop(need(tokens.account_id, "account_id")?);
        Ok(A1Login {
            access_token: Secret::new(access),
            refresh_token: Secret::new(refresh),
            id_token: Secret::new(id),
            client_id,
            digest: Sha256::digest(&bytes).into(),
        })
    }

    #[must_use]
    pub fn secrets(&self) -> [&str; 3] {
        [
            self.access_token.expose(),
            self.refresh_token.expose(),
            self.id_token.expose(),
        ]
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
}

#[must_use]
pub fn host_login_state(codex_home: &Path, before: &A1Login) -> &'static str {
    match fs::read(codex_home.join("auth.json")) {
        Ok(bytes) if Sha256::digest(&bytes).as_slice() == before.digest => "unchanged",
        _ => "CHANGED",
    }
}

const URL_SAFE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn random_text(len: usize) -> Result<String, A1Error> {
    let mut bytes = vec![0_u8; len];
    getrandom::fill(&mut bytes).map_err(|error| A1Error::Entropy(error.to_string()))?;
    Ok(bytes
        .iter()
        .map(|byte| char::from(URL_SAFE[usize::from(*byte) % URL_SAFE.len()]))
        .collect())
}

fn random_uuid() -> Result<String, A1Error> {
    let mut b = [0_u8; 16];
    getrandom::fill(&mut b).map_err(|error| A1Error::Entropy(error.to_string()))?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex = hex(&b);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

fn jwt_shaped() -> Result<String, A1Error> {
    Ok(format!(
        "{}.{}.{}",
        random_text(36)?,
        random_text(48)?,
        random_text(43)?
    ))
}

pub fn fixture_login(dir: &Path, client_id: &str) -> Result<(), A1Error> {
    let write = |message: String| A1Error::Write {
        path: dir.to_path_buf(),
        message,
    };
    let document = serde_json::json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "access_token": jwt_shaped()?,
            "refresh_token": random_text(43)?,
            "id_token": jwt_shaped()?,
            "account_id": random_uuid()?,
            "oauth_provenance": { "client_id": client_id },
        },
    });
    fs::create_dir_all(dir).map_err(|e| write(e.to_string()))?;
    let path = dir.join("auth.json");
    let bytes = serde_json::to_vec(&document).map_err(|e| write(e.to_string()))?;
    fs::write(&path, bytes).map_err(|e| write(e.to_string()))?;
    set_private(&path).map_err(|e| write(e.to_string()))
}

fn set_private(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[derive(Debug)]
pub struct A1Lock {
    file: fs::File,
}

impl A1Lock {
    pub fn acquire(dir: &Path) -> Result<A1Lock, A1Error> {
        let path = dir.join(A1_LOCK_FILE);
        let io = |error: std::io::Error| A1Error::LockIo {
            path: path.clone(),
            message: error.to_string(),
        };
        fs::create_dir_all(dir).map_err(io)?;
        let file = fs::File::create(&path).map_err(io)?;
        match file.try_lock() {
            Ok(()) => Ok(A1Lock { file }),
            Err(fs::TryLockError::WouldBlock) => Err(A1Error::LockHeld(path)),
            Err(fs::TryLockError::Error(error)) => Err(io(error)),
        }
    }
}

impl Drop for A1Lock {
    fn drop(&mut self) {
        drop(self.file.unlock());
    }
}
