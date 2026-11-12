// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
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
const OPENCODE_INTEGRATION: &str = "opencode";
const OPENCODE_OAUTH: &str = "oauth";
const OPENCODE_SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
const OPENCODE_SNAPSHOT_ATTEMPTS: usize = 3;
const OPENCODE_WAL_HEADER: usize = 32;
const OPENCODE_WAL_FRAME_HEADER: usize = 24;
const OPENCODE_WAL_MAGIC: u32 = 0x377f_0682;
const OPENCODE_WAL_MAGIC_SWAPPED: u32 = 0x377f_0683;
const OPENCODE_WAL_FORMAT: u32 = 3_007_000;
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

async fn opencode_store(deps: &UsageDeps, path: &HostPath) -> Result<Option<Material>, Unresolved> {
    if !is_opencode_store(deps, path).await {
        return Ok(None);
    }
    let identity = deps
        .fs
        .identity(path)
        .await
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?
        .ok_or(Unresolved::Missing)?;
    let (bytes, wal) = read_opencode_snapshot(deps, path).await?;
    let found =
        tokio::task::spawn_blocking(move || extract_opencode_credential(&bytes, wal.as_deref()))
            .await
            .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))??;
    let revision = opencode_revision(path.as_path(), identity, &found);
    Ok(Some(
        Material::login(found.value).with_revision(Some(revision)),
    ))
}

async fn is_opencode_store(deps: &UsageDeps, path: &HostPath) -> bool {
    match deps.fs.read_at(path, 0, OPENCODE_SQLITE_MAGIC.len()).await {
        Ok(head) => head == OPENCODE_SQLITE_MAGIC,
        Err(_) => false,
    }
}

async fn read_opencode_snapshot(
    deps: &UsageDeps,
    path: &HostPath,
) -> Result<(Vec<u8>, Option<Vec<u8>>), Unresolved> {
    opencode_consistent_snapshot(|| async {
        let bytes = read_opencode_store(deps, path).await?;
        let wal = read_opencode_wal(deps, path).await?;
        Ok((bytes, wal))
    })
    .await
}

async fn opencode_consistent_snapshot<Read, ReadFuture>(
    mut read: Read,
) -> Result<(Vec<u8>, Option<Vec<u8>>), Unresolved>
where
    Read: FnMut() -> ReadFuture,
    ReadFuture: Future<Output = Result<(Vec<u8>, Option<Vec<u8>>), Unresolved>>,
{
    for _ in 0..OPENCODE_SNAPSHOT_ATTEMPTS {
        let (first, wal) = read().await?;
        let (second, _) = read().await?;
        if first == second {
            return Ok((first, wal));
        }
    }
    Err(Unresolved::Unreadable(
        "the OpenCode store could not be read",
    ))
}

async fn read_opencode_store(deps: &UsageDeps, path: &HostPath) -> Result<Vec<u8>, Unresolved> {
    deps.fs
        .read(path)
        .await
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?
        .ok_or(Unresolved::Missing)
}

async fn read_opencode_wal(
    deps: &UsageDeps,
    path: &HostPath,
) -> Result<Option<Vec<u8>>, Unresolved> {
    let wal = HostPath::parse(&PathBuf::from(format!("{}-wal", path.as_path().display())))
        .map_err(|_| Unresolved::Unreadable("the OpenCode write-ahead log could not be read"))?;
    match deps.fs.read(&wal).await {
        Ok(bytes) => Ok(bytes),
        Err(_) => Err(Unresolved::Unreadable(
            "the OpenCode write-ahead log could not be read",
        )),
    }
}

struct OpenCodeRecord {
    id: String,
    value: String,
    digest: String,
}

fn extract_opencode_credential(
    bytes: &[u8],
    wal: Option<&[u8]>,
) -> Result<OpenCodeRecord, Unresolved> {
    let image = opencode_store_image(bytes, wal)?;
    let mut connection = rusqlite::Connection::open_in_memory()
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be opened"))?;
    connection
        .deserialize_read_exact(
            "main",
            std::io::Cursor::new(image.as_slice()),
            image.len(),
            true,
        )
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be opened"))?;
    let mut statement = connection
        .prepare(
            "SELECT id, value FROM credential \
             WHERE integration_id = ?1 AND (active = 1 OR active IS NULL) \
             ORDER BY time_updated DESC, time_created DESC",
        )
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?;
    let mut rows = statement
        .query([OPENCODE_INTEGRATION])
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?;
    while let Some(row) = rows
        .next()
        .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?
    {
        let id: String = row
            .get(0)
            .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?;
        let value: String = row
            .get(1)
            .map_err(|_| Unresolved::Unreadable("the OpenCode store could not be read"))?;
        let kind = serde_json::from_str::<Value>(&value)
            .map_err(|_| Unresolved::Unreadable("the OpenCode credential is not valid JSON"))?;
        if kind.get("type").and_then(Value::as_str) != Some(OPENCODE_OAUTH) {
            continue;
        }
        return Ok(OpenCodeRecord {
            id,
            digest: revision_of(value.as_bytes()),
            value,
        });
    }
    Err(Unresolved::Missing)
}

fn opencode_store_image(bytes: &[u8], wal: Option<&[u8]>) -> Result<Vec<u8>, Unresolved> {
    let mut image = bytes.to_vec();
    if let Some(wal) = wal {
        apply_opencode_wal(&mut image, wal)?;
    }
    if let Some(header) = image.get_mut(18..20) {
        header.copy_from_slice(&[1, 1]);
    }
    Ok(image)
}

fn apply_opencode_wal(image: &mut Vec<u8>, wal: &[u8]) -> Result<(), Unresolved> {
    let unreadable = || Unresolved::Unreadable("the OpenCode write-ahead log could not be read");
    if wal.len() < OPENCODE_WAL_HEADER {
        return Ok(());
    }
    let Some(magic) = wal.get(0..4).and_then(opencode_u32) else {
        return Err(unreadable());
    };
    if magic != OPENCODE_WAL_MAGIC && magic != OPENCODE_WAL_MAGIC_SWAPPED {
        return Err(unreadable());
    }
    let big_endian = magic & 1 == 1;
    if wal.get(4..8).and_then(opencode_u32) != Some(OPENCODE_WAL_FORMAT) {
        return Err(unreadable());
    }
    let page_size = match wal.get(8..12).and_then(opencode_u32) {
        Some(1) => 65_536,
        Some(size) => size as usize,
        None => return Err(unreadable()),
    };
    if page_size < 512 || !page_size.is_power_of_two() {
        return Err(unreadable());
    }
    if image.len() < page_size || !image.len().is_multiple_of(page_size) {
        return Err(unreadable());
    }
    let store_page_size = match image.get(16..18).and_then(opencode_u16) {
        Some(1) => 65_536,
        Some(size) => size as usize,
        None => return Err(unreadable()),
    };
    if store_page_size != page_size {
        return Err(unreadable());
    }
    let Some(salt) = wal.get(16..24) else {
        return Err(unreadable());
    };
    let Some(header) = wal.get(0..24) else {
        return Err(unreadable());
    };
    let (mut check_one, mut check_two) = opencode_wal_checksum(header, 0, 0, big_endian);
    if wal.get(24..28).and_then(opencode_u32) != Some(check_one)
        || wal.get(28..32).and_then(opencode_u32) != Some(check_two)
    {
        return Err(unreadable());
    }
    let mut frames: Vec<(u32, usize)> = Vec::new();
    let mut committed: Vec<(u32, usize)> = Vec::new();
    let mut pages = 0usize;
    let mut offset = OPENCODE_WAL_HEADER;
    while offset + OPENCODE_WAL_FRAME_HEADER + page_size <= wal.len() {
        let Some(header) = wal.get(offset..offset + OPENCODE_WAL_FRAME_HEADER) else {
            break;
        };
        let Some(page) = header.get(0..4).and_then(opencode_u32) else {
            break;
        };
        let Some(after) = header.get(4..8).and_then(opencode_u32) else {
            break;
        };
        if page == 0 || header.get(8..16) != Some(salt) {
            break;
        }
        let Some(head) = header.get(0..8) else {
            break;
        };
        let (one, two) = opencode_wal_checksum(head, check_one, check_two, big_endian);
        let Some(page_data) = wal.get(
            offset + OPENCODE_WAL_FRAME_HEADER..offset + OPENCODE_WAL_FRAME_HEADER + page_size,
        ) else {
            break;
        };
        let (one, two) = opencode_wal_checksum(page_data, one, two, big_endian);
        if header.get(16..20).and_then(opencode_u32) != Some(one)
            || header.get(20..24).and_then(opencode_u32) != Some(two)
        {
            break;
        }
        check_one = one;
        check_two = two;
        frames.push((page, offset + OPENCODE_WAL_FRAME_HEADER));
        if after != 0 {
            committed.clone_from(&frames);
            pages = after as usize;
        }
        offset += OPENCODE_WAL_FRAME_HEADER + page_size;
    }
    if committed.is_empty() {
        return Ok(());
    }
    for (page, at) in committed {
        let start = (page as usize - 1) * page_size;
        if image.len() < start + page_size {
            image.resize(start + page_size, 0);
        }
        let Some(target) = image.get_mut(start..start + page_size) else {
            return Err(unreadable());
        };
        let Some(source) = wal.get(at..at + page_size) else {
            return Err(unreadable());
        };
        target.copy_from_slice(source);
    }
    image.truncate(pages * page_size);
    if let Some(size) = image.get_mut(28..32) {
        size.copy_from_slice(&(pages as u32).to_be_bytes());
    }
    Ok(())
}

fn opencode_u32(bytes: &[u8]) -> Option<u32> {
    match bytes {
        [first, second, third, fourth] => {
            Some(u32::from_be_bytes([*first, *second, *third, *fourth]))
        }
        _ => None,
    }
}

fn opencode_u16(bytes: &[u8]) -> Option<u16> {
    match bytes {
        [first, second] => Some(u16::from_be_bytes([*first, *second])),
        _ => None,
    }
}

fn opencode_wal_checksum(data: &[u8], mut one: u32, mut two: u32, big_endian: bool) -> (u32, u32) {
    let (chunks, _) = data.as_chunks::<8>();
    for chunk in chunks {
        let [
            byte_one,
            byte_two,
            byte_three,
            byte_four,
            byte_five,
            byte_six,
            byte_seven,
            byte_eight,
        ] = chunk;
        let first = if big_endian {
            u32::from_be_bytes([*byte_one, *byte_two, *byte_three, *byte_four])
        } else {
            u32::from_le_bytes([*byte_one, *byte_two, *byte_three, *byte_four])
        };
        let second = if big_endian {
            u32::from_be_bytes([*byte_five, *byte_six, *byte_seven, *byte_eight])
        } else {
            u32::from_le_bytes([*byte_five, *byte_six, *byte_seven, *byte_eight])
        };
        one = one.wrapping_add(first).wrapping_add(two);
        two = two.wrapping_add(second).wrapping_add(one);
    }
    (one, two)
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
        omp_revision, opencode_consistent_snapshot,
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

    fn snapshot_bytes(call: usize) -> Vec<u8> {
        if call == 1 {
            b"old".to_vec()
        } else {
            b"new".to_vec()
        }
    }

    #[tokio::test]
    async fn a_store_that_changes_while_it_is_read_is_read_again() {
        let mut calls = 0;
        let mut read = || {
            calls += 1;
            let call = calls;
            async move { Ok::<_, super::Unresolved>((snapshot_bytes(call), None)) }
        };
        let Ok((bytes, _)) = opencode_consistent_snapshot(&mut read).await else {
            panic!("a stable snapshot");
        };
        assert_eq!(bytes, b"new");
        assert_eq!(calls, 4);
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
