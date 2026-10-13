// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical identities derived from explicit local source capabilities.

use jackin_protocol::usage_broker::UsageAccountCapability;
use jackin_usage_host_accounts::CanonicalAccountIdentity;
use jackin_usage_host_presentation::HostSurfaceId;

/// Map one exact opaque Claude source ID to its non-secret broker capability.
/// The resulting account ID is an identity label, not collection authority.
pub(crate) fn claude_usage_capability_for_source_id(
    source_capability_id: &str,
) -> UsageAccountCapability {
    let identity =
        CanonicalAccountIdentity::source_capability(HostSurfaceId::Claude, source_capability_id);
    let subject = identity.account_key();
    let hashed = jackin_core::account_key_hash("claude", &subject);
    UsageAccountCapability {
        surface_id: HostSurfaceId::Claude.id().to_owned(),
        account_id: hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned(),
    }
}
