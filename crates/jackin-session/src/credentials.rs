// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ChatgptAccountId, FsError, HostPath, Secret};
use thiserror::Error;

#[derive(Debug)]
pub struct ChatgptLogin {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub client_id: String,
    pub account_id: ChatgptAccountId,
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("cannot read the login at {path:?}")]
    LoginUnreadable { path: HostPath },
    #[error("auth.json is invalid at {key}")]
    AuthJsonInvalid { key: String },
    #[error("auth.json is not a ChatGPT login")]
    NotAChatgptLogin,
    #[error(transparent)]
    Fs(#[from] FsError),
}
