// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) fn empty_membership() -> UsageAccountMembershipV1 {
    use jackin_protocol::usage_broker::{UsageProjectionRefreshStateV2, UsageProjectionSchemaV2};
    UsageAccountMembershipV1::Current {
        projection: Box::new(UsageProjectionV2 {
            schema_version: UsageProjectionSchemaV2,
            projection_id: "projection-1".to_owned(),
            generated_at_epoch: 1,
            discovery_revision: "revision-1".to_owned(),
            broker_instance_id: "broker-1".to_owned(),
            broker_generation: 1,
            refresh_state: UsageProjectionRefreshStateV2::Idle,
            providers: vec![],
            unresolved: vec![],
            unresolved_grants: vec![],
            issues: vec![],
        }),
    }
}

#[test]
fn accepted_empty_membership_is_authoritative() {
    assert!(verify_usage_membership(&empty_membership()).is_empty());
}

#[test]
fn unavailable_and_revoked_membership_never_verify_as_empty() {
    assert_eq!(
        verify_usage_membership(&UsageAccountMembershipV1::Unavailable)[0].status,
        "unavailable"
    );
    assert_eq!(
        verify_usage_membership(&UsageAccountMembershipV1::Revoked)[0].status,
        "revoked"
    );
}

#[test]
fn unresolved_grants_fail_membership_verification() {
    use jackin_protocol::usage_broker::{
        UsageIssueRecoverabilityV2, UsageIssueScopeV2, UsageIssueV2, UsageUnresolvedGrantV2,
    };
    let mut membership = empty_membership();
    let UsageAccountMembershipV1::Current { projection } = &mut membership else {
        unreachable!()
    };
    projection.unresolved_grants.push(UsageUnresolvedGrantV2 {
        configured_account_id: "configured".to_owned(),
        surface_id: "codex".to_owned(),
        issues: vec![UsageIssueV2 {
            code: "source_unavailable".to_owned(),
            scope: UsageIssueScopeV2::Account,
            recoverability: UsageIssueRecoverabilityV2::ActionRequired,
            message: "Configured source unavailable".to_owned(),
            retry_at_epoch: None,
        }],
    });
    assert_eq!(verify_usage_membership(&membership)[0].status, "unresolved");
}

fn account_membership(
    lifecycle: jackin_protocol::usage_broker::UsageLifecycleV2,
) -> UsageAccountMembershipV1 {
    use jackin_protocol::usage_broker::*;
    let mut membership = empty_membership();
    let UsageAccountMembershipV1::Current { projection } = &mut membership else {
        unreachable!()
    };
    let freshness = UsageFreshnessV2 {
        generation: 1,
        phase: UsageFreshnessPhaseV2::Current,
        last_good_at_epoch: Some(1),
        retry_at_epoch: None,
        is_stale: false,
    };
    projection.providers.push(UsageProviderV2 {
        provider_id: "openai".to_owned(),
        display_name: "OpenAI".to_owned(),
        rank: 0,
        membership_state: UsageMembershipStateV2::Current,
        freshness: freshness.clone(),
        accounts: vec![UsageAccountV2 {
            canonical_account_id: "canonical-account".to_owned(),
            refresh_capabilities: vec![],
            identity_kind: UsageIdentityKindV2::ProviderAccountId,
            rank: 0,
            display_label: "Same display label".to_owned(),
            username: None,
            auth_origin: None,
            plan_label: None,
            status_label: None,
            lifecycle,
            freshness,
            provenance_count: 1,
            windows: vec![],
            metric_groups: vec![],
            credential_expires_at_epoch: None,
            issues: vec![],
        }],
        issues: vec![],
    });
    membership
}

#[test]
fn current_identity_without_quota_never_verifies() {
    use jackin_protocol::usage_broker::UsageLifecycleV2;
    let membership = account_membership(UsageLifecycleV2::Available);
    assert_eq!(verify_usage_membership(&membership)[0].status, "untrusted");
}

#[test]
fn configured_unsupported_account_never_verifies() {
    use jackin_protocol::usage_broker::UsageLifecycleV2;
    let membership = account_membership(UsageLifecycleV2::Unsupported);
    assert_eq!(verify_usage_membership(&membership)[0].status, "untrusted");
}

#[test]
fn unresolved_configured_identity_never_verifies() {
    use jackin_protocol::usage_broker::{UsageLifecycleV2, UsageUnresolvedV2};
    let mut membership = empty_membership();
    let UsageAccountMembershipV1::Current { projection } = &mut membership else {
        unreachable!()
    };
    projection.unresolved.push(UsageUnresolvedV2 {
        provider_id: "openai".to_owned(),
        capability_id: "unresolved-capability".to_owned(),
        configuration_count: 1,
        state: UsageLifecycleV2::NeedsLogin,
        issues: vec![],
    });
    assert_eq!(verify_usage_membership(&membership)[0].status, "unresolved");
}

#[test]
fn cache_current_requires_exact_immutable_id_and_current_host_proof() {
    use jackin_usage::usage_snapshot_store::{StoredUsageMembership, UsageMembershipScope};
    let cached = || StoredUsageMembership {
        scope: UsageMembershipScope {
            container_id: "a".repeat(64),
            workspace_config_proof: "host-proof".to_owned(),
        },
        membership: empty_membership(),
    };
    let mut current = cached();
    apply_cached_membership_authority(
        &mut current,
        CachedMembershipAuthority::Validated {
            container_id: "a".repeat(64),
            proof: "host-proof".to_owned(),
        },
    );
    assert!(matches!(
        current.membership,
        UsageAccountMembershipV1::Current { .. }
    ));
    for (container_id, proof) in [
        ("b".repeat(64), "host-proof"),
        ("a".repeat(64), "retired-proof"),
    ] {
        let mut entry = cached();
        apply_cached_membership_authority(
            &mut entry,
            CachedMembershipAuthority::Validated {
                container_id,
                proof: proof.to_owned(),
            },
        );
        assert!(matches!(
            entry.membership,
            UsageAccountMembershipV1::Revoked
        ));
    }
    let mut absent = cached();
    apply_cached_membership_authority(&mut absent, CachedMembershipAuthority::Absent);
    assert!(matches!(
        absent.membership,
        UsageAccountMembershipV1::Revoked
    ));
    let mut unavailable = cached();
    apply_cached_membership_authority(&mut unavailable, CachedMembershipAuthority::Unavailable);
    assert!(matches!(
        unavailable.membership,
        UsageAccountMembershipV1::Unavailable
    ));
}

#[test]
fn trusted_canonical_quota_verifies_independent_of_display_aliases() {
    use jackin_protocol::usage_broker::{
        UsageLifecycleV2, UsageLimitWindowV2, UsagePercent, UsageQuotaStateV2,
        UsageWindowCategoryV2,
    };
    let mut membership = account_membership(UsageLifecycleV2::Available);
    let UsageAccountMembershipV1::Current { projection } = &mut membership else {
        unreachable!()
    };
    projection.providers[0].display_name = "Arbitrary display".to_owned();
    projection.providers[0].accounts[0]
        .windows
        .push(UsageLimitWindowV2 {
            window_id: "session".to_owned(),
            rank: 0,
            category: UsageWindowCategoryV2::Session,
            label: "Session".to_owned(),
            value_label: "37% remaining".to_owned(),
            reset_label: String::new(),
            remaining_percent: Some(UsagePercent::new(37).unwrap()),
            remaining_raw_percent: Some(37),
            used_percent: None,
            used_raw_percent: None,
            reset_at_epoch: None,
            quota_state: UsageQuotaStateV2::Available,
            count_quota: None,
            pace_label: None,
            runs_out_label: None,
        });
    assert_eq!(verify_usage_membership(&membership)[0].status, "ok");
}

#[test]
fn verification_json_and_human_preserve_success_and_failure_semantics() {
    let target = UsageTarget {
        container: "display-container".to_owned(),
        instance_id: None,
    };
    for membership in [empty_membership(), UsageAccountMembershipV1::Unavailable] {
        let mut human = Vec::new();
        let human_result = write_usage_verification(
            &mut human,
            OutputFormat::Human,
            &target,
            &"a".repeat(64),
            &membership,
        );
        let mut json = Vec::new();
        let json_result = write_usage_verification(
            &mut json,
            OutputFormat::Json,
            &target,
            &"a".repeat(64),
            &membership,
        );
        assert_eq!(human_result.is_ok(), json_result.is_ok());
        let report: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(report["data"]["container"], "display-container");
        assert_eq!(report["data"]["scope_id"], "a".repeat(64));
        if matches!(membership, UsageAccountMembershipV1::Current { .. }) {
            assert!(json_result.is_ok());
            assert_eq!(report["data"]["membership"]["state"], "current");
            assert_eq!(report["data"]["checks"], serde_json::json!([]));
            assert!(
                String::from_utf8(human)
                    .unwrap()
                    .contains("verification passed")
            );
        } else {
            assert_eq!(
                human_result.unwrap_err().to_string(),
                json_result.unwrap_err().to_string()
            );
            assert_eq!(report["data"]["membership"]["state"], "unavailable");
            assert_eq!(report["data"]["checks"][0]["status"], "unavailable");
        }
    }
}
