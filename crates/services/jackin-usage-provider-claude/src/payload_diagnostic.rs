// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::lease::MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES;
use serde::{Deserialize, Deserializer, Serialize};

/// Fixed, value-free facts about a rejected Keychain payload. Every field name
/// and enum value is defined by this type; payload strings and unknown keys are
/// never copied into the diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaudeCredentialPayloadDiagnostic {
    payload_bytes: usize,
    limit_bytes: usize,
    json: ClaudePayloadJsonState,
    root: ClaudePayloadFieldKind,
    oauth_container: ClaudePayloadAliasKinds,
    access_token: ClaudePayloadAccessTokenKinds,
    subscription_type: ClaudePayloadSubscriptionKinds,
    account_container: ClaudePayloadAliasKinds,
    email_address: ClaudePayloadAliasKinds,
    organization_type: ClaudePayloadAliasKinds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ClaudePayloadJsonState {
    SkippedOversize,
    Invalid,
    Valid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ClaudePayloadFieldKind {
    Unavailable,
    Missing,
    Null,
    Object,
    String,
    Number,
    Boolean,
    Array,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ClaudePayloadAliasKinds {
    camel_case: ClaudePayloadFieldKind,
    snake_case: ClaudePayloadFieldKind,
    duplicate_alias: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ClaudePayloadAccessTokenKinds {
    camel_case: ClaudePayloadFieldKind,
    snake_case: ClaudePayloadFieldKind,
    camel_case_nonempty: Option<bool>,
    snake_case_nonempty: Option<bool>,
    duplicate_alias: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ClaudePayloadSubscriptionKinds {
    subscription_type: ClaudePayloadFieldKind,
    subscription_type_snake_case: ClaudePayloadFieldKind,
    rate_limit_tier: ClaudePayloadFieldKind,
    rate_limit_tier_snake_case: ClaudePayloadFieldKind,
    duplicate_alias: bool,
}

impl ClaudePayloadAliasKinds {
    fn unavailable() -> Self {
        Self {
            camel_case: ClaudePayloadFieldKind::Unavailable,
            snake_case: ClaudePayloadFieldKind::Unavailable,
            duplicate_alias: false,
        }
    }
}

impl ClaudePayloadAccessTokenKinds {
    fn unavailable() -> Self {
        Self {
            camel_case: ClaudePayloadFieldKind::Unavailable,
            snake_case: ClaudePayloadFieldKind::Unavailable,
            camel_case_nonempty: None,
            snake_case_nonempty: None,
            duplicate_alias: false,
        }
    }
}

impl ClaudePayloadSubscriptionKinds {
    fn unavailable() -> Self {
        Self {
            subscription_type: ClaudePayloadFieldKind::Unavailable,
            subscription_type_snake_case: ClaudePayloadFieldKind::Unavailable,
            rate_limit_tier: ClaudePayloadFieldKind::Unavailable,
            rate_limit_tier_snake_case: ClaudePayloadFieldKind::Unavailable,
            duplicate_alias: false,
        }
    }
}

#[derive(Default)]
struct ClaudePayloadRawField<'a>(Option<&'a serde_json::value::RawValue>);

impl<'de: 'a, 'a> Deserialize<'de> for ClaudePayloadRawField<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        <&'de serde_json::value::RawValue>::deserialize(deserializer).map(|raw| Self(Some(raw)))
    }
}

#[derive(Default, Deserialize)]
struct ClaudePayloadRawRoot<'a> {
    #[serde(default, borrow, rename = "claudeAiOauth")]
    oauth_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "claude_ai_oauth")]
    oauth_snake: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "oauthAccount")]
    account_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "oauth_account")]
    account_snake: ClaudePayloadRawField<'a>,
}

#[derive(Default, Deserialize)]
struct ClaudePayloadRawOAuth<'a> {
    #[serde(default, borrow, rename = "accessToken")]
    token_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "access_token")]
    token_snake: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "subscriptionType")]
    subscription_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "subscription_type")]
    subscription_snake: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "rateLimitTier")]
    rate_tier_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "rate_limit_tier")]
    rate_tier_snake: ClaudePayloadRawField<'a>,
}

#[derive(Default, Deserialize)]
struct ClaudePayloadRawAccount<'a> {
    #[serde(default, borrow, rename = "emailAddress")]
    email_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "email_address")]
    email_snake: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "organizationType")]
    organization_camel: ClaudePayloadRawField<'a>,
    #[serde(default, borrow, rename = "organization_type")]
    organization_snake: ClaudePayloadRawField<'a>,
}

struct ClaudePayloadClassified {
    kind: ClaudePayloadFieldKind,
    nonempty: Option<bool>,
}

fn classify_raw(raw: &serde_json::value::RawValue, token: bool) -> ClaudePayloadClassified {
    let value = raw.get().trim_start();
    let kind = match value.as_bytes().first() {
        Some(b'n') => ClaudePayloadFieldKind::Null,
        Some(b'{') => ClaudePayloadFieldKind::Object,
        Some(b'"') => ClaudePayloadFieldKind::String,
        Some(b't' | b'f') => ClaudePayloadFieldKind::Boolean,
        Some(b'[') => ClaudePayloadFieldKind::Array,
        Some(b'-' | b'0'..=b'9') => ClaudePayloadFieldKind::Number,
        _ => ClaudePayloadFieldKind::Unavailable,
    };
    let nonempty = (token && kind == ClaudePayloadFieldKind::String)
        .then(|| {
            value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
        })
        .flatten()
        .filter(|value| !value.contains('\\'))
        .map(|value| !value.trim().is_empty());
    ClaudePayloadClassified { kind, nonempty }
}

fn raw_field_kind(field: &ClaudePayloadRawField<'_>) -> ClaudePayloadFieldKind {
    field.0.map_or(ClaudePayloadFieldKind::Missing, |raw| {
        classify_raw(raw, false).kind
    })
}

fn raw_alias_kinds(
    camel: &ClaudePayloadRawField<'_>,
    snake: &ClaudePayloadRawField<'_>,
) -> ClaudePayloadAliasKinds {
    ClaudePayloadAliasKinds {
        camel_case: raw_field_kind(camel),
        snake_case: raw_field_kind(snake),
        duplicate_alias: camel.0.is_some() && snake.0.is_some(),
    }
}

fn raw_token_kinds(
    camel: &ClaudePayloadRawField<'_>,
    snake: &ClaudePayloadRawField<'_>,
) -> ClaudePayloadAccessTokenKinds {
    let camel_value = camel.0.map(|raw| classify_raw(raw, true));
    let snake_value = snake.0.map(|raw| classify_raw(raw, true));
    ClaudePayloadAccessTokenKinds {
        camel_case: camel_value
            .as_ref()
            .map_or(ClaudePayloadFieldKind::Missing, |value| value.kind),
        snake_case: snake_value
            .as_ref()
            .map_or(ClaudePayloadFieldKind::Missing, |value| value.kind),
        camel_case_nonempty: camel_value.and_then(|value| value.nonempty),
        snake_case_nonempty: snake_value.and_then(|value| value.nonempty),
        duplicate_alias: camel.0.is_some() && snake.0.is_some(),
    }
}

fn raw_subscription_kinds(oauth: &ClaudePayloadRawOAuth<'_>) -> ClaudePayloadSubscriptionKinds {
    let aliases = [
        &oauth.subscription_camel,
        &oauth.subscription_snake,
        &oauth.rate_tier_camel,
        &oauth.rate_tier_snake,
    ];
    let kinds = aliases.map(raw_field_kind);
    ClaudePayloadSubscriptionKinds {
        subscription_type: kinds[0],
        subscription_type_snake_case: kinds[1],
        rate_limit_tier: kinds[2],
        rate_limit_tier_snake_case: kinds[3],
        duplicate_alias: (oauth.subscription_camel.0.is_some()
            && oauth.subscription_snake.0.is_some())
            || (oauth.rate_tier_camel.0.is_some() && oauth.rate_tier_snake.0.is_some()),
    }
}

/// Describe a failed payload without exposing values. Check the size before
/// parsing so oversized Keychain data cannot cause diagnostic allocations.
pub fn diagnose_claude_profile_payload(bytes: &[u8]) -> ClaudeCredentialPayloadDiagnostic {
    let payload_bytes = bytes.len();
    let mut diagnostic = ClaudeCredentialPayloadDiagnostic {
        payload_bytes,
        limit_bytes: MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES,
        json: ClaudePayloadJsonState::SkippedOversize,
        root: ClaudePayloadFieldKind::Unavailable,
        oauth_container: ClaudePayloadAliasKinds::unavailable(),
        access_token: ClaudePayloadAccessTokenKinds::unavailable(),
        subscription_type: ClaudePayloadSubscriptionKinds::unavailable(),
        account_container: ClaudePayloadAliasKinds::unavailable(),
        email_address: ClaudePayloadAliasKinds::unavailable(),
        organization_type: ClaudePayloadAliasKinds::unavailable(),
    };
    if payload_bytes > MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES {
        return diagnostic;
    }

    let Ok(root_raw) = serde_json::from_slice::<&serde_json::value::RawValue>(bytes) else {
        diagnostic.json = ClaudePayloadJsonState::Invalid;
        return diagnostic;
    };
    diagnostic.json = ClaudePayloadJsonState::Valid;
    diagnostic.root = classify_raw(root_raw, false).kind;
    if diagnostic.root != ClaudePayloadFieldKind::Object {
        return diagnostic;
    }

    let Ok(root) = serde_json::from_str::<ClaudePayloadRawRoot<'_>>(root_raw.get()) else {
        return diagnostic;
    };
    diagnostic.oauth_container = raw_alias_kinds(&root.oauth_camel, &root.oauth_snake);
    diagnostic.account_container = raw_alias_kinds(&root.account_camel, &root.account_snake);

    let oauth_raw = root
        .oauth_camel
        .0
        .or(root.oauth_snake.0)
        .filter(|raw| classify_raw(raw, false).kind == ClaudePayloadFieldKind::Object);
    if let Some(oauth_raw) = oauth_raw
        && let Ok(oauth) = serde_json::from_str::<ClaudePayloadRawOAuth<'_>>(oauth_raw.get())
    {
        diagnostic.access_token = raw_token_kinds(&oauth.token_camel, &oauth.token_snake);
        diagnostic.subscription_type = raw_subscription_kinds(&oauth);
    }

    let account_raw = root
        .account_camel
        .0
        .or(root.account_snake.0)
        .filter(|raw| classify_raw(raw, false).kind == ClaudePayloadFieldKind::Object);
    if let Some(account_raw) = account_raw
        && let Ok(account) = serde_json::from_str::<ClaudePayloadRawAccount<'_>>(account_raw.get())
    {
        diagnostic.email_address = raw_alias_kinds(&account.email_camel, &account.email_snake);
        diagnostic.organization_type =
            raw_alias_kinds(&account.organization_camel, &account.organization_snake);
    }
    diagnostic
}

#[cfg(test)]
mod tests;
