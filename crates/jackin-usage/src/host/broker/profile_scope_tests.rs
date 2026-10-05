// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Synthetic credential authority tests. Dispatch never invokes a provider.

use super::*;
use crate::host::discovery::ProfileCredentialMaterial;
use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, OpaqueCredentialHandle};
use jackin_core::{Agent, profile_credential_material_revision, profile_credential_source_identity};
use jackin_protocol::usage_broker::UsageProfileSourceProof;
use std::path::Path;

fn profile(account: &str, directory: &str, key: &str) -> ValidatedCredentialBinding {
    let source = profile_credential_source_identity(Agent::Amp, "amp", Path::new(directory), None);
    let payload = serde_json::json!({ "apiKey@https://ampcode.com/": key });
    let material_revision = profile_credential_material_revision(
        Agent::Amp,
        &serde_json::to_vec(&payload).expect("synthetic payload serializes"),
    )
    .expect("synthetic payload is JSON");
    ValidatedCredentialBinding {
        surface: HostSurfaceId::Amp,
        identity: Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Amp,
            subject: CanonicalAccountSubject::ProviderId("fixture-shared-account".to_owned()),
        }),
        source_id: source.descriptor_fingerprint.clone(),
        capability_id: "fixture-shared-capability".to_owned(),
        credential_revision: material_revision.clone(),
        profile_material: Some(ProfileCredentialSourceMaterial {
            source,
            material_revision,
        }),
        provenance: BTreeSet::from([format!("account {account}")]),
        configured_account_ids: BTreeSet::from([account.to_owned()]),
        source: ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Amp {
            key: key.to_owned(),
        }),
    }
}

fn proof(binding: &ValidatedCredentialBinding, account: &str) -> UsageProfileSourceProof {
    let material = binding.profile_material.as_ref().expect("fixture material");
    UsageProfileSourceProof {
        instance_id: format!("instance-{account}"),
        account_id: account.to_owned(),
        surface_id: binding.surface.id().to_owned(),
        source: material.source.clone(),
        material_revision: material.material_revision.clone(),
    }
}

fn scope(proofs: impl IntoIterator<Item = UsageProfileSourceProof>) -> UsageCredentialScope {
    UsageCredentialScope {
        sources: BTreeSet::new(),
        profiles: proofs.into_iter().collect(),
    }
}

fn assert_zero_dispatch(
    bindings: &[ValidatedCredentialBinding],
    surface: &str,
    scope: Option<&UsageCredentialScope>,
) {
    let mut calls = 0;
    let result = dispatch_authorized_binding(bindings, surface, scope, |_| {
        calls += 1;
    });
    assert!(result.is_none(), "forbidden authority was accepted");
    assert_eq!(calls, 0, "forbidden provider dispatch occurred");
}

#[test]
fn exact_selected_profile_alias_dispatches_only_its_material_in_either_order() {
    let a = profile("alias-a", "/fixture/profiles/a", "synthetic-a");
    let b = profile("alias-b", "/fixture/profiles/b", "synthetic-b");
    assert_eq!(a.identity, b.identity);
    assert_eq!(a.capability_id, b.capability_id);
    let selected = scope([proof(&b, "alias-b")]);
    for bindings in [vec![a.clone(), b.clone()], vec![b.clone(), a.clone()]] {
        let mut calls = Vec::new();
        let result = dispatch_authorized_binding(&bindings, "amp", Some(&selected), |binding| {
            let ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Amp { key }) =
                &binding.source
            else {
                panic!("profile dispatch selected another credential kind");
            };
            calls.push((binding.source_id.clone(), key.clone()));
        });
        assert!(result.is_some());
        assert_eq!(calls, vec![(b.source_id.clone(), "synthetic-b".to_owned())]);
    }
}

#[test]
fn missing_or_mismatched_profile_proof_never_dispatches() {
    let b = profile("alias-b", "/fixture/profiles/b", "synthetic-b");
    let exact = proof(&b, "alias-b");
    let mut wrong_account = exact.clone();
    wrong_account.account_id = "unselected-alias".to_owned();
    let mut wrong_surface = exact.clone();
    wrong_surface.surface_id = "codex".to_owned();
    let mut wrong_source = exact.clone();
    wrong_source.source = profile_credential_source_identity(
        Agent::Amp,
        "amp",
        Path::new("/fixture/profiles/other"),
        None,
    );
    let mut wrong_revision = exact;
    wrong_revision.material_revision = profile_credential_material_revision(
        Agent::Amp,
        br#"{"apiKey@https://ampcode.com/":"synthetic-rotated"}"#,
    )
    .expect("synthetic JSON");
    for denied in [
        UsageCredentialScope::default(),
        scope([wrong_account]),
        scope([wrong_surface]),
        scope([wrong_source]),
        scope([wrong_revision]),
    ] {
        assert_zero_dispatch(std::slice::from_ref(&b), "amp", Some(&denied));
    }
    let selected = scope([proof(&b, "alias-b")]);
    assert_zero_dispatch(std::slice::from_ref(&b), "codex", Some(&selected));
}

#[test]
fn selected_profile_cannot_be_replaced_by_same_identity_alias_or_rotated_material() {
    let a = profile("alias-a", "/fixture/profiles/a", "synthetic-a");
    let b = profile("alias-b", "/fixture/profiles/b", "synthetic-b");
    let selected = scope([proof(&b, "alias-b")]);
    assert_zero_dispatch(&[a], "amp", Some(&selected));
    let rotated = profile("alias-b", "/fixture/profiles/b", "synthetic-rotated");
    assert_zero_dispatch(&[rotated], "amp", Some(&selected));
}

#[test]
fn conflicting_or_ambiguous_profile_authorities_never_dispatch() {
    let a = profile("alias-a", "/fixture/profiles/a", "synthetic-a");
    let b = profile("alias-b", "/fixture/profiles/b", "synthetic-b");
    let mut stale_b = proof(&b, "alias-b");
    stale_b.material_revision = "fixture-stale-revision".to_owned();
    for denied in [
        scope([proof(&a, "alias-a"), proof(&b, "alias-b")]),
        scope([proof(&b, "alias-b"), stale_b]),
    ] {
        for bindings in [vec![a.clone(), b.clone()], vec![b.clone(), a.clone()]] {
            assert_zero_dispatch(&bindings, "amp", Some(&denied));
        }
    }
    let selected = scope([proof(&b, "alias-b")]);
    assert_zero_dispatch(&[b.clone(), b], "amp", Some(&selected));
}

#[test]
fn selected_account_and_surface_without_staged_profile_proof_never_authorize_dispatch() {
    let b = profile("alias-b", "/fixture/profiles/b", "synthetic-b");
    let sources = ForwardedUsageSources {
        selected_account_ids: BTreeSet::from(["alias-b".to_owned()]),
        selected_account_surfaces: BTreeMap::from([("alias-b".to_owned(), "amp".to_owned())]),
        env_keys: BTreeSet::new(),
        credential_scope: UsageCredentialScope::default(),
    };
    assert!(!forwarding_requirement(&b).is_forwarded(&sources));
    assert_zero_dispatch(&[b], "amp", Some(&sources.credential_scope));
}

#[test]
fn mixed_profile_and_env_authority_never_dispatches_even_with_exact_profile_proof() {
    let b = profile("alias-b", "/fixture/profiles/b", "synthetic-b");
    let selected = scope([proof(&b, "alias-b")]);
    let mut env = b.clone();
    env.profile_material = None;
    env.source_id = "fixture-env-source".to_owned();
    env.source = ValidatedCredentialSource::Env {
        handle: OpaqueCredentialHandle::new("fixture-env-handle"),
        key: "AMP_API_KEY".to_owned(),
        dispatch_key: "AMP_API_KEY".to_owned(),
        launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
        material: None,
    };
    for bindings in [vec![b.clone(), env.clone()], vec![env, b]] {
        assert_zero_dispatch(&bindings, "amp", Some(&selected));
    }
}

#[test]
fn unscoped_distinct_profile_aliases_never_dispatch() {
    let a = profile("alias-a", "/fixture/profiles/a", "synthetic-a");
    let b = profile("alias-b", "/fixture/profiles/b", "synthetic-b");
    for bindings in [vec![a.clone(), b.clone()], vec![b, a]] {
        assert_zero_dispatch(&bindings, "amp", None);
    }
}

#[test]
fn sole_host_antigravity_grant_dispatches_unscoped_but_never_with_empty_launch_proof() {
    let mut grant = profile("fixture-antigravity", "/fixture/antigravity", "unused-fixture");
    grant.surface = HostSurfaceId::Antigravity;
    grant.identity = None;
    grant.profile_material = None;
    grant.source = ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Antigravity);
    let mut calls = 0;
    let result = dispatch_authorized_binding(
        std::slice::from_ref(&grant),
        grant.surface.id(),
        None,
        |binding| {
            assert!(matches!(
                &binding.source,
                ValidatedCredentialSource::Profile(ProfileCredentialMaterial::Antigravity)
            ));
            calls += 1;
        },
    );
    assert!(result.is_some());
    assert_eq!(calls, 1);
    assert_zero_dispatch(
        std::slice::from_ref(&grant),
        grant.surface.id(),
        Some(&UsageCredentialScope::default()),
    );
}
