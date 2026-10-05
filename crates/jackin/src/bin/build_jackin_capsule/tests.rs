// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn feature_suffix_is_stable_and_filename_safe() {
    assert_eq!(feature_suffix(&[]), "");
    assert_eq!(
        feature_suffix(&["dhat-heap".to_owned(), "trace/json".to_owned()]),
        "-features-dhat-heap-trace-json"
    );
}

#[test]
fn feature_builds_do_not_overwrite_normal_cache_entry() {
    let cache = PathBuf::from("/tmp/cache");
    let normal = binary_cache_path(&cache, "0.6.0-dev+abc", "arm64", BuildProfile::Release, &[]);
    let dhat = binary_cache_path(
        &cache,
        "0.6.0-dev+abc",
        "arm64",
        BuildProfile::Release,
        &["dhat-heap".to_owned()],
    );

    assert_ne!(normal, dhat);
    assert!(normal.ends_with("jackin-capsule"));
    assert!(dhat.ends_with("jackin-capsule-features-dhat-heap"));
}

#[test]
fn target_directory_uses_cargo_environment_path_rules() {
    let workspace = Path::new("/workspace");

    assert_eq!(target_directory(workspace, None), workspace.join("target"));
    assert_eq!(
        target_directory(workspace, Some(OsStr::new("artifacts"))),
        workspace.join("artifacts")
    );
    assert_eq!(
        target_directory(workspace, Some(OsStr::new("/scratch/jackin-target"))),
        PathBuf::from("/scratch/jackin-target")
    );
}

#[cfg(unix)]
#[test]
fn mbx_zigbuild_invocation_preserves_arguments_cwd_and_exit_status() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace with spaces");
    std::fs::create_dir(&workspace).unwrap();
    let target_dir = workspace.join("target with spaces");
    let capture_args = temp.path().join("captured args.txt");
    let capture_cwd = temp.path().join("captured cwd.txt");
    let fake_mise = temp.path().join("fake mise");
    std::fs::write(
        &fake_mise,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CAPTURE_ARGS\"\npwd > \"$CAPTURE_CWD\"\nexit \"$FAKE_EXIT_STATUS\"\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&fake_mise).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&fake_mise, permissions).unwrap();

    let features = ["dhat-heap".to_owned(), "trace/json".to_owned()];
    let args = mbx_zigbuild_args(
        "x86_64-unknown-linux-gnu.2.17",
        "release",
        &features,
        &target_dir,
    );
    let mut command = mise_command_with_fd_limit(fake_mise.as_os_str(), &args);
    command
        .current_dir(&workspace)
        .env("CAPTURE_ARGS", &capture_args)
        .env("CAPTURE_CWD", &capture_cwd)
        .env("FAKE_EXIT_STATUS", "37");

    let status = command.status().unwrap();

    assert_eq!(status.code(), Some(37));
    let captured_args = std::fs::read_to_string(capture_args).unwrap();
    assert_eq!(
        captured_args.lines().collect::<Vec<_>>(),
        [
            "exec",
            "--",
            "mbx",
            "zigbuild",
            "--profile",
            "release",
            "-p",
            "jackin-capsule",
            "--target",
            "x86_64-unknown-linux-gnu.2.17",
            "--locked",
            "--target-dir",
            target_dir.to_str().unwrap(),
            "--features",
            "dhat-heap,trace/json",
        ]
    );
    assert!(
        std::fs::read_to_string(capture_cwd)
            .unwrap()
            .trim_end()
            .ends_with("workspace with spaces")
    );
}
