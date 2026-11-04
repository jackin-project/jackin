// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{ChatgptAccountId, FileSystem, FsError, HostPath, Secret, plain};
use serde_json::Value;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

const AUTH_FILE: &str = "auth.json";
const CHATGPT_MODE: &str = "chatgpt";

#[derive(Debug)]
pub struct ChatgptLogin {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub client_id: String,
    pub account_id: ChatgptAccountId,
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("cannot read the login at {path}")]
    LoginUnreadable { path: HostPath },
    #[error("auth.json is invalid at {}", plain(.key))]
    AuthJsonInvalid { key: String },
    #[error("auth.json is not a ChatGPT login")]
    NotAChatgptLogin,
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

pub async fn read_chatgpt_login(
    fs: &dyn FileSystem,
    home: &HostPath,
) -> Result<ChatgptLogin, CredentialError> {
    let path = HostPath::resolve(Path::new(AUTH_FILE), home)
        .map_err(|_| CredentialError::LoginUnreadable { path: home.clone() })?;
    let bytes = Zeroizing::new(
        fs.read(&path)
            .await?
            .ok_or(CredentialError::LoginUnreadable { path })?,
    );
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
