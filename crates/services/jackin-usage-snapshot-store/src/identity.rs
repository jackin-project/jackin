// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use jackin_protocol::usage_broker::UsageAccountCapability;

use super::StoredAccountUsageSnapshot;

const RESERVED_IDENTITY_KEY_PREFIX: &str = "capability-v1:";

pub(crate) fn account_key_for_view(
    provider: &str,
    account_label: &str,
    capability: Option<&UsageAccountCapability>,
) -> Result<String, String> {
    let account_key_hash = capability.map_or_else(
        || jackin_core::account_key_hash(provider, account_label),
        UsageAccountCapability::opaque_identity_key,
    );
    validate_account_identity(provider, &account_key_hash, capability)?;
    Ok(account_key_hash)
}

pub(crate) fn decode_account_capability(
    provider: &str,
    account_key_hash: &str,
    account_id: Option<String>,
    surface_id: Option<String>,
) -> Result<Option<UsageAccountCapability>, String> {
    let capability = match (account_id, surface_id) {
        (None, None) => None,
        (Some(account_id), Some(surface_id)) => Some(UsageAccountCapability {
            account_id,
            surface_id,
        }),
        _ => {
            return Err(
                "snapshot account capability columns must be both NULL or both present".to_owned(),
            );
        }
    };
    validate_account_identity(provider, account_key_hash, capability.as_ref())?;
    Ok(capability)
}

pub(crate) fn validate_account_identity(
    provider: &str,
    account_key_hash: &str,
    capability: Option<&UsageAccountCapability>,
) -> Result<(), String> {
    let Some(capability) = capability else {
        if account_key_hash.starts_with(RESERVED_IDENTITY_KEY_PREFIX) {
            return Err(
                "unbound snapshot account key uses the reserved capability namespace".to_owned(),
            );
        }
        return Ok(());
    };

    validate_identifier(&capability.account_id, "account_id")?;
    validate_identifier(&capability.surface_id, "surface_id")?;
    if capability.opaque_identity_key() != account_key_hash {
        return Err("snapshot account capability does not match account_key_hash".to_owned());
    }

    let matches_provider = if provider == "OpenCode" {
        capability.surface_id == "opencode"
    } else {
        jackin_usage_provider_core::capability_matches_surface("", Some(provider), capability)
    };
    if !matches_provider {
        return Err(format!(
            "snapshot account capability surface {:?} does not match provider {provider:?}",
            capability.surface_id
        ));
    }
    Ok(())
}

pub(crate) fn validate_identity_groups<'a>(
    rows: impl IntoIterator<Item = (&'a str, &'a str, Option<&'a UsageAccountCapability>)>,
) -> Result<(), String> {
    let mut identities: HashMap<&str, (&str, Option<&UsageAccountCapability>)> = HashMap::new();
    for (provider, account_key_hash, capability) in rows {
        validate_account_identity(provider, account_key_hash, capability)?;
        if let Some((existing_provider, existing_capability)) = identities.get(account_key_hash) {
            if *existing_provider != provider || *existing_capability != capability {
                return Err(format!(
                    "snapshot rows for account key {account_key_hash:?} contain conflicting provider or capability identity"
                ));
            }
        } else {
            identities.insert(account_key_hash, (provider, capability));
        }
    }
    Ok(())
}

pub(crate) fn validate_stored_rows(rows: &[StoredAccountUsageSnapshot]) -> Result<(), String> {
    validate_identity_groups(rows.iter().map(|row| {
        (
            row.provider.as_str(),
            row.account_key_hash.as_str(),
            row.account_capability.as_ref(),
        )
    }))
}

fn validate_identifier(value: &str, name: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(format!(
            "snapshot account capability {name} is outside the accepted identifier domain"
        ));
    }
    Ok(())
}
