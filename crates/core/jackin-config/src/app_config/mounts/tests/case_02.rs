// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
