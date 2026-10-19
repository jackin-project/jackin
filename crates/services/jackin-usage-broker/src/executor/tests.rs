// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use jackin_config::AppConfig;
use jackin_core::{UsageCredentialEnvName, WorkspaceName};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationErrorKind, UsageCredentialSourceIdentity,
};
use jackin_usage_discovery::{
    ProfileCredentialMaterial, UsageDiscoveryScope, ValidatedCredentialBinding,
    ValidatedCredentialSource,
};
use jackin_usage_host_credentials::{
    OpaqueCredentialHandle, ProviderCredentialEnvResolution, ProviderCredentialEnvResolver,
    ProviderCredentialRefreshOutcome, ProviderCredentialSourceMaterial,
};
use jackin_usage_provider_claude::ClaudeResolved;

use super::*;

#[derive(Default)]
struct CountingResolver {
    resolve_calls: AtomicUsize,
    refresh_calls: AtomicUsize,
}

impl ProviderCredentialEnvResolver for CountingResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        self.resolve_calls.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    }

    fn refresh_provider_credential(
        &self,
        _surface: HostSurfaceId,
        _key: &str,
        _handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialRefreshOutcome {
        self.refresh_calls.fetch_add(1, Ordering::SeqCst);
        ProviderCredentialRefreshOutcome::Malformed
    }
}

fn capability(surface: HostSurfaceId, account_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: surface.id().to_owned(),
    }
}

fn binding(
    surface: HostSurfaceId,
    capability_id: &str,
    source: ValidatedCredentialSource,
) -> ValidatedCredentialBinding {
    ValidatedCredentialBinding {
        surface,
        identity: None,
        source_id: format!("source-{capability_id}"),
        capability_id: capability_id.to_owned(),
        credential_revision: format!("revision-{capability_id}"),
        provenance: BTreeSet::new(),
        source,
    }
}

fn profile_claude_binding(capability_id: &str) -> ValidatedCredentialBinding {
    binding(
        HostSurfaceId::Claude,
        capability_id,
        ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Claude(
            ClaudeResolved::from_token(
                "fixture-token".to_owned(),
                None,
                Some(format!("{capability_id}@example.test")),
                None,
                "OAuth · configured profile".to_owned(),
                false,
            ),
        )),
    )
}

fn env_claude_binding(capability_id: &str) -> ValidatedCredentialBinding {
    binding(
        HostSurfaceId::Claude,
        capability_id,
        ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new(format!("handle-{capability_id}")),
            key: "ANTHROPIC_AUTH_TOKEN".to_owned(),
            dispatch_key: "ANTHROPIC_AUTH_TOKEN".to_owned(),
            launch_keys: BTreeSet::from(["ANTHROPIC_AUTH_TOKEN".to_owned()]),
            material: Some(env_material("ANTHROPIC_AUTH_TOKEN")),
        },
    )
}

fn env_binding(
    surface: HostSurfaceId,
    capability_id: &str,
    key: &str,
) -> ValidatedCredentialBinding {
    ValidatedCredentialBinding {
        surface,
        identity: None,
        source_id: format!("source-{capability_id}"),
        capability_id: capability_id.to_owned(),
        credential_revision: format!("revision-{capability_id}"),
        provenance: BTreeSet::new(),
        source: ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new(format!("handle-{capability_id}")),
            key: key.to_owned(),
            dispatch_key: key.to_owned(),
            launch_keys: BTreeSet::from([key.to_owned()]),
            material: Some(env_material(key)),
        },
    }
}

fn env_material(name: &str) -> ProviderCredentialSourceMaterial {
    ProviderCredentialSourceMaterial {
        source: UsageCredentialSourceIdentity::HostEnv {
            name: name.to_owned(),
        },
        material_fingerprint: format!("fixture-{name}"),
    }
}

fn executor(
    bindings: BTreeMap<UsageAccountCapability, Vec<ValidatedCredentialBinding>>,
    scope: UsageDiscoveryScope,
    resolver: Arc<CountingResolver>,
) -> DiscoveryProviderExecutor {
    DiscoveryProviderExecutor {
        bindings: Mutex::new(bindings),
        validated_catalog: Mutex::new(None),
        scope,
        resolver,
        probe_budget: Duration::from_secs(1),
        collector_lease: None,
        monitor_store: None,
    }
}

#[test]
fn projection_probes_cannot_refresh_selected_or_unselected_claude_accounts() {
    let resolver = Arc::new(CountingResolver::default());
    let profile_capability = capability(HostSurfaceId::Claude, "selected-profile");
    let env_capability = capability(HostSurfaceId::Claude, "unselected-env");
    let uncached_capability = capability(HostSurfaceId::Claude, "uncached-profile");
    let fixture = tempfile::tempdir().expect("empty discovery fixture");
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: fixture.path().join("config"),
        operator_home: fixture.path().join("home"),
    };

    let claude_executor = executor(
        BTreeMap::from([
            (
                profile_capability.clone(),
                vec![profile_claude_binding("selected-profile")],
            ),
            (
                env_capability.clone(),
                vec![env_claude_binding("unselected-env")],
            ),
        ]),
        scope.clone(),
        Arc::clone(&resolver),
    );
    let provider_refresh_calls = Arc::new(AtomicUsize::new(0));
    for capability in [profile_capability, env_capability, uncached_capability] {
        let provider_refresh_calls = Arc::clone(&provider_refresh_calls);
        let outcome = probe_with_scope_using(
            &claude_executor,
            &capability,
            None,
            move |binding, resolver| {
                provider_refresh_calls.fetch_add(1, Ordering::SeqCst);
                if let ValidatedCredentialSource::Env {
                    handle,
                    dispatch_key,
                    ..
                } = &binding.source
                {
                    resolver.refresh_provider_credential(binding.surface, dispatch_key, handle);
                }
                ProviderProbeOutcome::Failure {
                    kind: UsageCoordinationErrorKind::ProviderUnavailable,
                    message: "fake provider refresh reached".to_owned(),
                    retry_at_epoch: None,
                }
            },
        );
        match outcome {
            ProviderProbeOutcome::Failure {
                kind,
                message,
                retry_at_epoch,
            } => {
                assert_eq!(kind, UsageCoordinationErrorKind::ProviderUnavailable);
                assert_eq!(message, INDEPENDENT_CLAUDE_REFRESH_DISABLED);
                assert_eq!(retry_at_epoch, None);
            }
            ProviderProbeOutcome::Success(_) => panic!("Claude refresh unexpectedly succeeded"),
        }
    }

    assert_eq!(provider_refresh_calls.load(Ordering::SeqCst), 0);
    assert_eq!(resolver.resolve_calls.load(Ordering::SeqCst), 0);
    assert_eq!(resolver.refresh_calls.load(Ordering::SeqCst), 0);

    let codex_capability = capability(HostSurfaceId::Codex, "codex-account");
    let codex_executor = executor(
        BTreeMap::from([(
            codex_capability.clone(),
            vec![env_binding(
                HostSurfaceId::Codex,
                "codex-account",
                "OPENAI_API_KEY",
            )],
        )]),
        scope,
        Arc::clone(&resolver),
    );
    let outcome = probe_with_scope_using(
        &codex_executor,
        &codex_capability,
        None,
        move |_, resolver| {
            resolver.refresh_provider_credential(
                HostSurfaceId::Codex,
                "OPENAI_API_KEY",
                &OpaqueCredentialHandle::new("codex-handle"),
            );
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::ProviderUnavailable,
                message: "fake provider failure".to_owned(),
                retry_at_epoch: None,
            }
        },
    );

    assert!(matches!(
        outcome,
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message,
            retry_at_epoch: None,
        } if message == "fake provider failure"
    ));
    assert_eq!(resolver.refresh_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn collector_source_id_and_broker_capability_cannot_cross_authorize() {
    let source_id = "a".repeat(64);
    let capability = crate::source_identity::claude_usage_capability_for_source_id(&source_id);

    assert!(collection_source_authorizes_broker_capability(
        &source_id,
        std::slice::from_ref(&source_id),
        &capability,
    ));

    assert!(!collection_source_authorizes_broker_capability(
        &source_id,
        std::slice::from_ref(&capability.account_id),
        &capability,
    ));

    let confused_capability = UsageAccountCapability {
        surface_id: capability.surface_id.clone(),
        account_id: source_id.clone(),
    };
    assert!(!collection_source_authorizes_broker_capability(
        &source_id,
        std::slice::from_ref(&source_id),
        &confused_capability,
    ));
}
