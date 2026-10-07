// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn parity_mega_screen(render_at: i64) -> UsageScreenState {
    // The fixture and renderer share one explicit epoch. This keeps relative
    // labels stable without mutating fixture timestamps to wall time.
    let providers = parity_mega_providers();
    let (projection, _) = parity_projection_at(
        render_at,
        &providers,
        parity_unresolved_entries(),
        vec![parity_projection_issue()],
    );
    UsageScreenState::from_projection(&projection)
}

pub(super) fn production_projection_runtime()
-> (tempfile::TempDir, HostUsageRuntime, UsageAccountCapability) {
    use crate::host::discovery::{ValidatedCredentialBinding, ValidatedCredentialSource};
    use crate::host::{DiscoveredAccountDescriptor, HostRuntimeConfig};
    let temp = tempfile::tempdir().unwrap();
    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Codex,
        subject: CanonicalAccountSubject::ProviderId("projection-account".to_owned()),
    };
    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Codex,
        identity: Some(identity.clone()),
        source_id: "projection-source".to_owned(),
        capability_id: "projection-capability".to_owned(),
        credential_revision: "revision".to_owned(),
        provenance: BTreeSet::from(["account work".to_owned()]),
        source: ValidatedCredentialSource::Capability,
    };
    let capability =
        jackin_usage_discovery::capability_for_binding(&binding, Some("projection-revision"));
    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("projection-revision".to_owned()),
        accounts: vec![DiscoveredAccountDescriptor {
            surface_id: "codex".to_owned(),
            account_key: identity.account_key(),
            account_label: "work@example.test".to_owned(),
            provenance: vec!["account work".to_owned()],
            source_ids: vec!["projection-source".to_owned()],
            identity,
        }],
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![binding],
    };
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open_with_validated_discovery(HostRuntimeConfig::under_data_dir(temp.path()), discovery)
        .unwrap();
    (temp, runtime, capability)
}
