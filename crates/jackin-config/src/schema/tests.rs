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

fn launch_configurations() -> BTreeMap<String, AgentConfiguration> {
    BTreeMap::from([
        (
            "claude-a".to_owned(),
            AgentConfiguration {
                agent: Agent::Claude,
                account: "a-claude".into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        ),
        (
            "claude-z".to_owned(),
            AgentConfiguration {
                agent: Agent::Claude,
                account: "z-claude".into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        ),
        (
            "claude-out".to_owned(),
            AgentConfiguration {
                agent: Agent::Claude,
                account: "outside".into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        ),
    ])
}

#[test]
fn validate_default_launch_list_accepts_valid_lists() {
    let configurations = launch_configurations();
    let allowlist = vec!["a-claude".to_owned(), "z-claude".to_owned()];
    assert!(
        WorkspaceConfig::validate_default_launch_list(
            &["claude-a".to_owned(), "claude-z".to_owned()],
            Some(&allowlist),
            &configurations,
        )
        .is_empty()
    );
    assert!(
        WorkspaceConfig::validate_default_launch_list(&[], Some(&allowlist), &configurations)
            .is_empty(),
        "an explicit empty list (shell-only) is valid"
    );
}

#[test]
fn validate_default_launch_list_reports_each_invalid_entry() {
    let configurations = launch_configurations();
    let allowlist = vec!["a-claude".to_owned(), "z-claude".to_owned()];
    let errors = WorkspaceConfig::validate_default_launch_list(
        &[
            "claude-a".to_owned(),
            "ghost".to_owned(),
            "claude-a".to_owned(),
            "claude-out".to_owned(),
        ],
        Some(&allowlist),
        &configurations,
    );
    assert_eq!(errors.len(), 3);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("unknown agent configuration")),
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("duplicate launch configuration")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|error| error.contains("not assigned")),
        "{errors:?}"
    );
    // Authorization and admission stay distinct: the unknown id is never
    // reported as unauthorized, and vice versa.
    assert!(
        errors
            .iter()
            .filter(|error| error.contains("ghost"))
            .all(|error| !error.contains("not assigned")),
        "{errors:?}"
    );
}

#[test]
fn validate_default_launch_list_skips_authorization_for_global_scope() {
    let configurations = launch_configurations();
    assert!(
        WorkspaceConfig::validate_default_launch_list(
            &["claude-out".to_owned()],
            None,
            &configurations,
        )
        .is_empty(),
        "global candidates filter by authorization at resolve time instead"
    );
    assert_eq!(
        WorkspaceConfig::validate_default_launch_list(&["ghost".to_owned()], None, &configurations)
            .len(),
        1
    );
}

#[test]
fn validate_default_launch_list_matches_save_time_validation() {
    // Editor previews must agree with `AppConfig::validate_accounts`:
    // whatever it rejects, the surfacing helper flags with the same words.
    use crate::{AccountConfig, AccountCredential};
    use jackin_core::EnvValue;
    let configurations = launch_configurations();
    let allowlist = vec!["a-claude".to_owned()];
    let mut config = crate::AppConfig::default();
    for id in ["a-claude", "z-claude", "outside"] {
        config.accounts.insert(
            id.into(),
            AccountConfig {
                enabled: true,
                name: id.into(),
                provider: crate::AiProvider::Anthropic,
                credential: AccountCredential::ApiKey {
                    value: EnvValue::Plain("test-key".into()),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    config.workspaces.insert(
        "demo".into(),
        WorkspaceConfig {
            workdir: "/demo".into(),
            accounts: allowlist.clone(),
            default_launch: Some(vec!["claude-out".into()]),
            ..Default::default()
        },
    );
    for (id, configuration) in &configurations {
        config
            .agent_configurations
            .insert(id.clone(), configuration.clone());
    }
    let save_error = config.validate_accounts().unwrap_err().to_string();
    let preview = WorkspaceConfig::validate_default_launch_list(
        &["claude-out".to_owned()],
        Some(&allowlist),
        &configurations,
    );
    assert_eq!(preview.len(), 1);
    assert!(
        save_error.contains(&preview[0]),
        "save-time {save_error:?} must contain the preview {preview:?}"
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
