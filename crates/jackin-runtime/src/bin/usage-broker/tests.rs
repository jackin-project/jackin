// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn registered_account_declaration_resolves_in_broker_service() {
    let mut config = jackin_config::AppConfig::default();
    config
        .env
        .insert("OPENAI_API_KEY".into(), "fixture-account-key".into());
    let entry = jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
        .iter()
        .find(|entry| entry.name == "OPENAI_API_KEY")
        .copied()
        .unwrap();
    let resolution = ServiceSecretSource
        .resolve_secret(&config, None, None, entry)
        .unwrap();
    assert!(
        matches!(resolution.outcome, ProviderCredentialSecretOutcome::Resolved(ref secret) if secret == "fixture-account-key")
    );
}
