// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::time::Duration;

use jackin_core::{
    Agent, CaptureLimits, CaptureProblem, CredentialFileProblem, CredentialSource, FileIdentity,
    HostPath, OMP_CAPTURE_LIMITS, ProcessSpec, ProcessStatus, ProcessStdin, Program, SQLITE_MAGIC,
    STORE_VALUE_MAX_BYTES, Secret, StoreImage, capture, lookup_tuple, read_credential_file,
    run_interruptible,
};
use jackin_usage_provider_core::Credential;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::deps::UsageDeps;
use crate::request::UsageAccount;

const OP_TIMEOUT: Duration = Duration::from_secs(30);
const OPENCODE_LIMITS: CaptureLimits = CaptureLimits {
    database_bytes: 512 * 1024 * 1024,
    log_bytes: 128 * 1024 * 1024,
    image_bytes: 640 * 1024 * 1024,
    deadline: Duration::from_secs(60),
};
const CODEX_LOGIN_FILE: &str = "auth.json";
const OMP_DB: &str = jackin_core::OMP_HOST_STORE;
const KIMI_CONFIG: &str = "config.toml";
const KIMI_GLOBAL_BASE: &str = "https://api.kimi.ai/coding/v1";
const KIMI_MAINLAND_BASE: &str = "https://api.kimi.com/coding/v1";
const OPENCODE_INTEGRATION: &str = "opencode";
const OPENCODE_OAUTH: &str = "oauth";
const OPENCODE_CREDENTIAL_QUERY: &str = "SELECT id, value, time_updated, time_created FROM credential \
     WHERE integration_id = ?1 AND (active = 1 OR active IS NULL)";
const KIMI_GLOBAL_OAUTH: &str = "https://auth.kimi.ai";
const KIMI_MAINLAND_OAUTH: &str = "https://auth.kimi.com";
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
            if account.provider == jackin_core::Provider::Opencode
                && let Some(material) = opencode_store(deps, path).await?
            {
                return Ok(material);
            }
            let (text, revision) = read_credential(deps, path).await?;
            Ok(file_login(account, path, text).with_revision(Some(revision)))
        }
        CredentialSource::CodexLogin {
            home,
            chatgpt_account_id,
        } => {
            let path = HostPath::resolve(Path::new(CODEX_LOGIN_FILE), home)
                .map_err(|_| Unresolved::Unreadable("the Codex login file name is not usable"))?;
            let (text, revision) = read_credential(deps, &path).await?;
            let identity = codex_account_id(&text, chatgpt_account_id.as_str())?;
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

fn codex_account_id(text: &str, configured: &str) -> Result<Option<String>, Unresolved> {
    let Ok(document) = serde_json::from_str::<Value>(text) else {
        return Ok(None);
    };
    let Some(tokens) = document.get("tokens").and_then(Value::as_object) else {
        return Ok(None);
    };
    let account_id = tokens
        .get("account_id")
        .and_then(Value::as_str)
        .and_then(non_empty)
        .ok_or(Unresolved::Unreadable(
            "the Codex login holds no account id",
        ))?;
    if account_id != configured {
        return Err(Unresolved::Unreadable(
            "the Codex login account is not the configured account",
        ));
    }
    Ok(Some(account_id.to_owned()))
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
    let oauth_host = oauth
        .get("oauth_host")
        .map(|value| {
            value
                .as_str()
                .map(|value| value.trim().trim_end_matches('/'))
                .filter(|value| !value.is_empty())
                .ok_or(Unresolved::Unreadable(
                    "the Kimi config oauth host is not usable",
                ))
        })
        .transpose()?;
    match (base, oauth_host) {
        (KIMI_MAINLAND_BASE, None | Some(KIMI_MAINLAND_OAUTH)) => {}
        (KIMI_GLOBAL_BASE, Some(KIMI_GLOBAL_OAUTH)) => {}
        _ => {
            return Err(Unresolved::Unreadable(
                "the Kimi config hosts are not a supported Kimi host pair",
            ));
        }
    }
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

async fn opencode_store(deps: &UsageDeps, path: &HostPath) -> Result<Option<Material>, Unresolved> {
    let captured = match capture(deps.fs.as_ref(), path, OPENCODE_LIMITS).await {
        Ok(captured) => captured,
        Err(CaptureProblem::NotSqlite) => return Ok(None),
        Err(CaptureProblem::Symlink) if !links_to_sqlite(deps, path).await => return Ok(None),
        Err(CaptureProblem::Missing) => return Err(Unresolved::Missing),
        Err(CaptureProblem::LogUnreadable | CaptureProblem::LogTooLarge) => {
            return Err(Unresolved::Unreadable(
                "the OpenCode write-ahead log could not be read",
            ));
        }
        Err(_) => {
            return Err(Unresolved::Unreadable(
                "the OpenCode store could not be read",
            ));
        }
    };
    let identity = captured.identity;
    let found = query_image(captured.image, OPENCODE_WORDS, |connection| {
        extract_opencode_credential(connection)
    })
    .await?;
    let revision = opencode_revision(path.as_path(), identity, &found);
    Ok(Some(
        Material::login(found.value).with_revision(Some(revision)),
    ))
}

async fn links_to_sqlite(deps: &UsageDeps, path: &HostPath) -> bool {
    match deps.fs.read_at(path, 0, SQLITE_MAGIC.len()).await {
        Ok(head) => head == SQLITE_MAGIC,
        Err(_) => false,
    }
}

struct OpenCodeRecord {
    id: String,
    value: String,
    digest: String,
}

fn extract_opencode_credential(
    connection: &rusqlite::Connection,
) -> Result<OpenCodeRecord, Unresolved> {
    let mut statement = connection
        .prepare(OPENCODE_CREDENTIAL_QUERY)
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?;
    let mut rows = statement
        .query([OPENCODE_INTEGRATION])
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?;
    let mut found: Option<OpenCodeCandidate> = None;
    while let Some(row) = rows
        .next()
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?
    {
        let unreadable = || Unresolved::Unreadable("the OpenCode store could not be read");
        let id = row
            .get_ref(0)
            .and_then(bounded_text)
            .map_err(|_| unreadable())?;
        let value = row
            .get_ref(1)
            .and_then(bounded_text)
            .map_err(|_| unreadable())?;
        let updated: i64 = row
            .get(2)
            .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?;
        let created: i64 = row
            .get(3)
            .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?;
        let kind = serde_json::from_str::<Value>(&value)
            .map_err(|_| Unresolved::Unreadable("the OpenCode credential is not valid JSON"))?;
        if kind.get("type").and_then(Value::as_str) != Some(OPENCODE_OAUTH) {
            continue;
        }
        let candidate = OpenCodeCandidate {
            id,
            value,
            updated,
            created,
        };
        if found.as_ref().is_none_or(|best| candidate.newer_than(best)) {
            found = Some(candidate);
        }
    }
    let Some(found) = found else {
        return Err(Unresolved::Missing);
    };
    Ok(OpenCodeRecord {
        id: found.id,
        digest: revision_of(found.value.as_bytes()),
        value: found.value,
    })
}

struct OpenCodeCandidate {
    id: String,
    value: String,
    updated: i64,
    created: i64,
}

impl OpenCodeCandidate {
    fn newer_than(&self, other: &OpenCodeCandidate) -> bool {
        (self.updated, self.created) > (other.updated, other.created)
            || ((self.updated, self.created) == (other.updated, other.created)
                && self.id < other.id)
    }
}

fn opencode_revision(db: &Path, identity: FileIdentity, found: &OpenCodeRecord) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        db.display(),
        identity.dev,
        identity.ino,
        found.id,
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
    let captured = match capture(deps.fs.as_ref(), &db, OMP_CAPTURE_LIMITS).await {
        Ok(captured) => captured,
        Err(CaptureProblem::Missing) => return Err(Unresolved::Missing),
        Err(_) => return Err(Unresolved::Unreadable("the OMP store could not be read")),
    };
    let identity = captured.identity;
    let wanted = provider_id.to_owned();
    let found = query_image(captured.image, OMP_WORDS, move |connection| {
        read_omp_api_key(connection, &wanted)
    })
    .await?;
    Ok((omp_revision(db.as_path(), identity, &found), found.key))
}

struct Extracted {
    id: i64,
    key: Secret,
    digest: String,
}

fn omp_revision(db: &Path, identity: FileIdentity, found: &Extracted) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        db.display(),
        identity.dev,
        identity.ino,
        found.id,
        found.digest
    )
}

fn read_omp_api_key(
    connection: &rusqlite::Connection,
    provider_id: &str,
) -> Result<Extracted, Unresolved> {
    let (id, data): (i64, String) = connection
        .query_row(
            "SELECT id, data FROM auth_credentials WHERE provider = ?1 AND credential_type = 'api_key' ORDER BY id DESC LIMIT 1",
            [provider_id],
            |row| Ok((row.get(0)?, bounded_text(row.get_ref(1)?)?)),
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
        digest: revision_of(data.as_bytes()),
    })
}

fn bounded_text(value: rusqlite::types::ValueRef<'_>) -> rusqlite::Result<String> {
    let text = value.as_str()?;
    if text.len() > STORE_VALUE_MAX_BYTES {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(text.to_owned())
}

struct StoreWords {
    opened: &'static str,
    read: &'static str,
}

const OPENCODE_WORDS: StoreWords = StoreWords {
    opened: "the OpenCode store could not be opened",
    read: "the OpenCode store could not be read",
};
const OMP_WORDS: StoreWords = StoreWords {
    opened: "the OMP store could not be opened",
    read: "the OMP store could not be read",
};

async fn query_image<T, Work>(
    image: StoreImage,
    words: StoreWords,
    work: Work,
) -> Result<T, Unresolved>
where
    T: Send + 'static,
    Work: FnOnce(&rusqlite::Connection) -> Result<T, Unresolved> + Send + 'static,
{
    let connection = store_connection(words.opened)?;
    let handle = connection.get_interrupt_handle();
    let (opened, read) = (words.opened, words.read);
    let done = run_interruptible(
        move || handle.interrupt(),
        move || {
            let mut connection = connection;
            let length = image.as_slice().len();
            connection
                .deserialize_read_exact(
                    "main",
                    std::io::Cursor::new(image.as_slice()),
                    length,
                    true,
                )
                .map_err(|_| Unresolved::Unreadable(opened))?;
            work(&connection)
        },
    )
    .await;
    done.unwrap_or(Err(Unresolved::Unreadable(read)))
}

fn store_connection(opened: &'static str) -> Result<rusqlite::Connection, Unresolved> {
    let connection =
        rusqlite::Connection::open_in_memory().map_err(|_| Unresolved::Unreadable(opened))?;
    connection
        .execute_batch("PRAGMA temp_store = MEMORY;")
        .map_err(|_| Unresolved::Unreadable(opened))?;
    Ok(connection)
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
        Extracted, FileIdentity, KIMI_GLOBAL_BASE, KIMI_MAINLAND_BASE, OPENCODE_CREDENTIAL_QUERY,
        Path, Secret, kimi_base_for_slot, omp_revision,
    };

    const OPENCODE_SCHEMA: &str = "CREATE TABLE credential (
    id TEXT PRIMARY KEY NOT NULL,
    integration_id TEXT NOT NULL,
    label TEXT NOT NULL,
    value TEXT NOT NULL,
    active INTEGER,
    time_created INTEGER NOT NULL,
    time_updated INTEGER NOT NULL
);";

    fn plan_of(connection: &rusqlite::Connection) -> String {
        let query = format!("EXPLAIN QUERY PLAN {OPENCODE_CREDENTIAL_QUERY}");
        let mut statement = connection.prepare(&query).expect("the plan");
        let mut rows = statement.query(["opencode"]).expect("the plan rows");
        let mut plan = String::new();
        while let Some(row) = rows.next().expect("a plan row") {
            let detail: String = row.get(3).expect("the plan detail");
            plan.push_str(&detail);
            plan.push('\n');
        }
        plan
    }

    fn row(digest: &str) -> Extracted {
        Extracted {
            id: 7,
            key: Secret::new("k".to_owned()),
            digest: digest.to_owned(),
        }
    }

    fn identity(ino: u64) -> FileIdentity {
        FileIdentity { dev: 3, ino }
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
    fn a_reused_store_identity_with_new_row_content_revises_the_entry() {
        let db = Path::new("/home/work/.omp/agent/agent.db");
        let before = omp_revision(db, identity(9), &row("d"));
        let after = omp_revision(db, identity(9), &row("e"));
        assert_ne!(before, after);
    }

    #[test]
    fn a_new_store_identity_revises_the_entry() {
        let db = Path::new("/home/work/.omp/agent/agent.db");
        assert_ne!(
            omp_revision(db, identity(9), &row("d")),
            omp_revision(db, identity(10), &row("d"))
        );
    }

    #[test]
    fn the_credential_query_never_uses_a_temporary_b_tree() {
        let connection = rusqlite::Connection::open_in_memory().expect("a connection");
        connection
            .execute_batch(OPENCODE_SCHEMA)
            .expect("the schema");
        let plan = plan_of(&connection);
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }

    #[test]
    fn the_store_connection_keeps_its_temporary_storage_in_memory() {
        let Ok(connection) = super::store_connection("opened") else {
            panic!("a connection");
        };
        let store: i64 = connection
            .query_row("PRAGMA temp_store", [], |row| row.get(0))
            .expect("the temporary storage");
        assert_eq!(store, 2);
    }

    #[test]
    fn a_large_store_is_read_without_a_temporary_sort() {
        let directory = tempfile::tempdir().expect("a directory");
        let path = directory.path().join("opencode.db");
        {
            let connection = rusqlite::Connection::open(&path).expect("the store");
            connection
                .execute_batch(OPENCODE_SCHEMA)
                .expect("the schema");
            let filler = "x".repeat(4096);
            for index in 0..2000 {
                connection
                    .execute(
                        "INSERT INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES (?1, 'other', 'other', ?2, 1, ?3, ?3)",
                        rusqlite::params![format!("filler_{index}"), filler, index],
                    )
                    .expect("the filler row");
            }
            connection
                .execute(
                    "INSERT INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES ('cred_older', 'opencode', 'opencode', ?1, 1, 7, 7)",
                    rusqlite::params![r#"{"type":"oauth","access":"older"}"#],
                )
                .expect("the older row");
            connection
                .execute(
                    "INSERT INTO credential (id, integration_id, label, value, active, time_created, time_updated) VALUES ('cred_newer', 'opencode', 'opencode', ?1, 1, 5, 9)",
                    rusqlite::params![r#"{"type":"oauth","access":"newer"}"#],
                )
                .expect("the newer row");
        }
        let bytes = std::fs::read(&path).expect("the store bytes");
        let Ok(mut loaded) = super::store_connection("opened") else {
            panic!("a connection");
        };
        loaded
            .deserialize_read_exact(
                "main",
                std::io::Cursor::new(bytes.as_slice()),
                bytes.len(),
                true,
            )
            .expect("the image");
        let Ok(found) = super::extract_opencode_credential(&loaded) else {
            panic!("the credential");
        };
        assert_eq!(found.id, "cred_newer");
        assert!(found.value.contains("newer"));
        let Ok(connection) = super::store_connection("opened") else {
            panic!("a connection");
        };
        connection
            .execute_batch(OPENCODE_SCHEMA)
            .expect("the schema");
        let plan = plan_of(&connection);
        assert!(!plan.contains("TEMP B-TREE"), "{plan}");
    }
}
