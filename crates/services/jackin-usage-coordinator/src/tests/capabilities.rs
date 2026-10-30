// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCoordinationErrorKind};

use crate::{UsageCapabilitySet, UsageCoordinatorConfig};

fn capability(surface: &str, account: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        surface_id: surface.to_owned(),
        account_id: account.to_owned(),
    }
}

#[test]
fn authorized_capability_passes_authorize_check() {
    let allowed = UsageCapabilitySet::new([capability("claude", "account-1")]);

    allowed
        .authorize(&capability("claude", "account-1"))
        .expect("authorized capability must pass");
}

#[test]
fn unknown_capability_is_rejected_as_unauthorized() {
    let allowed = UsageCapabilitySet::new([capability("claude", "account-1")]);

    let error = allowed
        .authorize(&capability("opencode", "account-9"))
        .expect_err("foreign capability must be rejected");

    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
}

#[test]
fn resolve_surface_selects_the_unique_authorized_account() {
    let allowed = UsageCapabilitySet::new([capability("claude", "account-1")]);

    let selected = allowed
        .resolve_surface("claude")
        .expect("unique surface must resolve");

    assert_eq!(selected.account_id, "account-1");
}

#[test]
fn resolve_surface_rejects_ambiguous_and_unknown_surfaces() {
    let allowed = UsageCapabilitySet::new([
        capability("claude", "account-1"),
        capability("claude", "account-2"),
    ]);

    let ambiguous = allowed
        .resolve_surface("claude")
        .expect_err("ambiguous surface must be rejected");
    assert_eq!(ambiguous.kind, UsageCoordinationErrorKind::Unauthorized);

    let unknown = allowed
        .resolve_surface("opencode")
        .expect_err("unknown surface must be rejected");
    assert_eq!(unknown.kind, UsageCoordinationErrorKind::Unauthorized);
}

#[test]
fn default_config_carries_usable_execution_bounds() {
    let config = UsageCoordinatorConfig::default();

    assert!(config.max_concurrency > 0);
    assert!(config.queue_capacity > 0);
    assert!(!config.provider_timeout.is_zero());
    assert!(!config.success_cooldown.is_zero());
}
