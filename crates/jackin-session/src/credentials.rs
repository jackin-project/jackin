// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jackin_core::{
    Account, ChatgptAccountId, CredentialFileProblem, CredentialSource, FileSystem, FsError,
    HostPath, ProcessError, ProcessSpec, ProcessStatus, ProcessStdin, Program, Secret, SecretBytes,
    plain, read_credential_file,
};
use tokio_util::sync::CancellationToken;

use crate::service::HostPorts;
use serde_json::Value;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

const AUTH_FILE: &str = "auth.json";
const CHATGPT_MODE: &str = "chatgpt";
const CLAUDE_CREDENTIALS_FILE: &str = ".credentials.json";
pub const CLAUDE_KEYCHAIN_ARGS: [&str; 4] = [
    "find-generic-password",
    "-s",
    "Claude Code-credentials",
    "-w",
];
pub const ANTIGRAVITY_KEYCHAIN_ARGS: [&str; 6] = [
    "find-generic-password",
    "-s",
    "gemini",
    "-a",
    "antigravity",
    "-w",
];

#[derive(Debug)]
pub struct ChatgptLogin {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub client_id: String,
    pub account_id: ChatgptAccountId,
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("no credential is available for the account")]
    MissingStaticKey,
    #[error("cannot read the login at {path}")]
    LoginUnreadable { path: HostPath },
    #[error("the login at {path} is not a regular file")]
    LoginNotRegular { path: HostPath },
    #[error("the login at {path} is larger than the credential file limit")]
    LoginTooLarge { path: HostPath },
    #[error("auth.json is invalid at {}", plain(.key))]
    AuthJsonInvalid { key: String },
    #[error("auth.json is not a ChatGPT login")]
    NotAChatgptLogin,
    #[error("the host credential store is unavailable")]
    KeychainUnavailable,
    #[error("the host credential store has no usable login")]
    KeychainLoginMissing,
    #[error(transparent)]
    Fs(#[from] FsError),
}

pub(crate) struct Document(pub(crate) Value);

impl Drop for Document {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

fn wipe(value: &mut Value) {
    match value {
        Value::String(text) => text.zeroize(),
        Value::Array(items) => items.iter_mut().for_each(wipe),
        Value::Object(fields) => fields.values_mut().for_each(wipe),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn invalid(key: &str) -> CredentialError {
    CredentialError::AuthJsonInvalid {
        key: key.to_owned(),
    }
}

fn field<'a>(parent: &'a Value, name: &str, key: &str) -> Result<&'a Value, CredentialError> {
    parent.get(name).ok_or_else(|| invalid(key))
}

fn text<'a>(parent: &'a Value, name: &str, key: &str) -> Result<&'a str, CredentialError> {
    field(parent, name, key)?
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid(key))
}

pub fn extract_omp_api_key(db_path: &Path, provider_id: &str) -> Option<Secret> {
    let connection =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?;
    let data: String = connection
        .query_row(
            "SELECT data FROM auth_credentials WHERE provider = ?1 AND credential_type = 'api_key' ORDER BY id DESC LIMIT 1",
            [provider_id],
            |row| row.get(0),
        )
        .ok()?;
    let document = Document(serde_json::from_str(&data).ok()?);
    let value = document.0.get("key")?.as_str()?;
    if value.is_empty() {
        None
    } else {
        Some(Secret::new(value.to_owned()))
    }
}

pub fn read_omp_host_key(
    _fs: &dyn FileSystem,
    db_path: &HostPath,
    provider_id: &str,
) -> Result<Secret, CredentialError> {
    extract_omp_api_key(db_path.as_path(), provider_id).ok_or_else(|| {
        CredentialError::LoginUnreadable {
            path: db_path.clone(),
        }
    })
}

#[derive(Debug)]
pub struct GrokLogin {
    pub session_key: Secret,
    pub issuer: String,
}

#[derive(Debug)]
pub struct MuseHostMetadata {
    pub subs_active: bool,
    pub storage: String,
}

async fn read_host_auth(
    fs: &dyn FileSystem,
    path: &HostPath,
    file: &str,
) -> Result<Document, CredentialError> {
    let bytes = match read_credential_file(fs, path).await? {
        Ok(bytes) => Zeroizing::new(bytes),
        Err(CredentialFileProblem::Missing) => {
            return Err(CredentialError::LoginUnreadable { path: path.clone() });
        }
        Err(CredentialFileProblem::NotARegularFile) => {
            return Err(CredentialError::LoginNotRegular { path: path.clone() });
        }
        Err(CredentialFileProblem::TooLarge) => {
            return Err(CredentialError::LoginTooLarge { path: path.clone() });
        }
    };
    Ok(Document(
        serde_json::from_slice(&bytes).map_err(|_| invalid(file))?,
    ))
}

pub async fn read_grok_login(
    fs: &dyn FileSystem,
    path: &HostPath,
) -> Result<GrokLogin, CredentialError> {
    let document = read_host_auth(fs, path, AUTH_FILE).await?;
    let root = document.0.as_object().ok_or_else(|| invalid(AUTH_FILE))?;
    let mut entries = root.iter();
    let (issuer, entry) = entries.next().ok_or_else(|| invalid(AUTH_FILE))?;
    if entries.next().is_some() || issuer.is_empty() {
        return Err(invalid(AUTH_FILE));
    }
    let key = entry
        .get("key")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("key"))?;
    Ok(GrokLogin {
        session_key: Secret::new(key.to_owned()),
        issuer: issuer.clone(),
    })
}

pub async fn read_muse_host_metadata(
    fs: &dyn FileSystem,
    path: &HostPath,
) -> Result<MuseHostMetadata, CredentialError> {
    let document = read_host_auth(fs, path, AUTH_FILE).await?;
    let providers = field(&document.0, "providers", "providers")?;
    let meta = field(providers, "meta", "providers.meta")?;
    let subs_active = field(meta, "subs_active", "providers.meta.subs_active")?
        .as_bool()
        .ok_or_else(|| invalid("providers.meta.subs_active"))?;
    let storage = text(meta, "storage", "providers.meta.storage")?;
    Ok(MuseHostMetadata {
        subs_active,
        storage: storage.to_owned(),
    })
}

pub fn parse_muse_keychain_secret(stdout: &SecretBytes) -> Option<Secret> {
    let document = Document(serde_json::from_slice(stdout.expose()).ok()?);
    let access_token = document
        .0
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())?;
    Some(Secret::new(access_token.to_owned()))
}

#[derive(Debug)]
pub struct ClaudeLogin {
    pub access_token: Secret,
}

pub async fn read_claude_login(
    fs: &dyn FileSystem,
    path: &HostPath,
) -> Result<ClaudeLogin, CredentialError> {
    let document = read_host_auth(fs, path, CLAUDE_CREDENTIALS_FILE).await?;
    let oauth = field(&document.0, "claudeAiOauth", "claudeAiOauth")?;
    let access_token = text(oauth, "accessToken", "claudeAiOauth.accessToken")?;
    Ok(ClaudeLogin {
        access_token: Secret::new(access_token.to_owned()),
    })
}

pub fn parse_claude_keychain_secret(stdout: &SecretBytes) -> Option<Secret> {
    let document = Document(serde_json::from_slice(stdout.expose()).ok()?);
    let access_token = document
        .0
        .get("claudeAiOauth")?
        .get("accessToken")?
        .as_str()
        .filter(|value| !value.trim().is_empty())?;
    Some(Secret::new(access_token.to_owned()))
}

pub fn parse_antigravity_keychain_secret(stdout: &SecretBytes) -> Option<Secret> {
    let value = std::str::from_utf8(stdout.expose()).ok()?.trim();
    let payload = value.split_once(':')?.1;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let document = Document(serde_json::from_slice(&bytes).ok()?);
    let access_token = document
        .0
        .get("token")?
        .get("access_token")?
        .as_str()
        .filter(|value| !value.trim().is_empty())?;
    Some(Secret::new(access_token.to_owned()))
}

async fn keychain_secret(
    host: &HostPorts,
    cancel: &CancellationToken,
    args: &[&str],
) -> Result<SecretBytes, CredentialError> {
    let spec = ProcessSpec {
        program: Program::Security,
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: Vec::new(),
        stdin: ProcessStdin::Null,
        timeout: Duration::from_secs(30),
    };
    let output = host
        .runner
        .run_secret(&spec, cancel)
        .await
        .map_err(|error| match error {
            ProcessError::Spawn { .. } => CredentialError::KeychainUnavailable,
            _ => CredentialError::KeychainLoginMissing,
        })?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(CredentialError::KeychainLoginMissing);
    }
    Ok(output.stdout)
}

pub async fn read_claude_host_login(
    host: &HostPorts,
    path: &HostPath,
    cancel: &CancellationToken,
) -> Result<Secret, CredentialError> {
    if let Ok(login) = read_claude_login(host.fs.as_ref(), path).await {
        return Ok(login.access_token);
    }
    let stdout = keychain_secret(host, cancel, &CLAUDE_KEYCHAIN_ARGS).await?;
    parse_claude_keychain_secret(&stdout).ok_or(CredentialError::KeychainLoginMissing)
}

pub async fn read_antigravity_host_login(
    host: &HostPorts,
    cancel: &CancellationToken,
) -> Result<Secret, CredentialError> {
    let stdout = keychain_secret(host, cancel, &ANTIGRAVITY_KEYCHAIN_ARGS).await?;
    parse_antigravity_keychain_secret(&stdout).ok_or(CredentialError::KeychainLoginMissing)
}

pub async fn read_chatgpt_login(
    fs: &dyn FileSystem,
    home: &HostPath,
) -> Result<ChatgptLogin, CredentialError> {
    let path = HostPath::resolve(Path::new(AUTH_FILE), home)
        .map_err(|_| CredentialError::LoginUnreadable { path: home.clone() })?;
    let bytes = match read_credential_file(fs, &path).await? {
        Ok(bytes) => Zeroizing::new(bytes),
        Err(CredentialFileProblem::Missing) => {
            return Err(CredentialError::LoginUnreadable { path });
        }
        Err(CredentialFileProblem::NotARegularFile) => {
            return Err(CredentialError::LoginNotRegular { path });
        }
        Err(CredentialFileProblem::TooLarge) => {
            return Err(CredentialError::LoginTooLarge { path });
        }
    };
    let document = Document(serde_json::from_slice(&bytes).map_err(|_| invalid(AUTH_FILE))?);
    let root = &document.0;
    if text(root, "auth_mode", "auth_mode")? != CHATGPT_MODE {
        return Err(CredentialError::NotAChatgptLogin);
    }
    let tokens = field(root, "tokens", "tokens")?;
    if !tokens.is_object() {
        return Err(invalid("tokens"));
    }
    let access_token = text(tokens, "access_token", "tokens.access_token")?;
    let refresh_token = text(tokens, "refresh_token", "tokens.refresh_token")?;
    let provenance = field(
        tokens,
        "oauth_provenance",
        "tokens.oauth_provenance.client_id",
    )?;
    let client_id = text(provenance, "client_id", "tokens.oauth_provenance.client_id")?;
    let account_id = ChatgptAccountId::parse(text(tokens, "account_id", "tokens.account_id")?)
        .map_err(|_| invalid("tokens.account_id"))?;
    Ok(ChatgptLogin {
        access_token: Secret::new(access_token.to_owned()),
        refresh_token: Secret::new(refresh_token.to_owned()),
        client_id: client_id.to_owned(),
        account_id,
    })
}

pub async fn resolve_static_key(
    host: &HostPorts,
    account: &Account,
    provider_id: &str,
    cancel: &CancellationToken,
) -> Result<Secret, CredentialError> {
    let missing = || CredentialError::MissingStaticKey;
    match account.credential() {
        CredentialSource::Env { var } => std::env::var(var.as_str())
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(Secret::new)
            .ok_or_else(missing),
        CredentialSource::File { path } => {
            let Ok(bytes) = read_credential_file(host.fs.as_ref(), path).await? else {
                return Err(missing());
            };
            let text = String::from_utf8(bytes).map_err(|_| missing())?;
            let value = text.trim().to_owned();
            if value.is_empty() {
                Err(missing())
            } else {
                Ok(Secret::new(value))
            }
        }
        CredentialSource::OmpHost => {
            let home = std::env::var("HOME").map_err(|_| missing())?;
            let db = HostPath::parse(Path::new(&format!("{home}/.omp/agent/agent.db")))
                .map_err(|_| missing())?;
            read_omp_host_key(host.fs.as_ref(), &db, provider_id).map_err(|_| missing())
        }
        CredentialSource::OnePassword {
            reference,
            account: op_account,
        } => {
            let mut args = vec!["read".to_owned()];
            if let Some(op_account) = op_account {
                args.extend(["--account".to_owned(), op_account.as_str().to_owned()]);
            }
            args.push(reference.as_str().to_owned());
            let spec = ProcessSpec {
                program: Program::Op,
                args,
                cwd: None,
                env: Vec::new(),
                env_remove_prefixes: Vec::new(),
                stdin: ProcessStdin::Null,
                timeout: Duration::from_secs(30),
            };
            let output = host
                .runner
                .run_secret(&spec, cancel)
                .await
                .map_err(|_| missing())?;
            if output.status != ProcessStatus::Exited(0) {
                return Err(missing());
            }
            let text = std::str::from_utf8(output.stdout.expose()).map_err(|_| missing())?;
            let value = text.trim().to_owned();
            if value.is_empty() {
                Err(missing())
            } else {
                Ok(Secret::new(value))
            }
        }
        CredentialSource::CodexLogin { .. } | CredentialSource::HostLogin { .. } => Err(missing()),
    }
}
