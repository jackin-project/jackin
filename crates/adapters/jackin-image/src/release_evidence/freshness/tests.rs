// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

mod quota;
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
async fn preview_requires_its_immutable_source_release_and_annotated_tag() {
    for mutation in 0..7 {
        let fixture = Fixture::new(PackageReleaseChannel::Preview);
        fixture.fail_or_remove_job("Publish authenticated channel release", false);
        match mutation {
            0 => fixture.change(&release_route(&fixture.receipt), |release| {
                release["immutable"] = json!(false);
            }),
            1 => fixture.change(&release_route(&fixture.receipt), |release| {
                release["prerelease"] = json!(false);
            }),
            2 => fixture.change(&release_route(&fixture.receipt), |release| {
                release["body"] = json!(format!("{}\n", release["body"].as_str().unwrap()));
            }),
            3 => fixture.change(&annotated_tag_route(), |tag| {
                tag["message"] = json!("velnor-package-release-recovery-v1\n");
            }),
            4 => fixture.change(&tag_route(&fixture.receipt), |tag| {
                // A lightweight tag at the right commit is not the published generation.
                tag["object"] = json!({"sha":fixture.receipt.source.commit_sha,"type":"commit"});
            }),
            5 => fixture.change(&annotated_tag_route(), |tag| {
                tag["object"]["sha"] = json!(fixture.receipt.workflow.authority_sha);
            }),
            _ => fixture.change(&annotated_tag_route(), |tag| {
                tag["tag"] = json!("preview");
            }),
        }
        assert!(fixture.pin().await.is_err(), "mutation {mutation}");
    }
}

#[tokio::test]
async fn preview_source_and_workflow_authority_are_distinct_identities() {
    let fixture = Fixture::new(PackageReleaseChannel::Preview);
    let (source, authority) = (
        fixture.receipt.source.commit_sha.clone(),
        fixture.receipt.workflow.authority_sha.clone(),
    );
    assert_ne!(source, authority);
    fixture.pin().await.unwrap();
    for (route, sha) in [
        (
            route(&format!(
                "actions/runs/{}/attempts/{}",
                fixture.receipt.workflow.run_id, fixture.receipt.workflow.run_attempt
            )),
            &source,
        ),
        (
            route(&format!(
                "actions/runs/{}/attempts/{}",
                fixture.receipt.source.ci_run_id, fixture.receipt.source.ci_run_attempt
            )),
            &authority,
        ),
    ] {
        let fixture = Fixture::new(PackageReleaseChannel::Preview);
        fixture.change(&route, |run| run["head_sha"] = json!(sha));
        assert!(fixture.pin().await.is_err(), "{route}");
    }
    let fixture = Fixture::new(PackageReleaseChannel::Preview);
    fixture.change(&jobs_route(&fixture.receipt), |rows| {
        rows["jobs"][0]["head_sha"] = json!(source);
    });
    fixture.pin().await.unwrap_err();
}

#[tokio::test]
async fn preview_source_must_precede_its_authority_on_protected_main() {
    for (base_is_source, mutation) in [true, false]
        .into_iter()
        .flat_map(|first| (0..5).map(move |m| (first, m)))
    {
        let fixture = Fixture::new(PackageReleaseChannel::Preview);
        let receipt = &fixture.receipt;
        let route = if base_is_source {
            compare_route(&receipt.source.commit_sha, &receipt.workflow.authority_sha)
        } else {
            compare_route(&receipt.workflow.authority_sha, "main")
        };
        fixture.change(&route, |comparison| match mutation {
            0 => comparison["status"] = json!("diverged"),
            1 => comparison["status"] = json!("behind"),
            2 => comparison["behind_by"] = json!(1),
            3 => comparison["merge_base_commit"]["sha"] = json!("f".repeat(40)),
            _ => {
                comparison["status"] = json!("identical");
            }
        });
        let error = fixture.pin().await.unwrap_err().to_string();
        assert!(
            error.contains("not an ancestor"),
            "{base_is_source}/{mutation}: {error}"
        );
    }
}

#[tokio::test]
async fn stable_needs_no_source_to_authority_comparison() {
    let fixture = Fixture::new(PackageReleaseChannel::Stable);
    fixture.pin().await.unwrap();
    let requests = fixture.requests.lock().unwrap();
    let compares: Vec<_> = requests
        .iter()
        .filter(|url| url.contains("/compare/"))
        .collect();
    assert_eq!(
        compares,
        [&compare_route(
            &fixture.receipt.workflow.authority_sha,
            "main"
        )]
    );
}

#[tokio::test]
async fn retained_publication_rechecks_receipt_bundle_and_inventory() {
    for mutation in 0..4 {
        let mut fixture = Fixture::new(PackageReleaseChannel::Preview);
        let pin = fixture.pin().await.unwrap();
        match mutation {
            0 => fixture.raw.push(b' '),
            1 => fixture.bundle.push(b' '),
            2 => fixture.change(&assets_route(), |rows| rows[0]["id"] = json!(999)),
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
        fixture.change(&assets_route(), |rows| match mutation {
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
