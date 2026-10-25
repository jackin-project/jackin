// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, SystemTime};

use crate::{ManualClock, normalize_snapshot_text, redact_digit_runs, seed_valid_role_repo};

#[test]
fn seeded_role_repo_contains_manifest_and_dockerfile() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    seed_valid_role_repo(temp.path());
    assert!(temp.path().join(".git").is_dir());
    let dockerfile = std::fs::read_to_string(temp.path().join("Dockerfile"))?;
    assert!(dockerfile.contains("FROM"));
    let manifest = std::fs::read_to_string(temp.path().join("jackin.role.toml"))?;
    assert!(manifest.contains("v1alpha3"));
    Ok(())
}

#[test]
fn digit_runs_redact_only_at_threshold() {
    assert_eq!(redact_digit_runs("job 123 done"), "job 123 done");
    assert_eq!(redact_digit_runs("token 1234567"), "token <digits>");
}

#[test]
fn snapshots_trim_trailing_whitespace_and_blank_tail() {
    assert_eq!(
        normalize_snapshot_text("alpha  \nbeta\n\n"),
        "alpha\nbeta\n"
    );
    assert_eq!(normalize_snapshot_text(""), "");
}

#[test]
fn manual_clock_advances_and_reports_due() -> anyhow::Result<()> {
    let clock = ManualClock::new(SystemTime::UNIX_EPOCH);
    let advanced = clock.advance(Duration::from_secs(5))?;
    assert_eq!(advanced, SystemTime::UNIX_EPOCH + Duration::from_secs(5));
    assert_eq!(
        clock.elapsed_since(SystemTime::UNIX_EPOCH),
        Duration::from_secs(5)
    );
    assert!(clock.is_due(SystemTime::UNIX_EPOCH, Duration::from_secs(5)));
    assert!(!clock.is_due(SystemTime::UNIX_EPOCH, Duration::from_secs(6)));
    Ok(())
}
