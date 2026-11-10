// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Write as _;
use std::path::Path;

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jackin_core::{
    Account, ChatgptAccountId, CredentialFileProblem, CredentialSource, FileSystem, FsError,
    HostPath, ProcessError, ProcessSpec, ProcessStatus, ProcessStdin, Program, Provider, Secret,
    SecretBytes, plain, read_credential_file,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::service::HostPorts;
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

#[derive(Debug)]
pub struct OAuth2Login {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub expires_at: u64,
    pub expires_in: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KimiRegion {
    MainlandCn,
    Global,
}

impl KimiRegion {
    pub const fn base_url(self) -> &'static str {
        match self {
            KimiRegion::MainlandCn => "https://api.kimi.com/coding/v1",
            KimiRegion::Global => "https://api.kimi.ai/coding/v1",
        }
    }

    pub const fn oauth_host(self) -> Option<&'static str> {
        match self {
            KimiRegion::MainlandCn => None,
            KimiRegion::Global => Some("https://auth.kimi.ai"),
        }
    }

    pub const fn token_url(self) -> &'static str {
        match self {
            KimiRegion::MainlandCn => "https://auth.kimi.com/api/oauth/token",
            KimiRegion::Global => "https://auth.kimi.ai/api/oauth/token",
        }
    }

    pub const fn credential_slot(self) -> &'static str {
        match self {
            KimiRegion::MainlandCn => "kimi-code",
            KimiRegion::Global => "kimi-code-env-0e4f99c69cc27850",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KimiLoginMetadata {
    pub region: KimiRegion,
    pub scope: String,
}

#[derive(Debug)]
pub struct KimiHostLogin {
    pub grant: OAuth2Login,
    pub metadata: KimiLoginMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinimaxRegion {
    Cn,
    En,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinimaxLoginStatus {
    Authenticated,
    Expired,
}

#[derive(Debug)]
pub struct MinimaxHostLogin {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub region: MinimaxRegion,
    pub status: MinimaxLoginStatus,
    pub generation: u64,
    pub scopes: Vec<String>,
    pub expires_at_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeRefreshUnsupportedReason {
    AudienceUnsupported,
    NoRefreshGrant,
}

impl NativeRefreshUnsupportedReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::AudienceUnsupported => "audience unsupported",
            Self::NoRefreshGrant => "no refresh grant",
        }
    }
}

#[derive(Debug, Error)]
#[error("the native login cannot be refreshed by jackin: {}", .reason.as_str())]
pub struct NativeRefreshUnsupported {
    pub reason: NativeRefreshUnsupportedReason,
}

#[derive(Debug, Error)]
pub enum NativeCredentialError {
    #[error(transparent)]
    Credential(#[from] CredentialError),
    #[error("the native {} login is invalid at {key}", .provider.as_str())]
    Invalid {
        provider: Provider,
        key: &'static str,
    },
    #[error(transparent)]
    NativeRefreshUnsupported(#[from] NativeRefreshUnsupported),
    #[error(transparent)]
    Fs(#[from] FsError),
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
        .filter(|value| !value.trim().is_empty())
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
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub client_id: String,
    pub issuer: String,
}

#[derive(Debug)]
pub struct MuseHostMetadata {
    pub subs_active: bool,
    pub storage: String,
}

async fn read_host_bytes(
    fs: &dyn FileSystem,
    path: &HostPath,
) -> Result<Zeroizing<Vec<u8>>, CredentialError> {
    match read_credential_file(fs, path).await? {
        Ok(bytes) => Ok(Zeroizing::new(bytes)),
        Err(CredentialFileProblem::Missing) => {
            Err(CredentialError::LoginUnreadable { path: path.clone() })
        }
        Err(CredentialFileProblem::NotARegularFile) => {
            Err(CredentialError::LoginNotRegular { path: path.clone() })
        }
        Err(CredentialFileProblem::TooLarge) => {
            Err(CredentialError::LoginTooLarge { path: path.clone() })
        }
    }
}

async fn read_host_auth(
    fs: &dyn FileSystem,
    path: &HostPath,
    file: &str,
) -> Result<Document, CredentialError> {
    let bytes = read_host_bytes(fs, path).await?;
    Ok(Document(
        serde_json::from_slice(&bytes).map_err(|_| invalid(file))?,
    ))
}

mod native {
    use super::*;

    type CredentialError = NativeCredentialError;

    fn native_invalid(provider: Provider, key: &'static str) -> CredentialError {
        CredentialError::Invalid { provider, key }
    }

    fn native_field<'a>(
        parent: &'a Value,
        name: &str,
        provider: Provider,
        key: &'static str,
    ) -> Result<&'a Value, CredentialError> {
        parent
            .get(name)
            .ok_or_else(|| native_invalid(provider, key))
    }

    fn native_string<'a>(
        parent: &'a Value,
        name: &str,
        provider: Provider,
        key: &'static str,
    ) -> Result<&'a str, CredentialError> {
        native_field(parent, name, provider, key)?
            .as_str()
            .ok_or_else(|| native_invalid(provider, key))
    }

    fn native_nonempty_string<'a>(
        parent: &'a Value,
        name: &str,
        provider: Provider,
        key: &'static str,
    ) -> Result<&'a str, CredentialError> {
        let value = native_string(parent, name, provider, key)?;
        if value.is_empty() {
            return Err(native_invalid(provider, key));
        }
        Ok(value)
    }

    fn native_u64(
        parent: &Value,
        name: &str,
        provider: Provider,
        key: &'static str,
    ) -> Result<u64, CredentialError> {
        native_field(parent, name, provider, key)?
            .as_u64()
            .ok_or_else(|| native_invalid(provider, key))
    }

    fn native_positive_u64(
        parent: &Value,
        name: &str,
        provider: Provider,
        key: &'static str,
    ) -> Result<u64, CredentialError> {
        let value = native_u64(parent, name, provider, key)?;
        if value == 0 {
            return Err(native_invalid(provider, key));
        }
        Ok(value)
    }

    fn native_f64(
        parent: &Value,
        name: &str,
        provider: Provider,
        key: &'static str,
    ) -> Result<f64, CredentialError> {
        native_field(parent, name, provider, key)?
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| native_invalid(provider, key))
    }

    fn native_generation(parent: &Value, provider: Provider) -> Result<u64, CredentialError> {
        let generation = native_u64(parent, "generation", provider, "generation")?;
        if generation > 9_007_199_254_740_991 {
            return Err(native_invalid(provider, "generation"));
        }
        Ok(generation)
    }

    fn native_scopes(parent: &Value, provider: Provider) -> Result<Vec<String>, CredentialError> {
        let values = native_field(parent, "scopes", provider, "scopes")?
            .as_array()
            .ok_or_else(|| native_invalid(provider, "scopes"))?;
        let scopes = values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| native_invalid(provider, "scopes"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !scopes.iter().any(|scope| scope == "agent.default") {
            return Err(native_invalid(provider, "scopes"));
        }
        Ok(scopes)
    }

    fn native_path(
        root: &HostPath,
        relative: &str,
        provider: Provider,
    ) -> Result<HostPath, CredentialError> {
        HostPath::resolve(Path::new(relative), root).map_err(|_| native_invalid(provider, "path"))
    }

    async fn read_native_auth(
        fs: &dyn FileSystem,
        path: &HostPath,
        provider: Provider,
        key: &'static str,
    ) -> Result<Document, CredentialError> {
        let bytes = read_host_bytes(fs, path).await?;
        Ok(Document(
            serde_json::from_slice(&bytes).map_err(|_| native_invalid(provider, key))?,
        ))
    }

    struct TomlDocument(toml::Value);

    impl Drop for TomlDocument {
        fn drop(&mut self) {
            wipe_toml(&mut self.0);
        }
    }

    fn wipe_toml(value: &mut toml::Value) {
        match value {
            toml::Value::String(text) => text.zeroize(),
            toml::Value::Array(items) => items.iter_mut().for_each(wipe_toml),
            toml::Value::Table(fields) => fields.iter_mut().for_each(|(_, value)| wipe_toml(value)),
            toml::Value::Integer(_)
            | toml::Value::Float(_)
            | toml::Value::Boolean(_)
            | toml::Value::Datetime(_) => {}
        }
    }

    async fn read_kimi_config(
        fs: &dyn FileSystem,
        path: &HostPath,
    ) -> Result<TomlDocument, CredentialError> {
        let bytes = read_host_bytes(fs, path).await?;
        let text =
            std::str::from_utf8(&bytes).map_err(|_| native_invalid(Provider::Kimi, "config"))?;
        Ok(TomlDocument(
            toml::from_str(text).map_err(|_| native_invalid(Provider::Kimi, "config"))?,
        ))
    }

    const KIMI_MAINLAND_BASE: &str = "https://api.kimi.com/coding/v1";
    const KIMI_MAINLAND_OAUTH: &str = "https://auth.kimi.com";
    const KIMI_GLOBAL_BASE: &str = "https://api.kimi.ai/coding/v1";
    const KIMI_GLOBAL_OAUTH: &str = "https://auth.kimi.ai";

    fn kimi_slot(region: KimiRegion, base_url: &str) -> Result<String, CredentialError> {
        if region == KimiRegion::MainlandCn {
            return Ok("kimi-code".to_owned());
        }
        let input = format!(r#"{{"oauthHost":"{KIMI_GLOBAL_OAUTH}","baseUrl":"{base_url}"}}"#);
        let digest = Sha256::digest(input.as_bytes());
        let mut suffix = String::with_capacity(16);
        for byte in &digest[..8] {
            write!(&mut suffix, "{byte:02x}")
                .map_err(|_| native_invalid(Provider::Kimi, "oauth.key"))?;
        }
        Ok(format!("kimi-code-env-{suffix}"))
    }

    pub async fn read_kimi_host_login(
        fs: &dyn FileSystem,
        kimi_home: &HostPath,
    ) -> Result<KimiHostLogin, CredentialError> {
        let config_path = native_path(kimi_home, "config.toml", Provider::Kimi)?;
        let config = read_kimi_config(fs, &config_path).await?;
        let invalid = |key| native_invalid(Provider::Kimi, key);
        let providers = config
            .0
            .get("providers")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| invalid("providers"))?;
        let managed = providers
            .get("managed:kimi-code")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| invalid("providers.managed:kimi-code"))?;
        if managed.get("type").and_then(toml::Value::as_str) != Some("kimi") {
            return Err(invalid("providers.managed:kimi-code.type"));
        }
        if let Some(api_key) = managed.get("api_key")
            && api_key.as_str() != Some("")
        {
            return Err(invalid("providers.managed:kimi-code.api_key"));
        }
        if managed.contains_key("api_key_env") {
            return Err(invalid("providers.managed:kimi-code.api_key_env"));
        }
        let base_url = managed
            .get("base_url")
            .and_then(toml::Value::as_str)
            .map(|value| value.trim_end_matches('/'))
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid("providers.managed:kimi-code.base_url"))?;
        let oauth = managed
            .get("oauth")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| invalid("providers.managed:kimi-code.oauth"))?;
        if oauth.get("storage").and_then(toml::Value::as_str) != Some("file") {
            return Err(invalid("providers.managed:kimi-code.oauth.storage"));
        }
        let configured_key = oauth
            .get("key")
            .and_then(toml::Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| invalid("providers.managed:kimi-code.oauth.key"))?;
        let oauth_host = oauth
            .get("oauth_host")
            .map(|value| {
                value
                    .as_str()
                    .map(|value| value.trim().trim_end_matches('/'))
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| invalid("providers.managed:kimi-code.oauth.oauth_host"))
            })
            .transpose()?;
        let region = match (base_url, oauth_host) {
            (KIMI_MAINLAND_BASE, None | Some(KIMI_MAINLAND_OAUTH)) => KimiRegion::MainlandCn,
            (KIMI_GLOBAL_BASE, Some(KIMI_GLOBAL_OAUTH)) => KimiRegion::Global,
            _ => return Err(invalid("providers.managed:kimi-code.hosts")),
        };
        let slot = kimi_slot(region, base_url)?;
        if configured_key != format!("oauth/{slot}") {
            return Err(invalid("providers.managed:kimi-code.oauth.key"));
        }
        let token_path = native_path(
            kimi_home,
            &format!("credentials/{slot}.json"),
            Provider::Kimi,
        )?;
        let token = read_native_auth(fs, &token_path, Provider::Kimi, "token").await?;
        let access_token =
            native_nonempty_string(&token.0, "access_token", Provider::Kimi, "access_token")?;
        let refresh_token =
            native_nonempty_string(&token.0, "refresh_token", Provider::Kimi, "refresh_token")?;
        let expires_at = native_positive_u64(&token.0, "expires_at", Provider::Kimi, "expires_at")?;
        let expires_in = native_positive_u64(&token.0, "expires_in", Provider::Kimi, "expires_in")?;
        let scope = native_string(&token.0, "scope", Provider::Kimi, "scope")?;
        if native_string(&token.0, "token_type", Provider::Kimi, "token_type")? != "Bearer" {
            return Err(native_invalid(Provider::Kimi, "token_type"));
        }
        Ok(KimiHostLogin {
            grant: OAuth2Login {
                access_token: Secret::new(access_token.to_owned()),
                refresh_token: Secret::new(refresh_token.to_owned()),
                expires_at,
                expires_in,
            },
            metadata: KimiLoginMetadata {
                region,
                scope: scope.to_owned(),
            },
        })
    }

    const MINIMAX_CLIENT: &str = "mcode-public";
    const MINIMAX_AUDIENCE: &str = "agent-backend";

    impl MinimaxRegion {
        fn as_str(self) -> &'static str {
            match self {
                MinimaxRegion::Cn => "cn",
                MinimaxRegion::En => "en",
            }
        }
    }

    struct MinimaxState {
        status: MinimaxLoginStatus,
        generation: u64,
        scopes: Vec<String>,
        expires_at_ms: f64,
    }

    struct MinimaxCredential {
        access_token: Secret,
        refresh_token: Secret,
        generation: u64,
        scopes: Vec<String>,
        expires_at_ms: f64,
    }

    fn sensitive_state_key(key: &str) -> bool {
        let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
        ["accesstoken", "refreshtoken", "devicecode", "codeverifier"]
            .iter()
            .any(|sensitive| normalized.contains(sensitive))
    }

    fn sensitive_state_fields(value: &Value) -> bool {
        match value {
            Value::Object(fields) => fields
                .iter()
                .any(|(key, value)| sensitive_state_key(key) || sensitive_state_fields(value)),
            Value::Array(items) => items.iter().any(sensitive_state_fields),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
        }
    }

    fn parse_minimax_state(
        value: &Value,
        region: MinimaxRegion,
    ) -> Result<MinimaxState, CredentialError> {
        if sensitive_state_fields(value) {
            return Err(native_invalid(Provider::Minimax, "state.sensitive"));
        }
        if value.get("authorization").is_some() || value.get("refresh").is_some() {
            return Err(native_invalid(Provider::Minimax, "state.pending"));
        }
        if native_u64(
            value,
            "schemaVersion",
            Provider::Minimax,
            "state.schemaVersion",
        )? != 2
        {
            return Err(native_invalid(Provider::Minimax, "state.schemaVersion"));
        }
        let status = match native_string(value, "status", Provider::Minimax, "state.status")? {
            "authenticated" => MinimaxLoginStatus::Authenticated,
            "expired" => MinimaxLoginStatus::Expired,
            _ => return Err(native_invalid(Provider::Minimax, "state.status")),
        };
        if native_string(value, "storeKind", Provider::Minimax, "state.storeKind")? != "file" {
            return Err(native_invalid(Provider::Minimax, "state.storeKind"));
        }
        if native_string(value, "clientId", Provider::Minimax, "state.clientId")? != MINIMAX_CLIENT
        {
            return Err(native_invalid(Provider::Minimax, "state.clientId"));
        }
        if native_string(value, "audience", Provider::Minimax, "state.audience")?
            != MINIMAX_AUDIENCE
        {
            return Err(native_invalid(Provider::Minimax, "state.audience"));
        }
        if native_string(value, "buildEnv", Provider::Minimax, "state.buildEnv")? != "prod" {
            return Err(native_invalid(Provider::Minimax, "state.buildEnv"));
        }
        if native_string(value, "region", Provider::Minimax, "state.region")? != region.as_str() {
            return Err(native_invalid(Provider::Minimax, "state.region"));
        }
        Ok(MinimaxState {
            status,
            generation: native_generation(value, Provider::Minimax)?,
            scopes: native_scopes(value, Provider::Minimax)?,
            expires_at_ms: native_f64(
                value,
                "expiresAtMs",
                Provider::Minimax,
                "state.expiresAtMs",
            )?,
        })
    }

    fn parse_minimax_credential(value: &Value) -> Result<MinimaxCredential, CredentialError> {
        if native_u64(
            value,
            "schemaVersion",
            Provider::Minimax,
            "credential.schemaVersion",
        )? != 1
        {
            return Err(native_invalid(
                Provider::Minimax,
                "credential.schemaVersion",
            ));
        }
        if native_string(
            value,
            "tokenType",
            Provider::Minimax,
            "credential.tokenType",
        )? != "Bearer"
        {
            return Err(native_invalid(Provider::Minimax, "credential.tokenType"));
        }
        if native_string(value, "clientId", Provider::Minimax, "credential.clientId")?
            != MINIMAX_CLIENT
        {
            return Err(native_invalid(Provider::Minimax, "credential.clientId"));
        }
        if native_string(value, "audience", Provider::Minimax, "credential.audience")?
            != MINIMAX_AUDIENCE
        {
            return Err(native_invalid(Provider::Minimax, "credential.audience"));
        }
        for identity in ["subject", "accountId", "loginEpoch"] {
            if value.get(identity).is_some_and(|value| !value.is_string()) {
                return Err(native_invalid(Provider::Minimax, "credential.identity"));
            }
        }
        Ok(MinimaxCredential {
            access_token: Secret::new(
                native_nonempty_string(
                    value,
                    "accessToken",
                    Provider::Minimax,
                    "credential.accessToken",
                )?
                .to_owned(),
            ),
            refresh_token: Secret::new(
                native_nonempty_string(
                    value,
                    "refreshToken",
                    Provider::Minimax,
                    "credential.refreshToken",
                )?
                .to_owned(),
            ),
            generation: native_generation(value, Provider::Minimax)?,
            scopes: native_scopes(value, Provider::Minimax)?,
            expires_at_ms: native_f64(
                value,
                "expiresAtMs",
                Provider::Minimax,
                "credential.expiresAtMs",
            )?,
        })
    }

    fn minimax_record_key(
        data_root: &HostPath,
        region: MinimaxRegion,
    ) -> Result<Zeroizing<String>, CredentialError> {
        let auth_home = data_root.as_path().join("auth");
        let auth_home = auth_home
            .to_str()
            .ok_or_else(|| native_invalid(Provider::Minimax, "path"))?;
        let input = Zeroizing::new(format!("{auth_home}\0{MINIMAX_CLIENT}"));
        let account = Zeroizing::new(URL_SAFE_NO_PAD.encode(Sha256::digest(input.as_bytes())));
        Ok(Zeroizing::new(format!(
            "com.minimax.mcode.oauth.prod.{}\0{}",
            region.as_str(),
            account.as_str()
        )))
    }

    async fn read_minimax_region(
        fs: &dyn FileSystem,
        data_root: &HostPath,
        region: MinimaxRegion,
    ) -> Result<Option<MinimaxHostLogin>, CredentialError> {
        let namespace = format!("auth/prod/{}/{MINIMAX_CLIENT}", region.as_str());
        let state_path = native_path(
            data_root,
            &format!("{namespace}/auth-state.json"),
            Provider::Minimax,
        )?;
        let credential_path = native_path(
            data_root,
            &format!("{namespace}/auth.json"),
            Provider::Minimax,
        )?;
        if fs.canonicalize(&state_path).await?.is_none() {
            if fs.canonicalize(&credential_path).await?.is_some() {
                return Err(CredentialError::Credential(
                    super::CredentialError::LoginUnreadable {
                        path: state_path.clone(),
                    },
                ));
            }
            return Ok(None);
        }
        let state_document = read_native_auth(fs, &state_path, Provider::Minimax, "state").await?;
        let state = parse_minimax_state(&state_document.0, region)?;
        let credential_document =
            read_native_auth(fs, &credential_path, Provider::Minimax, "credential").await?;
        if native_u64(
            &credential_document.0,
            "schemaVersion",
            Provider::Minimax,
            "credentials.schemaVersion",
        )? != 1
        {
            return Err(native_invalid(
                Provider::Minimax,
                "credentials.schemaVersion",
            ));
        }
        let records = native_field(
            &credential_document.0,
            "records",
            Provider::Minimax,
            "credentials.records",
        )?
        .as_object()
        .ok_or_else(|| native_invalid(Provider::Minimax, "credentials.records"))?;
        let record_key = minimax_record_key(data_root, region)?;
        let mut selected = None;
        for (key, value) in records {
            let credential = parse_minimax_credential(value)?;
            if key == record_key.as_str() {
                if selected.is_some() {
                    return Err(native_invalid(Provider::Minimax, "credentials.records"));
                }
                selected = Some(credential);
            }
        }
        let credential =
            selected.ok_or_else(|| native_invalid(Provider::Minimax, "credentials.records"))?;
        if credential.generation != state.generation {
            return Err(native_invalid(Provider::Minimax, "state.generation"));
        }
        if credential.scopes != state.scopes
            || credential.expires_at_ms.to_bits() != state.expires_at_ms.to_bits()
        {
            return Err(native_invalid(Provider::Minimax, "state.credential"));
        }
        Ok(Some(MinimaxHostLogin {
            access_token: credential.access_token,
            refresh_token: credential.refresh_token,
            region,
            status: state.status,
            generation: credential.generation,
            scopes: credential.scopes,
            expires_at_ms: credential.expires_at_ms,
        }))
    }

    pub async fn read_minimax_host_login(
        fs: &dyn FileSystem,
        data_root: &HostPath,
    ) -> Result<MinimaxHostLogin, CredentialError> {
        let mut selected = None;
        for region in [MinimaxRegion::Cn, MinimaxRegion::En] {
            if let Some(login) = read_minimax_region(fs, data_root, region).await? {
                if selected.is_some() {
                    return Err(native_invalid(Provider::Minimax, "regions"));
                }
                selected = Some(login);
            }
        }
        selected.ok_or_else(|| native_invalid(Provider::Minimax, "namespace"))
    }

    pub async fn read_mimo_host_login(
        fs: &dyn FileSystem,
        data_root: &HostPath,
    ) -> Result<OAuth2Login, CredentialError> {
        let path = native_path(data_root, AUTH_FILE, Provider::Mimo)?;
        let document = read_native_auth(fs, &path, Provider::Mimo, "auth").await?;
        let record = native_field(&document.0, "xiaomi", Provider::Mimo, "xiaomi")?;
        if native_string(record, "type", Provider::Mimo, "xiaomi.type")? != "api" {
            return Err(native_invalid(Provider::Mimo, "xiaomi.type"));
        }
        native_nonempty_string(record, "key", Provider::Mimo, "xiaomi.key")?;
        Err(NativeRefreshUnsupported {
            reason: NativeRefreshUnsupportedReason::NoRefreshGrant,
        }
        .into())
    }
}

pub use native::{read_kimi_host_login, read_mimo_host_login, read_minimax_host_login};

pub async fn read_grok_login(
    fs: &dyn FileSystem,
    path: &HostPath,
) -> Result<GrokLogin, CredentialError> {
    let document = read_host_auth(fs, path, AUTH_FILE).await?;
    let root = document.0.as_object().ok_or_else(|| invalid(AUTH_FILE))?;
    let mut entries = root.values().filter(|entry| {
        entry.get("oidc_issuer").and_then(Value::as_str) == Some("https://auth.x.ai")
            && matches!(
                entry.get("auth_mode").and_then(Value::as_str),
                Some("session" | "oidc")
            )
    });
    let entry = entries.next().ok_or_else(|| invalid(AUTH_FILE))?;
    if entries.next().is_some() {
        return Err(invalid(AUTH_FILE));
    }
    let access_token = text(entry, "key", "key")?;
    let refresh_token = text(entry, "refresh_token", "refresh_token")?;
    let client_id = text(entry, "oidc_client_id", "oidc_client_id")?;
    let issuer = text(entry, "oidc_issuer", "oidc_issuer")?;
    Ok(GrokLogin {
        access_token: Secret::new(access_token.to_owned()),
        refresh_token: Secret::new(refresh_token.to_owned()),
        client_id: client_id.to_owned(),
        issuer: issuer.to_owned(),
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
    pub refresh_token: Secret,
    pub expires_at: i64,
    pub scopes: Vec<String>,
}

fn parse_claude_grant(root: &Value) -> Result<ClaudeLogin, &'static str> {
    let oauth = root
        .get("claudeAiOauth")
        .and_then(Value::as_object)
        .ok_or("claudeAiOauth")?;
    let access_token = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or("claudeAiOauth.accessToken")?;
    let refresh_token = oauth
        .get("refreshToken")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or("claudeAiOauth.refreshToken")?;
    let expires_at = oauth
        .get("expiresAt")
        .and_then(Value::as_i64)
        .filter(|value| *value > 0)
        .ok_or("claudeAiOauth.expiresAt")?;
    let scopes = oauth
        .get("scopes")
        .and_then(Value::as_array)
        .filter(|scopes| !scopes.is_empty())
        .ok_or("claudeAiOauth.scopes")?
        .iter()
        .map(|scope| {
            scope
                .as_str()
                .filter(|scope| !scope.trim().is_empty())
                .map(str::to_owned)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or("claudeAiOauth.scopes")?;
    Ok(ClaudeLogin {
        access_token: Secret::new(access_token.to_owned()),
        refresh_token: Secret::new(refresh_token.to_owned()),
        expires_at,
        scopes,
    })
}

pub async fn read_claude_login(
    fs: &dyn FileSystem,
    path: &HostPath,
) -> Result<ClaudeLogin, CredentialError> {
    let document = read_host_auth(fs, path, CLAUDE_CREDENTIALS_FILE).await?;
    parse_claude_grant(&document.0).map_err(invalid)
}

pub fn parse_claude_keychain_secret(stdout: &SecretBytes) -> Option<ClaudeLogin> {
    let document = Document(serde_json::from_slice(stdout.expose()).ok()?);
    parse_claude_grant(&document.0).ok()
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
) -> Result<ClaudeLogin, CredentialError> {
    if let Ok(login) = read_claude_login(host.fs.as_ref(), path).await {
        return Ok(login);
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
            let db = HostPath::resolve(Path::new(".omp/agent/agent.db"), &host.home)
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
