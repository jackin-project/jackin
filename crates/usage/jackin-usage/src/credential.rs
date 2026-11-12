// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::Duration;

use jackin_core::{
    Agent, CredentialFileProblem, CredentialSource, FileIdentity, HostPath, ProcessSpec,
    ProcessStatus, ProcessStdin, Program, Secret, lookup_tuple, read_credential_file,
};
use jackin_usage_provider_core::Credential;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::deps::UsageDeps;
use crate::request::UsageAccount;

const OP_TIMEOUT: Duration = Duration::from_secs(30);
const OMP_RESOLVE_ATTEMPTS: usize = 4;
const CODEX_LOGIN_FILE: &str = "auth.json";
const OMP_DB: &str = jackin_core::OMP_HOST_STORE;
const KIMI_CONFIG: &str = "config.toml";
const KIMI_GLOBAL_BASE: &str = "https://api.kimi.ai/coding/v1";
const KIMI_MAINLAND_BASE: &str = "https://api.kimi.com/coding/v1";
const HEX: [char; 16] = [
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Token,
    Login,
}

pub(crate) struct Material {
    pub(crate) secret: Secret,
    pub(crate) shape: Shape,
    pub(crate) base: Option<String>,
    pub(crate) identity: Option<String>,
    pub(crate) revision: Option<String>,
}

impl Material {
    fn token(text: &str) -> Material {
        Material {
            secret: Secret::new(text.to_owned()),
            shape: Shape::Token,
            base: None,
            identity: None,
            revision: None,
        }
    }

    pub(crate) fn credential(&self) -> Credential<'_> {
        match self.shape {
            Shape::Token => Credential::Token(self.secret.expose()),
            Shape::Login => Credential::Login(self.secret.expose()),
        }
    }

    fn login(text: String) -> Material {
        Material {
            secret: Secret::new(text),
            shape: Shape::Login,
            base: None,
            identity: None,
            revision: None,
        }
    }

    fn based(mut self, base: Option<String>) -> Material {
        self.base = base;
        self
    }

    fn with_identity(mut self, identity: Option<String>) -> Material {
        self.identity = identity;
        self
    }

    fn with_revision(mut self, revision: Option<String>) -> Material {
        self.revision = revision;
        self
    }

    pub(crate) fn established(&self) -> bool {
        self.identity.is_some() || self.revision.is_some()
    }
}

pub(crate) enum Unresolved {
    Missing,
    Unreadable(&'static str),
}

pub(crate) async fn resolve(
    deps: &UsageDeps,
    account: &UsageAccount,
    cancel: &CancellationToken,
) -> Result<Material, Unresolved> {
    match &account.credential {
        CredentialSource::Env { var } => {
            let value = deps
                .env
                .get(var)
                .and_then(non_empty)
                .ok_or(Unresolved::Missing)?;
            Ok(Material::token(value).with_revision(Some(revision_of(value.as_bytes()))))
        }
        CredentialSource::OnePassword { reference, account } => {
            let mut args = vec!["read".to_owned()];
            if let Some(account) = account {
                args.extend(["--account".to_owned(), account.as_str().to_owned()]);
            }
            args.push(reference.as_str().to_owned());
            let spec = ProcessSpec {
                program: Program::Op,
                args,
                cwd: None,
                env: Vec::new(),
                env_remove_prefixes: Vec::new(),
                stdin: ProcessStdin::Null,
                timeout: OP_TIMEOUT,
            };
            let output = deps
                .runner
                .run_secret(&spec, cancel)
                .await
                .map_err(|_| Unresolved::Missing)?;
            if output.status != ProcessStatus::Exited(0) {
                return Err(Unresolved::Missing);
            }
            let text = std::str::from_utf8(output.stdout.expose())
                .map_err(|_| Unresolved::Unreadable("the 1Password value is not text"))?;
            let value = non_empty(text).ok_or(Unresolved::Missing)?;
            Ok(Material::token(value).with_revision(Some(revision_of(value.as_bytes()))))
        }
        CredentialSource::OmpHost => {
            let (revision, key) = omp_key(deps, account).await?;
            Ok(Material::token(key.expose()).with_revision(Some(revision)))
        }
        CredentialSource::File { path } => {
            if account.provider == jackin_core::Provider::Opencode {
                return opencode_store(deps, path).await;
            }
            let (text, revision) = read_credential(deps, path).await?;
            Ok(file_login(account, path, text).with_revision(Some(revision)))
        }
        CredentialSource::CodexLogin { home, .. } => {
            let path = HostPath::resolve(Path::new(CODEX_LOGIN_FILE), home)
                .map_err(|_| Unresolved::Unreadable("the Codex login file name is not usable"))?;
            let (text, revision) = read_credential(deps, &path).await?;
            let identity = serde_json::from_str::<Value>(&text).ok().and_then(|value| {
                value
                    .pointer("/tokens/account_id")
                    .and_then(Value::as_str)
                    .and_then(non_empty)
                    .map(str::to_owned)
            });
            Ok(Material::login(text)
                .with_identity(identity)
                .with_revision(Some(revision)))
        }
        CredentialSource::HostLogin { path } => host_login(deps, account, path, cancel).await,
    }
}

fn file_login(account: &UsageAccount, path: &HostPath, text: String) -> Material {
    if text.starts_with('{') {
        let base = match account.provider {
            jackin_core::Provider::Kimi => path
                .as_path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(kimi_base_for_slot),
            _ => None,
        };
        Material::login(text).based(base)
    } else {
        Material::token(&text)
    }
}

fn kimi_base_for_slot(stem: &str) -> Option<String> {
    if stem == "kimi-code" {
        Some(KIMI_MAINLAND_BASE.to_owned())
    } else if stem.starts_with("kimi-code-env-") {
        Some(KIMI_GLOBAL_BASE.to_owned())
    } else {
        None
    }
}

async fn host_login(
    deps: &UsageDeps,
    account: &UsageAccount,
    path: &HostPath,
    cancel: &CancellationToken,
) -> Result<Material, Unresolved> {
    match account.provider {
        jackin_core::Provider::Kimi => {
            let (text, base, revision) = kimi_host_login(deps, path).await?;
            Ok(Material::login(text)
                .based(Some(base))
                .with_revision(Some(revision)))
        }
        jackin_core::Provider::Minimax => {
            let (text, revision) = minimax_host_login(deps, path).await?;
            Ok(Material::login(text).with_revision(Some(revision)))
        }
        jackin_core::Provider::Grok | jackin_core::Provider::Claude => {
            let (text, revision) = read_credential(deps, path).await?;
            Ok(Material::login(text).with_revision(Some(revision)))
        }
        _ => {
            let _ = cancel;
            Err(Unresolved::Unreadable(
                "this provider has no host login store",
            ))
        }
    }
}

async fn kimi_host_login(
    deps: &UsageDeps,
    home: &HostPath,
) -> Result<(String, String, String), Unresolved> {
    let config_path = HostPath::resolve(Path::new(KIMI_CONFIG), home)
        .map_err(|_| Unresolved::Unreadable("the Kimi config path is not usable"))?;
    let (text, _) = read_credential(deps, &config_path).await?;
    let config: toml::Value = toml::from_str(&text)
        .map_err(|_| Unresolved::Unreadable("the Kimi config is not valid TOML"))?;
    let managed = config
        .get("providers")
        .and_then(toml::Value::as_table)
        .and_then(|providers| providers.get("managed:kimi-code"))
        .and_then(toml::Value::as_table)
        .ok_or(Unresolved::Unreadable(
            "the Kimi config has no managed:kimi-code provider",
        ))?;
    if managed.get("type").and_then(toml::Value::as_str) != Some("kimi") {
        return Err(Unresolved::Unreadable(
            "the Kimi config provider type is not kimi",
        ));
    }
    let base = managed
        .get("base_url")
        .and_then(toml::Value::as_str)
        .map(|value| value.trim_end_matches('/'))
        .filter(|value| !value.is_empty())
        .ok_or(Unresolved::Unreadable("the Kimi config has no coding base"))?;
    let oauth = managed
        .get("oauth")
        .and_then(toml::Value::as_table)
        .ok_or(Unresolved::Unreadable("the Kimi config has no oauth store"))?;
    if oauth.get("storage").and_then(toml::Value::as_str) != Some("file") {
        return Err(Unresolved::Unreadable(
            "the Kimi oauth store is not a file store",
        ));
    }
    let slot = oauth
        .get("key")
        .and_then(toml::Value::as_str)
        .and_then(|key| key.strip_prefix("oauth/"))
        .filter(|slot| !slot.is_empty())
        .ok_or(Unresolved::Unreadable(
            "the Kimi config has no usable oauth key",
        ))?;
    let token_path = HostPath::resolve(Path::new(&format!("credentials/{slot}.json")), home)
        .map_err(|_| Unresolved::Unreadable("the Kimi credential path is not usable"))?;
    let (token, revision) = read_credential(deps, &token_path).await?;
    Ok((token, base.to_owned(), revision))
}

async fn minimax_host_login(
    deps: &UsageDeps,
    root: &HostPath,
) -> Result<(String, String), Unresolved> {
    let mut found: Vec<(String, String)> = Vec::new();
    for region in ["en", "cn"] {
        let path = HostPath::resolve(
            Path::new(&format!("auth/prod/{region}/mcode-public/auth.json")),
            root,
        )
        .map_err(|_| Unresolved::Unreadable("the MiniMax credential path is not usable"))?;
        if deps
            .fs
            .canonicalize(&path)
            .await
            .map_err(|_| Unresolved::Missing)?
            .is_some()
        {
            found.push(read_credential(deps, &path).await?);
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err(Unresolved::Missing),
        _ => Err(Unresolved::Unreadable(
            "the MiniMax store holds more than one login",
        )),
    }
}

async fn opencode_store(deps: &UsageDeps, path: &HostPath) -> Result<Material, Unresolved> {
    let identity = deps
        .fs
        .identity(path)
        .await
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?
        .ok_or(Unresolved::Missing)?;
    let found = extract_opencode_credential(path.as_path())?;
    let revision = opencode_revision(path.as_path(), identity, &found);
    Ok(Material::login(found.value).with_revision(Some(revision)))
}

struct OpenCodeRecord {
    id: i64,
    value: String,
    ctime: i64,
    ctime_nsec: i64,
    digest: String,
}

fn extract_opencode_credential(db_path: &Path) -> Result<OpenCodeRecord, Unresolved> {
    let metadata = match std::fs::metadata(db_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Unresolved::Missing);
        }
        Err(_) => {
            return Err(Unresolved::Unreadable(
                "the OpenCode store could not be read",
            ));
        }
        Ok(metadata) => metadata,
    };
    let connection =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be opened"))?;
    let (id, value): (i64, String) = connection
        .query_row(
            "SELECT id, value FROM credential WHERE integration_id = 'opencode' AND type = 'oauth' AND active = 1 ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Unresolved::Missing,
            _ => Unresolved::Unreadable("the OpenCode store could not be read"),
        })?;
    Ok(OpenCodeRecord {
        id,
        digest: revision_of(value.as_bytes()),
        value,
        ctime: metadata.ctime(),
        ctime_nsec: metadata.ctime_nsec(),
    })
}

fn opencode_revision(db: &Path, identity: FileIdentity, found: &OpenCodeRecord) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}:{}",
        db.display(),
        identity.dev,
        identity.ino,
        found.id,
        found.ctime,
        found.ctime_nsec,
        found.digest
    )
}

async fn omp_key(deps: &UsageDeps, account: &UsageAccount) -> Result<(String, Secret), Unresolved> {
    let provider_id = lookup_tuple(Agent::Omp, account.provider)
        .ok()
        .and_then(jackin_core::AgentTuple::omp_provider_id)
        .ok_or(Unresolved::Unreadable(
            "the OMP store names no key for this provider",
        ))?;
    let db = HostPath::resolve(Path::new(OMP_DB), &deps.home)
        .map_err(|_| Unresolved::Unreadable("the OMP database path is not usable"))?;
    for _ in 0..OMP_RESOLVE_ATTEMPTS {
        let identity = omp_identity(deps, &db).await?;
        let found = extract_omp_api_key(db.as_path(), provider_id)?;
        let confirmed = match (identity, omp_identity(deps, &db).await?) {
            (Some(before), Some(after)) if before == after => before,
            _ => continue,
        };
        return Ok((
            omp_revision(db.as_path(), confirmed.dev, confirmed.ino, &found),
            found.key,
        ));
    }
    Err(Unresolved::Unreadable("the OMP store could not be read"))
}

async fn omp_identity(deps: &UsageDeps, db: &HostPath) -> Result<Option<FileIdentity>, Unresolved> {
    deps.fs
        .identity(db)
        .await
        .map_err(|_| Unresolved::Unreadable("the OMP store could not be read"))
}

struct Extracted {
    id: i64,
    key: Secret,
    ctime: i64,
    ctime_nsec: i64,
    digest: String,
}

fn omp_revision(db: &Path, dev: u64, ino: u64, found: &Extracted) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}:{}",
        db.display(),
        dev,
        ino,
        found.id,
        found.ctime,
        found.ctime_nsec,
        found.digest
    )
}

fn extract_omp_api_key(db_path: &Path, provider_id: &str) -> Result<Extracted, Unresolved> {
    let metadata = match std::fs::metadata(db_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Unresolved::Missing);
        }
        Err(_) => {
            return Err(Unresolved::Unreadable("the OMP store could not be read"));
        }
        Ok(metadata) => metadata,
    };
    let connection =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| Unresolved::Unreadable("the OMP store could not be opened"))?;
    let (id, data): (i64, String) = connection
        .query_row(
            "SELECT id, data FROM auth_credentials WHERE provider = ?1 AND credential_type = 'api_key' ORDER BY id DESC LIMIT 1",
            [provider_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => Unresolved::Missing,
            _ => Unresolved::Unreadable("the OMP store could not be read"),
        })?;
    let document: Value = serde_json::from_str(&data)
        .map_err(|_| Unresolved::Unreadable("the OMP credential is not valid JSON"))?;
    let value = document
        .get("key")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .ok_or(Unresolved::Unreadable("the OMP credential holds no key"))?;
    Ok(Extracted {
        id,
        key: Secret::new(value.to_owned()),
        ctime: metadata.ctime(),
        ctime_nsec: metadata.ctime_nsec(),
        digest: revision_of(data.as_bytes()),
    })
}

async fn read_credential(
    deps: &UsageDeps,
    path: &HostPath,
) -> Result<(String, String), Unresolved> {
    let bytes = match read_credential_file(deps.fs.as_ref(), path).await {
        Err(_) => {
            return Err(Unresolved::Unreadable(
                "the credential file could not be read",
            ));
        }
        Ok(Err(CredentialFileProblem::Missing)) => return Err(Unresolved::Missing),
        Ok(Err(CredentialFileProblem::NotARegularFile)) => {
            return Err(Unresolved::Unreadable(
                "the credential file is not a regular file",
            ));
        }
        Ok(Err(CredentialFileProblem::TooLarge)) => {
            return Err(Unresolved::Unreadable("the credential file is too large"));
        }
        Ok(Ok(bytes)) => bytes,
    };
    let revision = revision_of(&bytes);
    let text = String::from_utf8(bytes)
        .map_err(|_| Unresolved::Unreadable("the credential file is not text"))?;
    let text = non_empty(&text)
        .map(str::to_owned)
        .ok_or(Unresolved::Missing)?;
    Ok((text, revision))
}

fn revision_of(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(HEX[usize::from(byte >> 4)]);
        out.push(HEX[usize::from(byte & 15)]);
    }
    out
}

fn non_empty(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

#[cfg(test)]
mod tests {
    use super::{
        Extracted, KIMI_GLOBAL_BASE, KIMI_MAINLAND_BASE, Path, Secret, kimi_base_for_slot,
        omp_revision,
    };

    fn row(ctime: i64, digest: &str) -> Extracted {
        Extracted {
            id: 7,
            key: Secret::new("k".to_owned()),
            ctime,
            ctime_nsec: 0,
            digest: digest.to_owned(),
        }
    }

    #[test]
    fn the_kimi_code_slot_asks_the_mainland_coding_base() {
        assert_eq!(
            kimi_base_for_slot("kimi-code"),
            Some(KIMI_MAINLAND_BASE.to_owned())
        );
    }

    #[test]
    fn an_environment_slot_asks_the_global_coding_base() {
        assert_eq!(
            kimi_base_for_slot("kimi-code-env-work"),
            Some(KIMI_GLOBAL_BASE.to_owned())
        );
    }

    #[test]
    fn any_other_slot_asks_no_coding_base() {
        assert_eq!(kimi_base_for_slot("other"), None);
    }

    #[test]
    fn a_reused_store_identity_with_a_new_change_time_revises_the_entry() {
        let db = Path::new("/home/work/.omp/agent/agent.db");
        let before = omp_revision(db, 3, 9, &row(1, "d"));
        let after = omp_revision(db, 3, 9, &row(2, "d"));
        assert_ne!(before, after);
    }

    #[test]
    fn a_reused_store_identity_with_new_row_content_revises_the_entry() {
        let db = Path::new("/home/work/.omp/agent/agent.db");
        let before = omp_revision(db, 3, 9, &row(1, "d"));
        let after = omp_revision(db, 3, 9, &row(1, "e"));
        assert_ne!(before, after);
    }
}
