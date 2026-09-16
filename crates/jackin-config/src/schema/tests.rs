// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.to_owned(),
        dst: dst.to_owned(),
        readonly: false,
        isolation: MountIsolation::Shared,
    }
}

#[test]
fn mount_spec_rejects_dot_and_parent_components() {
    for candidate in [
        mount("/host/./repo", "/workspace/repo"),
        mount("/host/../repo", "/workspace/repo"),
        mount("/host/repo", "/workspace/./repo"),
        mount("/host/repo", "/workspace/../repo"),
    ] {
        let err = validate_mount_specs(&[candidate]).unwrap_err();
        assert!(err.to_string().contains("must not contain"), "{err}");
    }
}

#[test]
fn mount_spec_accepts_component_names_containing_dots() {
    validate_mount_specs(&[mount("/host/.../repo", "/workspace/repo..backup")]).unwrap();
}

fn named_mount(name: &str, src: &str, dst: &str) -> (Option<String>, MountConfig) {
    (Some(name.to_owned()), mount(src, dst))
}

#[test]
fn ensure_recreates_missing_cache_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let missing = root.join("global/cargo/git");
    let mounts = vec![named_mount(
        "cargo-git",
        &missing.display().to_string(),
        "/home/agent/.cargo/git",
    )];

    let (kept, report) = ensure_mount_sources(mounts, &[root]);

    assert_eq!(kept.len(), 1);
    assert!(missing.is_dir(), "missing cache dir must be recreated");
    assert_eq!(report.recreated.len(), 1);
    assert_eq!(report.recreated[0].name.as_deref(), Some("cargo-git"));
    assert!(report.skipped.is_empty());
}

#[test]
fn ensure_skips_missing_cache_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let missing = root.join("global/gradle/gradle.properties");
    let mounts = vec![named_mount(
        "gradle-properties",
        &missing.display().to_string(),
        "/home/agent/.gradle/gradle.properties",
    )];

    let (kept, report) = ensure_mount_sources(mounts, &[root]);

    assert!(kept.is_empty(), "missing cache file must be skipped");
    assert!(
        !missing.exists(),
        "an empty file must never be fabricated for a cache file"
    );
    assert!(report.recreated.is_empty());
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].name.as_deref(), Some("gradle-properties"));
}

#[test]
fn ensure_leaves_existing_and_non_cache_mounts_untouched() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("cache");
    let existing = root.join("registry");
    std::fs::create_dir_all(&existing).unwrap();
    let project_missing = temp.path().join("projects/gone");
    let mounts = vec![
        named_mount(
            "registry",
            &existing.display().to_string(),
            "/home/agent/.cargo/registry",
        ),
        (
            None,
            mount(&project_missing.display().to_string(), "/workspace/gone"),
        ),
    ];

    let (kept, report) = ensure_mount_sources(mounts, &[root]);

    assert_eq!(kept.len(), 2);
    assert!(
        !project_missing.exists(),
        "non-cache paths must never be created"
    );
    assert!(report.is_empty());
}

#[test]
fn ensure_falls_through_when_recreation_fails() {
    // A cache root that is a regular file makes `create_dir_all` fail;
    // the mount must be kept so existence validation still reports it.
    let temp = tempfile::tempdir().unwrap();
    let root_file = temp.path().join("cache");
    std::fs::write(&root_file, "not a dir").unwrap();
    let missing = root_file.join("cargo/git");
    let mounts = vec![named_mount(
        "cargo-git",
        &missing.display().to_string(),
        "/home/agent/.cargo/git",
    )];

    let (kept, report) = ensure_mount_sources(mounts, &[root_file]);

    assert_eq!(kept.len(), 1);
    assert!(report.is_empty());
}

#[test]
fn heal_report_notice_lines_name_mounts_and_remediation() {
    let report = MountHealReport {
        recreated: vec![HealedMountSource {
            name: Some("cargo-git".to_owned()),
            src: "/home/op/.cache/jackin/global/cargo/git".to_owned(),
            dst: "/home/agent/.cargo/git".to_owned(),
        }],
        skipped: vec![HealedMountSource {
            name: Some("gradle-properties".to_owned()),
            src: "/home/op/.cache/jackin/global/gradle/gradle.properties".to_owned(),
            dst: "/home/agent/.gradle/gradle.properties".to_owned(),
        }],
    };

    let lines = report.notice_lines();

    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("cargo-git"), "{}", lines[0]);
    assert!(lines[1].contains("warning"), "{}", lines[1]);
    assert!(lines[1].contains("gradle-properties"), "{}", lines[1]);
    assert!(
        lines[1].contains("jackin config mount remove"),
        "{}",
        lines[1]
    );
}

#[test]
fn launch_cache_roots_cover_dot_cache_and_platform_dir() {
    let base = directories::BaseDirs::new().unwrap();
    let roots = launch_cache_roots();

    assert!(
        roots.contains(&base.home_dir().join(".cache")),
        "XDG-style ~/.cache must heal on every platform: {roots:?}"
    );
    assert!(
        roots.contains(&base.cache_dir().to_path_buf()),
        "platform cache dir must heal: {roots:?}"
    );
}
