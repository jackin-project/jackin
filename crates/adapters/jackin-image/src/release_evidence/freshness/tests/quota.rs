// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn historical_admission_is_once_per_pin_with_mutable_checks_at_both_boundaries() {
    for (channel, expected) in [
        // Preview admits A <= B <= main once; stable has A == B and checks B <= main.
        (PackageReleaseChannel::Preview, 27),
        (PackageReleaseChannel::Stable, 26),
    ] {
        let fixture = Fixture::new(channel);
        let pin = fixture.pin().await.unwrap();
        pin.recheck(&fixture).await.unwrap();
        pin.recheck(&fixture).await.unwrap();
        let requests = fixture.requests.lock().unwrap();
        assert_eq!(
            requests.iter().filter(|url| url.starts_with(API)).count(),
            expected
        );
        assert_eq!(requests.iter().filter(|url| url.as_str() == API).count(), 1);
        assert_eq!(
            requests
                .iter()
                .filter(|url| url.contains("/actions/runs/"))
                .count(),
            3
        );
        assert_eq!(
            requests
                .iter()
                .filter(|url| **url == release_route(&fixture.receipt))
                .count(),
            6
        );
        assert_eq!(
            requests
                .iter()
                .filter(|url| url.contains("/assets?"))
                .count(),
            3
        );
        assert_eq!(
            requests
                .iter()
                .filter(|url| **url == tag_route(&fixture.receipt))
                .count(),
            6
        );
    }
}

#[tokio::test]
async fn failed_historical_admission_is_not_reused_by_a_retry() {
    let fixture = Fixture::new(PackageReleaseChannel::Preview);
    let ci = route(&format!(
        "actions/runs/{}/attempts/{}",
        fixture.receipt.source.ci_run_id, fixture.receipt.source.ci_run_attempt
    ));
    fixture.change(&ci, |run| run["conclusion"] = json!("failure"));
    fixture.pin().await.unwrap_err();
    fixture.change(&ci, |run| run["conclusion"] = json!("success"));
    fixture.pin().await.unwrap();
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|url| **url == ci)
            .count(),
        2
    );
}

#[tokio::test]
async fn each_recheck_boundary_rejects_lifecycle_inventory_and_tag_changes() {
    for channel in [
        PackageReleaseChannel::Preview,
        PackageReleaseChannel::Stable,
    ] {
        for boundary in 0..2 {
            for mutation in 0..3 {
                reject_mutated_publication(channel, boundary, mutation).await;
            }
        }
    }
}

async fn reject_mutated_publication(
    channel: PackageReleaseChannel,
    boundary: usize,
    mutation: usize,
) {
    let fixture = Fixture::new(channel);
    let pin = fixture.pin().await.unwrap();
    if boundary == 1 {
        pin.recheck(&fixture).await.unwrap();
    }
    match mutation {
        0 => fixture.change(&release_route(&fixture.receipt), |r| {
            r["draft"] = json!(true);
        }),
        1 => fixture.change(&assets_route(), |rows| rows[0]["id"] = json!(999)),
        _ => fixture.change(&tag_route(&fixture.receipt), |r| {
            r["object"]["sha"] = json!("f".repeat(40));
        }),
    }
    assert!(
        pin.recheck(&fixture).await.is_err(),
        "{channel:?}/{boundary}/{mutation}"
    );
}

#[tokio::test]
async fn successful_pin_is_private_and_a_new_pin_readmits_repository_and_history() {
    for historical in [false, true] {
        let fixture = Fixture::new(PackageReleaseChannel::Preview);
        let pin = fixture.pin().await.unwrap();
        if historical {
            fixture.change(&jobs_route(&fixture.receipt), |rows| {
                rows["jobs"][0]["conclusion"] = json!("failure");
            });
        } else {
            fixture.change(API, |repository| repository["private"] = json!(true));
        }
        pin.recheck(&fixture).await.unwrap();
        fixture.pin().await.unwrap_err();
    }
}

#[tokio::test]
async fn recovery_envelope_and_signed_bytes_are_fresh_at_each_boundary() {
    for boundary in 0..2 {
        for mutation in 0..3 {
            let mut fixture = Fixture::new(PackageReleaseChannel::Preview);
            let pin = fixture.pin().await.unwrap();
            if boundary == 1 {
                pin.recheck(&fixture).await.unwrap();
            }
            match mutation {
                0 => fixture.change(&release_route(&fixture.receipt), |release| {
                    release["body"] = json!("uncommitted replacement");
                }),
                1 => fixture.raw.push(b' '),
                _ => fixture.bundle.push(b' '),
            }
            assert!(
                pin.recheck(&fixture).await.is_err(),
                "{boundary}/{mutation}"
            );
        }
    }
}
