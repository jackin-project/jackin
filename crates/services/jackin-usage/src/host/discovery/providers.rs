// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Per-provider profile identity.

use super::{
    ProfileCredentialMaterial, ProfileCredentialReader, ProfileReadOutcome, ProfileValidation,
    first_recursive_string, read_json,
};

use std::path::Path;

/// Cursor identity comes from the sibling `cli-config.json` (`authInfo`
/// email), verified locally; token presence in `auth.json` is proven at
/// discovery and the path is kept as refresh material, so refresh re-reads
/// the registered root instead of a stale discovery-time copy. A
/// present-but-tokenless `auth.json` is malformed, never an anonymous
/// binding refresh cannot serve.
pub(crate) fn cursor_profile_identity(
    reader: &dyn ProfileCredentialReader,
    root: &Path,
) -> ProfileValidation {
    let auth_path = root.join("auth.json");
    let value = match read_json(reader, &auth_path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    if crate::usage::cursor_auth_from_value(&value).is_none() {
        return ProfileValidation::Malformed;
    }
    let material = Some(Box::new(ProfileCredentialMaterial::Cursor { auth_path }));
    let label = read_json(reader, &root.join("cli-config.json"))
        .ok()
        .flatten()
        .and_then(|config| crate::usage::cursor_cli_identity_from_value(&config));
    match label {
        Some(label) => ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
        None => ProfileValidation::Anonymous(material),
    }
}

/// Gemini identity comes from `oauth_creds.json` when it names the login;
/// any valid credential file mints material (the Grok shape), since refresh
/// only needs discovery-proven OAuth presence until an entitlement endpoint
/// lands.
pub(crate) fn gemini_profile_identity(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Gemini {
        creds_path: path.to_path_buf(),
    }));
    first_recursive_string(&value, &["email", "user_email", "user_id", "account"]).map_or(
        ProfileValidation::Anonymous(material.clone()),
        |label| ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
    )
}

/// Antigravity identity is the host Keychain grant singleton, probed for
/// presence only: the CLI owns the secret, so any payload is ignored and no
/// identity label is extracted. Grant present → anonymous binding with
/// refresh material; absent/denied propagates truthfully.
pub(crate) fn antigravity_profile_identity(
    reader: &dyn ProfileCredentialReader,
) -> ProfileValidation {
    match reader.read_antigravity_keychain() {
        ProfileReadOutcome::Bytes(_) => {
            ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::Antigravity)))
        }
        ProfileReadOutcome::Missing => ProfileValidation::Missing,
        ProfileReadOutcome::Denied => ProfileValidation::Denied,
        ProfileReadOutcome::ConsentRequired => ProfileValidation::ConsentRequired,
    }
}

/// Muse identity comes from `auth.json` (`providers.meta.user_email`),
/// verified locally; the secret itself stays in the host Keychain.
pub(crate) fn muse_profile_identity(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let label = value
        .pointer("/providers/meta/user_email")
        .or_else(|| value.pointer("/providers/meta/user_full_name"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_owned);
    match label {
        Some(label) => ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material: None,
        },
        None => ProfileValidation::Anonymous(None),
    }
}

pub(crate) fn opencode_profile_identity(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    match reader.read(path) {
        ProfileReadOutcome::Missing => {
            if path
                .parent()
                .map(|parent| parent.join("opencode.db"))
                .is_some_and(|database| reader.exists(&database))
            {
                // Database-only OpenCode stores have no materializable auth
                // source. Do not advertise a usage profile until the database
                // credential identity can be carried through launch binding.
                ProfileValidation::Malformed
            } else {
                ProfileValidation::Missing
            }
        }
        ProfileReadOutcome::Denied => ProfileValidation::Denied,
        ProfileReadOutcome::ConsentRequired => ProfileValidation::ConsentRequired,
        ProfileReadOutcome::Bytes(bytes) => {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return ProfileValidation::Malformed;
            };
            let Some(entries) = value.as_object() else {
                return ProfileValidation::Malformed;
            };
            if entries.len() != 1 {
                return ProfileValidation::Malformed;
            }
            let entry = value.get("opencode-go");
            let Some(entry) = entry else {
                return ProfileValidation::Missing;
            };
            let kind = entry.get("type").and_then(serde_json::Value::as_str);
            let key = entry
                .get("key")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|key| !key.is_empty());
            if kind != Some("api") || key.is_none() {
                return ProfileValidation::Malformed;
            }
            ProfileValidation::Anonymous(Some(Box::new(ProfileCredentialMaterial::OpenCode {
                auth_path: path.to_path_buf(),
            })))
        }
    }
}

pub(crate) fn claude_profile_identity(
    reader: &dyn ProfileCredentialReader,
    root: &Path,
    operator_home: &Path,
) -> ProfileValidation {
    let mut paths = vec![root.join(".credentials.json"), root.join(".claude.json")];
    if root == operator_home.join(".claude") {
        paths.push(operator_home.join(".claude.json"));
    }
    let mut credential = None;
    let mut account_label = None;
    let mut organization_type = None;
    for path in paths {
        match read_json(reader, &path) {
            Ok(Some(value)) => {
                if credential.is_none() {
                    credential = crate::usage::claude_oauth_from_value(&value);
                }
                if account_label.is_none() {
                    account_label = crate::usage::claude_email_from_value(&value);
                }
                if organization_type.is_none() {
                    organization_type = crate::usage::claude_organization_type_from_value(&value);
                }
            }
            Ok(None) => {}
            Err(ProfileValidation::Denied) => return ProfileValidation::Denied,
            Err(ProfileValidation::ConsentRequired) => return ProfileValidation::ConsentRequired,
            Err(_) => return ProfileValidation::Malformed,
        }
    }
    if let Some(credential) = credential {
        let is_anonymous = account_label.is_none() && credential.refresh_token.is_none();
        let material = Some(Box::new(ProfileCredentialMaterial::Claude(
            crate::usage::ClaudeResolved {
                access_token: credential.access_token,
                subscription_type: credential.subscription_type,
                account_email: account_label.clone(),
                organization_type,
                credential_origin: "OAuth · configured profile".to_owned(),
                is_anonymous,
            },
        )));
        return account_label.map_or(ProfileValidation::Anonymous(material.clone()), |label| {
            ProfileValidation::Authenticated {
                provider_id: None,
                account_label: Some(label),
                material,
            }
        });
    }
    let current_dir = operator_home;
    let Some(scope) = jackin_core::claude_keychain_scope(root, operator_home, current_dir) else {
        return ProfileValidation::Malformed;
    };
    match reader.read_claude_keychain(&scope) {
        ProfileReadOutcome::Bytes(bytes) => {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return ProfileValidation::Malformed;
            };
            let Some(credential) = crate::usage::claude_oauth_from_value(&value) else {
                return ProfileValidation::Malformed;
            };
            let account_label = crate::usage::claude_email_from_value(&value);
            let is_anonymous = account_label.is_none() && credential.refresh_token.is_none();
            let material = Some(Box::new(ProfileCredentialMaterial::Claude(
                crate::usage::ClaudeResolved {
                    access_token: credential.access_token,
                    subscription_type: credential.subscription_type,
                    account_email: account_label.clone(),
                    organization_type: crate::usage::claude_organization_type_from_value(&value),
                    credential_origin: "OAuth · configured profile".to_owned(),
                    is_anonymous,
                },
            )));
            account_label.map_or(ProfileValidation::Anonymous(material.clone()), |label| {
                ProfileValidation::Authenticated {
                    provider_id: None,
                    account_label: Some(label),
                    material,
                }
            })
        }
        ProfileReadOutcome::Missing => ProfileValidation::Missing,
        ProfileReadOutcome::Denied => ProfileValidation::Denied,
        ProfileReadOutcome::ConsentRequired => ProfileValidation::ConsentRequired,
    }
}

pub(crate) fn codex_profile_identity(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let Some(credentials) = crate::usage::codex_oauth_from_value(&value) else {
        return ProfileValidation::Malformed;
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Codex {
        credentials: credentials.clone(),
        root: path.parent().unwrap_or_else(|| Path::new("")).to_path_buf(),
    }));
    if credentials.account_id.is_none() && credentials.account_label.is_none() {
        ProfileValidation::Anonymous(material)
    } else {
        ProfileValidation::Authenticated {
            provider_id: credentials.account_id,
            account_label: credentials.account_label,
            material,
        }
    }
}

pub(crate) fn amp_profile_identity(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let Some(object) = value.as_object() else {
        return ProfileValidation::Malformed;
    };
    let labeled = object.iter().find_map(|(key, value)| {
        let label = key.strip_prefix("apiKey@")?.trim();
        let secret = value.as_str()?.trim();
        (!label.is_empty() && !secret.is_empty()).then(|| (label.to_owned(), secret.to_owned()))
    });
    let fallback_key = object.values().find_map(|value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|secret| !secret.is_empty())
            .map(str::to_owned)
    });
    let Some(key) = labeled
        .as_ref()
        .map(|(_, key)| key.clone())
        .or(fallback_key)
    else {
        return ProfileValidation::Malformed;
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Amp { key }));
    labeled.map_or(
        ProfileValidation::Anonymous(material.clone()),
        |(label, _)| ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
    )
}

pub(crate) fn grok_profile_identity(
    reader: &dyn ProfileCredentialReader,
    path: &Path,
) -> ProfileValidation {
    let value = match read_json(reader, path) {
        Ok(Some(value)) => value,
        Ok(None) => return ProfileValidation::Missing,
        Err(outcome) => return outcome,
    };
    let material = Some(Box::new(ProfileCredentialMaterial::Grok {
        auth_path: path.to_path_buf(),
    }));
    first_recursive_string(&value, &["email", "user_id", "team_id"]).map_or(
        ProfileValidation::Anonymous(material.clone()),
        |label| ProfileValidation::Authenticated {
            provider_id: None,
            account_label: Some(label),
            material,
        },
    )
}
