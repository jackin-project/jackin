use jackin_config::{
    AppConfig, GlobalMountConfig, GlobalMountRow, MountConfig, MountEntry, MountIsolation,
};

use super::{
    current_dir_mount_config, global_mount_scope_value, global_rows_have_sensitive_mount,
    prospective_workspace_mounts, shared_mount_config, unique_global_mount_name,
    unscoped_global_mounts,
};

fn mount(src: &str, dst: &str) -> MountConfig {
    MountConfig {
        src: src.into(),
        dst: dst.into(),
        readonly: false,
        isolation: MountIsolation::Shared,
    }
}

#[test]
fn shared_mount_helpers_build_shared_mounts() {
    assert_eq!(
        current_dir_mount_config("/work"),
        MountConfig {
            src: "/work".into(),
            dst: "/work".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }
    );
    assert_eq!(
        shared_mount_config("/host", "/container", true),
        MountConfig {
            src: "/host".into(),
            dst: "/container".into(),
            readonly: true,
            isolation: MountIsolation::Shared,
        }
    );
}

#[test]
fn prospective_workspace_mounts_matches_edit_merge_order() {
    let current = vec![mount("/old", "/work"), mount("/keep", "/keep")];
    let pending = vec![mount("/new", "/work"), mount("/added", "/added")];
    let out = prospective_workspace_mounts(&current, &pending, &["/keep".into()]);

    assert_eq!(out, vec![mount("/new", "/work"), mount("/added", "/added")]);
}

#[test]
fn unscoped_global_mounts_ignores_scoped_mount_tables() {
    let temp = tempfile::tempdir().unwrap();
    let shared = temp.path().join("shared");
    let private = temp.path().join("private");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::create_dir_all(&private).unwrap();

    let mut config = AppConfig::default();
    config.docker.mounts.insert(
        "shared".into(),
        MountEntry::Mount(GlobalMountConfig {
            src: shared.display().to_string(),
            dst: "/container/shared".into(),
            readonly: true,
        }),
    );
    config.docker.mounts.insert(
        "agent-smith".into(),
        MountEntry::Scoped(std::collections::BTreeMap::from([(
            "private".into(),
            GlobalMountConfig {
                src: private.display().to_string(),
                dst: "/container/private".into(),
                readonly: false,
            },
        )])),
    );

    let mounts = unscoped_global_mounts(&config).unwrap();

    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].src, shared.display().to_string());
    assert_eq!(mounts[0].dst, "/container/shared");
    assert!(mounts[0].readonly);
}

#[test]
fn global_mount_helpers_normalize_scope_name_and_sensitive_rows() {
    assert_eq!(global_mount_scope_value(""), None);
    assert_eq!(
        global_mount_scope_value("workspace"),
        Some("workspace".to_owned())
    );

    let rows = vec![
        GlobalMountRow {
            scope: None,
            name: "ssh".into(),
            mount: mount("/home/user/.ssh", "/ssh"),
        },
        GlobalMountRow {
            scope: Some("workspace".into()),
            name: "Project Data".into(),
            mount: mount("/data/project", "/Project Data"),
        },
        GlobalMountRow {
            scope: Some("workspace".into()),
            name: "Project-Data".into(),
            mount: mount("/data/project-2", "/Project Data"),
        },
    ];

    assert!(global_rows_have_sensitive_mount(&rows));
    assert_eq!(
        unique_global_mount_name(&rows, Some("workspace"), "/Project Data"),
        "Project-Data-2"
    );
    assert_eq!(
        unique_global_mount_name(&rows, Some("other"), "/Project Data"),
        "Project-Data"
    );
}
