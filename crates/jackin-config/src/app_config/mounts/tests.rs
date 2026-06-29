//! Tests for `mounts`.
use super::*;
use jackin_core::RoleSelector;

#[test]
fn deserializes_global_docker_mounts() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "~/.gradle/caches", dst = "/home/agent/.gradle/caches" }
gradle-wrapper = { src = "~/.gradle/wrapper", dst = "/home/agent/.gradle/wrapper", readonly = true }
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let mounts = &config.docker.mounts;
    match mounts.get("gradle-cache").unwrap() {
        MountEntry::Mount(m) => {
            assert_eq!(m.src, "~/.gradle/caches");
            assert_eq!(m.dst, "/home/agent/.gradle/caches");
            assert!(!m.readonly);
        }
        MountEntry::Scoped(_) => panic!("expected MountEntry::Mount"),
    }
    match mounts.get("gradle-wrapper").unwrap() {
        MountEntry::Mount(m) => assert!(m.readonly),
        MountEntry::Scoped(_) => panic!("expected MountEntry::Mount"),
    }
}

#[test]
fn resolve_mounts_collects_global_and_matching_scopes() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
gradle-cache = { src = "/tmp/gradle-caches", dst = "/home/agent/.gradle/caches" }

[docker.mounts."chainargos/*"]
chainargos-secrets = { src = "/tmp/chainargos-secrets", dst = "/secrets", readonly = true }

[docker.mounts."chainargos/agent-brown"]
brown-config = { src = "/tmp/chainargos-brown", dst = "/config" }

[docker.mounts."other/*"]
other-data = { src = "/tmp/other", dst = "/other" }
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "agent-brown");
    let resolved = config.resolve_mounts(&selector);
    assert_eq!(resolved.len(), 3);
    assert!(
        resolved
            .iter()
            .any(|(_, m)| m.dst == "/home/agent/.gradle/caches")
    );
    assert!(
        resolved
            .iter()
            .any(|(_, m)| m.dst == "/secrets" && m.readonly)
    );
    assert!(
        resolved
            .iter()
            .any(|(_, m)| m.dst == "/config" && !m.readonly)
    );
}

#[test]
fn resolve_mounts_exact_overrides_global_with_same_name() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
shared = { src = "/tmp/global", dst = "/data" }

[docker.mounts."chainargos/agent-brown"]
shared = { src = "/tmp/specific", dst = "/data" }
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "agent-brown");
    let resolved = config.resolve_mounts(&selector);
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].1.src, "/tmp/specific");
}

#[test]
fn resolve_mounts_returns_empty_when_no_mounts_configured() {
    let config = AppConfig::default();
    let selector = RoleSelector::new(None, "agent-smith");
    let resolved = config.resolve_mounts(&selector);
    assert!(resolved.is_empty());
}

#[test]
fn validate_mounts_rejects_missing_src() {
    let mounts = vec![(
        "test-mount".to_owned(),
        MountConfig {
            src: "/nonexistent/path/that/does/not/exist".to_owned(),
            dst: "/data".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
    )];
    let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
    assert!(
        err.to_string()
            .contains("/nonexistent/path/that/does/not/exist")
    );
}

#[test]
fn validate_mounts_rejects_relative_src() {
    let mounts = vec![(
        "test-mount".to_owned(),
        MountConfig {
            src: ".".to_owned(),
            dst: "/data".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
    )];

    let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();

    assert!(err.to_string().contains("mount source must be absolute"));
}

#[test]
fn validate_mounts_rejects_relative_dst() {
    let temp = tempfile::tempdir().unwrap();
    let mounts = vec![(
        "test-mount".to_owned(),
        MountConfig {
            src: temp.path().display().to_string(),
            dst: "relative/path".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
    )];
    let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
    assert!(err.to_string().contains("absolute"));
}

#[test]
fn validate_mounts_rejects_duplicate_dst() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().display().to_string();
    let mounts = vec![
        (
            "mount-a".to_owned(),
            MountConfig {
                src: src.clone(),
                dst: "/data".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ),
        (
            "mount-b".to_owned(),
            MountConfig {
                src,
                dst: "/data".to_owned(),
                readonly: true,
                isolation: MountIsolation::Shared,
            },
        ),
    ];
    let err = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap_err();
    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn validate_global_mount_rows_rejects_duplicate_scope_name() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().display().to_string();
    let rows = vec![
        GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: MountConfig {
                src: src.clone(),
                dst: "/a".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        },
        GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: MountConfig {
                src,
                dst: "/b".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        },
    ];

    let err = AppConfig::validate_global_mount_rows(&rows).unwrap_err();

    assert!(
        err.to_string().contains("duplicate global mount entry"),
        "expected duplicate-entry error, got: {err}"
    );
}

#[test]
fn validate_global_mount_rows_rejects_empty_name() {
    let temp = tempfile::tempdir().unwrap();
    let rows = vec![GlobalMountRow {
        scope: None,
        name: "  ".into(),
        mount: MountConfig {
            src: temp.path().display().to_string(),
            dst: "/x".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        },
    }];

    let err = AppConfig::validate_global_mount_rows(&rows).unwrap_err();

    assert!(err.to_string().contains("name cannot be empty"));
}

#[test]
fn validate_global_mount_rows_rejects_overlapping_scope_duplicate_dst() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().display().to_string();
    let rows = vec![
        GlobalMountRow {
            scope: Some("chainargos/*".into()),
            name: "a".into(),
            mount: MountConfig {
                src: src.clone(),
                dst: "/cache".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        },
        GlobalMountRow {
            scope: Some("chainargos/the-architect".into()),
            name: "b".into(),
            mount: MountConfig {
                src,
                dst: "/cache".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        },
    ];

    let err = AppConfig::validate_global_mount_rows(&rows).unwrap_err();

    assert!(err.to_string().contains("duplicate"));
}

#[test]
fn validate_global_mount_rows_allows_disjoint_scope_duplicate_dst() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().display().to_string();
    let rows = vec![
        GlobalMountRow {
            scope: Some("chainargos/*".into()),
            name: "a".into(),
            mount: MountConfig {
                src: src.clone(),
                dst: "/cache".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        },
        GlobalMountRow {
            scope: Some("scentbird/*".into()),
            name: "b".into(),
            mount: MountConfig {
                src,
                dst: "/cache".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        },
    ];

    AppConfig::validate_global_mount_rows(&rows).unwrap();
}

#[test]
fn validate_global_mount_rows_allows_same_name_override_duplicate_dst() {
    let temp = tempfile::tempdir().unwrap();
    let src = temp.path().display().to_string();
    let rows = vec![
        GlobalMountRow {
            scope: None,
            name: "cache".into(),
            mount: MountConfig {
                src: src.clone(),
                dst: "/cache".into(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        },
        GlobalMountRow {
            scope: Some("chainargos/*".into()),
            name: "cache".into(),
            mount: MountConfig {
                src,
                dst: "/cache".into(),
                readonly: true,
                isolation: MountIsolation::Shared,
            },
        },
    ];

    AppConfig::validate_global_mount_rows(&rows).unwrap();
}

#[test]
fn validate_mounts_expands_tilde_in_src() {
    let home = std::env::var("HOME").unwrap();
    let mounts = vec![(
        "home-mount".to_owned(),
        MountConfig {
            src: "~".to_owned(),
            dst: "/home-mount".to_owned(),
            readonly: true,
            isolation: MountIsolation::Shared,
        },
    )];
    let validated = AppConfig::expand_and_validate_named_mounts(&mounts).unwrap();
    assert_eq!(validated[0].src, home);
}

#[test]
fn resolve_mounts_matches_exact_scope_for_unscoped_selector() {
    let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[docker.mounts]
global-data = { src = "/tmp/global", dst = "/global" }

[docker.mounts."agent-smith"]
smith-data = { src = "/tmp/smith", dst = "/smith" }
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let resolved = config.resolve_mounts(&selector);
    assert_eq!(resolved.len(), 2);
    assert!(resolved.iter().any(|(_, m)| m.dst == "/global"));
    assert!(resolved.iter().any(|(_, m)| m.dst == "/smith"));
}

#[test]
fn global_mount_rejects_isolation_field() {
    let toml = r#"src = "/tmp/x"
dst = "/workspace/x"
isolation = "worktree"
"#;
    let err = toml::from_str::<GlobalMountConfig>(toml).unwrap_err();
    assert!(
        err.to_string().contains("isolation") || err.to_string().contains("unknown field"),
        "expected unknown-field error, got: {err}"
    );
}

#[test]
fn global_mount_accepts_minimal_fields() {
    let toml = r#"src = "/tmp/x"
dst = "/workspace/x"
"#;
    let m: GlobalMountConfig = toml::from_str(toml).unwrap();
    assert_eq!(m.src, "/tmp/x");
    assert_eq!(m.dst, "/workspace/x");
    assert!(!m.readonly);
}

#[test]
fn global_mount_accepts_readonly() {
    let toml = r#"src = "/tmp/x"
dst = "/workspace/x"
readonly = true
"#;
    let m: GlobalMountConfig = toml::from_str(toml).unwrap();
    assert!(m.readonly);
}

#[test]
fn wire_path_rejects_isolation_on_global_mount() {
    // Production wire path: AppConfig → DockerMounts → MountEntry
    // (untagged enum) → GlobalMountConfig. Setting `isolation` on
    // a top-level `[docker.mounts]` entry must fail to deserialize.
    // Because `MountEntry` is `#[serde(untagged)]`, the message is
    // the generic "data did not match any variant" rather than
    // the cleaner "unknown field `isolation`" — see the doc
    // comment on `GlobalMountConfig` for the rationale.
    let toml = r#"
[docker.mounts]
gradle-cache = { src = "/tmp/x", dst = "/workspace/x", isolation = "worktree" }
"#;
    let err = toml::from_str::<AppConfig>(toml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("did not match any variant of untagged enum MountEntry"),
        "expected untagged-enum mismatch error, got: {msg}"
    );
}

#[test]
fn validate_global_mount_rows_rejects_non_shared_isolation() {
    let rows = vec![GlobalMountRow {
        scope: None,
        name: "repo".into(),
        mount: MountConfig {
            src: "/tmp/repo".into(),
            dst: "/workspace/repo".into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        },
    }];

    let err = AppConfig::validate_global_mount_rows(&rows).unwrap_err();
    assert!(
        err.to_string().contains("global mounts are always shared"),
        "unexpected error: {err}"
    );
}
