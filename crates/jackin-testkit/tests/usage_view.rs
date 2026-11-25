// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Clock;
use jackin_testkit::{
    FakeClock, TestResult, USAGE_VIEW_NOW, USAGE_VIEW_OFFSET, pin_usage_view_clock, seed_usage_view,
};
use jackin_usage::accounts::{AccountRegistry, DATABASE_PATH};

const FIXTURE: &str = include_str!("fixtures/usage_view.json");

#[test]
fn the_fixture_holds_anonymized_labels_and_no_credentials() {
    let mut domains = FIXTURE.split('@').skip(1);
    assert!(domains.all(|rest| rest.starts_with("example.com\"")));
    assert_eq!(FIXTURE.matches('@').count(), 8);
    for forbidden in [
        "Bearer",
        "sk-",
        "access_token",
        "refresh_token",
        "api_key",
        "password",
    ] {
        assert!(!FIXTURE.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn the_seeded_registry_lists_every_fixture_account_with_its_failure() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(DATABASE_PATH);
    let seeded = seed_usage_view(&path)?;
    assert_eq!(seeded.len(), 14);
    let registry =
        AccountRegistry::open_read_only_with_vault(&path, jackin_testkit::MemoryVault::shared())?
            .ok_or("registry")?;
    let stored: Vec<_> = registry.snapshots()?;
    assert_eq!(stored.len(), 14);
    for ((account, fetched_at, snapshot), seeded) in stored.iter().zip(&seeded) {
        assert_eq!(account.id, seeded.id);
        assert!(fetched_at.is_some() && snapshot.is_some());
    }
    let failing: Vec<_> = seeded
        .iter()
        .filter(|account| {
            registry
                .failure(account.id)
                .is_ok_and(|failure| failure.is_some())
        })
        .collect();
    assert_eq!(failing.len(), 1);
    let failure = registry.failure(failing[0].id)?.ok_or("failure")?;
    assert_eq!(failure.message, "HTTP 403 SERVICE_DISABLED");
    assert_eq!(failure.retry_at, 1_795_447_730);
    Ok(())
}

#[test]
fn the_pinned_clock_reads_the_fixture_time_in_the_fixture_zone() {
    let clock = FakeClock::new();
    pin_usage_view_clock(&clock);
    assert_eq!(clock.now().0, USAGE_VIEW_NOW);
    assert_eq!(clock.utc_offset_secs(), USAGE_VIEW_OFFSET);
    pin_usage_view_clock(&clock);
    assert_eq!(clock.now().0, USAGE_VIEW_NOW);
}
