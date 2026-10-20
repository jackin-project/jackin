// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

mod support;

use super::*;
use serde_json::{Value, json};
use support::*;

#[tokio::test]
async fn publication_success_does_not_wait_for_downstream_tap() {
    for channel in [
        PackageReleaseChannel::Preview,
        PackageReleaseChannel::Stable,
    ] {
        let fixture = Fixture::new(channel);
        let pin = fixture.pin().await.unwrap();
        pin.recheck(&fixture).await.unwrap();
        let route = route(&format!(
            "actions/runs/{}/attempts/{}",
            fixture.receipt.workflow.run_id, fixture.receipt.workflow.run_attempt
        ));
        fixture.change(&route, |run| {
            run["status"] = json!("completed");
            run["conclusion"] = json!("failure");
        });
        // The authenticated signed generation and complete publication remain authoritative
        // even when downstream promotion fails after the assets have been committed.
        pin.recheck(&fixture).await.unwrap();
    }
}

#[tokio::test]
async fn incomplete_or_failed_signer_is_never_publication_authority() {
    for (status, conclusion) in [
        ("in_progress", Value::Null),
        ("completed", json!("failure")),
        ("completed", json!("skipped")),
    ] {
        let fixture = Fixture::new(PackageReleaseChannel::Preview);
        fixture.change(&jobs_route(&fixture.receipt), |rows| {
            let job = rows["jobs"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|j| j["name"] == fixture.receipt.channel.signer_job_id())
                .unwrap();
            job["status"] = json!(status);
            job["conclusion"] = conclusion;
        });
        assert!(
            fixture
                .pin()
                .await
                .unwrap_err()
                .to_string()
                .contains("did not succeed")
        );
    }
}

#[tokio::test]
async fn committed_recovery_does_not_require_the_original_publisher() {
    for channel in [
        PackageReleaseChannel::Preview,
        PackageReleaseChannel::Stable,
    ] {
        for omit in [false, true] {
            let fixture = Fixture::new(channel);
            fixture.fail_or_remove_job("Publish authenticated channel release", omit);
            fixture.change(
                &route(&format!(
                    "actions/runs/{}/attempts/{}",
                    fixture.receipt.workflow.run_id, fixture.receipt.workflow.run_attempt
                )),
                |run| {
                    run["status"] = json!("completed");
                    run["conclusion"] = json!("failure");
                },
            );
            let pin = fixture.pin().await.unwrap();
            pin.recheck(&fixture).await.unwrap();
        }
    }
}

#[test]
fn publisher_fixtures_use_generated_display_names() {
    for workflow in [
        include_str!("../../../../../../.github/workflows/binary-preview.yml"),
        include_str!("../../../../../../.github/workflows/binary-release.yml"),
    ] {
        assert!(workflow.contains("    name: Publish authenticated channel release\n"));
    }
}

#[tokio::test]
async fn producer_and_source_ci_identity_fail_closed() {
    for field in ["head_sha", "path", "event", "head_branch"] {
        let fixture = Fixture::new(PackageReleaseChannel::Preview);
        let url = route(&format!(
            "actions/runs/{}/attempts/{}",
            fixture.receipt.workflow.run_id, fixture.receipt.workflow.run_attempt
        ));
        fixture.change(&url, |run| run[field] = json!("untrusted"));
        assert!(fixture.pin().await.is_err(), "{field}");
    }
    let fixture = Fixture::new(PackageReleaseChannel::Preview);
    let url = route(&format!(
        "actions/runs/{}/attempts/{}",
        fixture.receipt.source.ci_run_id, fixture.receipt.source.ci_run_attempt
    ));
    fixture.change(&url, |run| run["conclusion"] = json!("failure"));
    assert!(
        fixture
            .pin()
            .await
            .unwrap_err()
            .to_string()
            .contains("source CI")
    );
}

#[tokio::test]
async fn exact_successful_job_inventory_is_required() {
    for mutation in 0..4 {
        let fixture = Fixture::new(PackageReleaseChannel::Preview);
        fixture.change(&jobs_route(&fixture.receipt), |rows| {
            let jobs = rows["jobs"].as_array_mut().unwrap();
            match mutation {
                0 => jobs[0]["conclusion"] = json!("failure"),
                1 => jobs[1]["id"] = jobs[0]["id"].clone(),
                2 => jobs[1]["name"] = jobs[0]["name"].clone(),
                _ => jobs[3]["head_sha"] = json!("f".repeat(40)),
            }
        });
        assert!(fixture.pin().await.is_err(), "mutation {mutation}");
    }
}

#[tokio::test]
async fn channel_lifecycle_and_tag_are_not_interchangeable() {
    for channel in [
        PackageReleaseChannel::Preview,
        PackageReleaseChannel::Stable,
    ] {
        for field in ["draft", "immutable", "prerelease"] {
            let fixture = Fixture::new(channel);
            fixture.change(&release_route(&fixture.receipt), |release| {
                release[field] = Value::Null;
            });
            assert!(fixture.pin().await.is_err(), "{channel:?}/{field}");
        }
        let fixture = Fixture::new(channel);
        fixture.change(&tag_route(&fixture.receipt), |tag| {
            tag["object"]["sha"] = json!("f".repeat(40));
        });
        fixture.pin().await.unwrap_err();
    }
}

#[tokio::test]
async fn rolling_requires_exact_committed_journal_and_live_ids() {
    for mutation in 0..6 {
        let fixture = Fixture::new(PackageReleaseChannel::Preview);
        fixture.fail_or_remove_job("Publish authenticated channel release", false);
        fixture.change(&release_route(&fixture.receipt), |release| {
            let body = release["body"]
                .as_str()
                .unwrap()
                .strip_prefix("velnor-rolling-preview-v1\n")
                .unwrap();
            let mut envelope: Value = serde_json::from_str(body).unwrap();
            match mutation {
                0 => envelope["journal"]["phase"] = json!("staging"),
                1 => envelope["journal"]["identity"]["receipt_digest"] = json!("f".repeat(64)),
                2 => envelope["journal"]["original"]["run_id"] = json!(1),
                3 => envelope["predecessor"] = json!({}),
                4 => {
                    envelope["journal"]["staged"]
                        .as_object_mut()
                        .unwrap()
                        .values_mut()
                        .next()
                        .unwrap()["id"] = json!(999);
                }
                _ => release["id"] = json!(123),
            }
            release["body"] = json!(format!(
                "velnor-rolling-preview-v1\n{}",
                String::from_utf8(
                    velnor_actions_contract::canonical_json_bytes(&envelope).unwrap()
                )
                .unwrap()
            ));
        });
        assert!(fixture.pin().await.is_err(), "mutation {mutation}");
    }
}

#[tokio::test]
async fn retained_publication_rechecks_receipt_bundle_and_inventory() {
    for mutation in 0..4 {
        let mut fixture = Fixture::new(PackageReleaseChannel::Preview);
        let pin = fixture.pin().await.unwrap();
        match mutation {
            0 => fixture.raw.push(b' '),
            1 => fixture.bundle.push(b' '),
            2 => fixture.change(
                &route(&format!(
                    "releases/{}/assets?per_page=100&page=1",
                    policy::PREVIEW_RELEASE_ID
                )),
                |rows| rows[0]["id"] = json!(999),
            ),
            _ => fixture.change(&tag_route(&fixture.receipt), |tag| {
                tag["object"]["sha"] = json!("f".repeat(40));
            }),
        }
        assert!(pin.recheck(&fixture).await.is_err(), "mutation {mutation}");
    }
}

#[tokio::test]
async fn unknown_missing_and_incomplete_assets_fail_closed() {
    for mutation in 0..5 {
        let fixture = Fixture::new(PackageReleaseChannel::Preview);
        fixture.fail_or_remove_job("Publish authenticated channel release", false);
        let url = route(&format!(
            "releases/{}/assets?per_page=100&page=1",
            policy::PREVIEW_RELEASE_ID
        ));
        fixture.change(&url, |rows| match mutation {
            0 => {
                rows.as_array_mut().unwrap().pop();
            }
            1 => {
                let extra = rows[0].clone();
                rows.as_array_mut().unwrap().push(extra);
            }
            2 => rows[0]["size"] = json!(1),
            3 => rows[0]["state"] = json!("starter"),
            _ => rows[0]["digest"] = json!(format!("sha256:{}", "f".repeat(64))),
        });
        assert!(fixture.pin().await.is_err(), "mutation {mutation}");
    }
}

struct ChangingRelease<'a> {
    fixture: &'a Fixture,
    reads: std::sync::atomic::AtomicUsize,
}

impl Transport for ChangingRelease<'_> {
    async fn get(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        use std::sync::atomic::Ordering;
        if url == release_route(&self.fixture.receipt)
            && self.reads.fetch_add(1, Ordering::SeqCst) > 0
        {
            self.fixture.change(url, |release| {
                release["body"] = json!("replacement generation");
            });
        }
        self.fixture.get(url, limit).await
    }
}

#[tokio::test]
async fn publication_change_during_observation_is_rejected() {
    let fixture = Fixture::new(PackageReleaseChannel::Preview);
    let transport = ChangingRelease {
        fixture: &fixture,
        reads: std::sync::atomic::AtomicUsize::new(0),
    };
    let error = pin(&transport, &fixture.receipt, &fixture.raw, &fixture.bundle)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("changed during observation"));
}

#[tokio::test]
async fn every_original_payload_assembly_and_signer_must_succeed_in_both_channels() {
    for channel in [
        PackageReleaseChannel::Preview,
        PackageReleaseChannel::Stable,
    ] {
        let baseline = Fixture::new(channel);
        baseline.pin().await.unwrap();
        let names = [
            channel.assemble_job_id().to_owned(),
            channel.signer_job_id().to_owned(),
        ]
        .into_iter()
        .chain(baseline.receipt.products.iter().flat_map(|product| {
            product
                .payloads
                .iter()
                .map(|payload| format!("Build {} / {}", product.id, payload.target))
        }))
        .collect::<Vec<_>>();
        assert_eq!(names.len(), 8);
        for name in names {
            for omit in [false, true] {
                let fixture = Fixture::new(channel);
                fixture.fail_or_remove_job(&name, omit);
                let error = fixture.pin().await.unwrap_err().to_string();
                assert!(
                    error.contains("missing authenticated release job")
                        || error.contains("did not succeed"),
                    "{channel:?}/{name}/{omit}: {error}"
                );
            }
        }
    }
}
