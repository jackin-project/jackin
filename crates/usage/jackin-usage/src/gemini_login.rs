// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Secret, SecretBytes};
use jackin_usage_provider_core::ProviderError;
use serde::Deserialize;

#[derive(Debug)]
pub struct DesktopClient {
    pub id: String,
    pub secret: Option<Secret>,
}

#[derive(Deserialize)]
struct Registration<'a> {
    #[serde(borrow)]
    installed: Installed<'a>,
    web: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct Installed<'a> {
    client_id: &'a str,
    client_secret: Option<&'a str>,
}

impl DesktopClient {
    pub(crate) fn from_installed_json(bytes: &SecretBytes) -> Option<Self> {
        let registration: Registration<'_> = serde_json::from_slice(bytes.expose()).ok()?;
        if registration.web.is_some() {
            return None;
        }
        let installed = registration.installed;
        let secret = installed
            .client_secret
            .filter(|s| !s.is_empty())
            .map(|s| Secret::new(s.to_owned()));
        Self::new(installed.client_id.to_owned(), secret).ok()
    }

    pub fn new(id: String, secret: Option<Secret>) -> Result<Self, ProviderError> {
        if !id.ends_with(".apps.googleusercontent.com")
            || id.len() < 28
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
        {
            return Err(ProviderError::Unsupported(
                "enter the OAuth client ID of a Google Desktop app",
            ));
        }
        Ok(Self { id, secret })
    }
}
