// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn jackin_owns_every_release_target() {
    assert_eq!(targets(ArchivePackage::Jackin), &JACKIN_TARGETS);
    assert_eq!(
        binaries(ArchivePackage::Jackin),
        &["jackin", "jackin-role", "jackin-usage-broker"]
    );
}

#[test]
fn capsule_owns_complete_linux_target_set() {
    assert_eq!(targets(ArchivePackage::JackinCapsule), &CAPSULE_TARGETS);
    assert_eq!(binaries(ArchivePackage::JackinCapsule), &["jackin-capsule"]);
}

#[test]
fn sidecars_preserve_archive_extension() {
    assert_eq!(
        sidecar(Path::new("dist/jackin-linux.tar.gz"), "sha256"),
        PathBuf::from("dist/jackin-linux.tar.gz.sha256")
    );
}

#[test]
fn target_builds_share_runner_capacity() {
    assert_eq!(jobs_per_target_for(4, 4), 1);
    assert_eq!(jobs_per_target_for(8, 4), 2);
    assert_eq!(jobs_per_target_for(2, 4), 1);
}

#[test]
fn archives_are_deterministic_and_contain_only_executable_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let target = "aarch64-apple-darwin";
    let release = temp.path().join(target).join("release");
    fs::create_dir_all(&release).unwrap();
    for name in binaries(ArchivePackage::Jackin) {
        fs::write(release.join(name), name.as_bytes()).unwrap();
    }
    let layout = ArchiveLayout::new(temp.path(), target, ArchiveProfile::Release).unwrap();
    let path = package(ArchivePackage::Jackin, &layout, temp.path()).unwrap();
    let first = fs::read(&path).unwrap();
    let path = package(ArchivePackage::Jackin, &layout, temp.path()).unwrap();
    assert_eq!(first, fs::read(path).unwrap());
    let gzip = flate2::read::GzDecoder::new(first.as_slice());
    let mut archive = tar::Archive::new(gzip);
    let mut names = Vec::new();
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        assert_eq!(entry.header().mode().unwrap(), 0o755);
        assert_eq!(entry.header().mtime().unwrap(), 0);
        assert_eq!(entry.header().uid().unwrap(), 0);
        assert_eq!(entry.header().gid().unwrap(), 0);
        assert!(entry.header().entry_type().is_file());
        names.push(entry.path().unwrap().to_string_lossy().into_owned());
    }
    assert_eq!(names, binaries(ArchivePackage::Jackin));
}

#[test]
fn build_commands_select_only_the_declared_product_binaries() {
    let jackin = build_arguments(
        ArchivePackage::Jackin,
        JACKIN_TARGETS[0],
        ArchiveProfile::Release,
    );
    assert_eq!(&jackin[4..6], &["-p", "jackin"]);
    assert_eq!(
        jackin
            .windows(2)
            .filter(|pair| pair[0] == "--bin")
            .map(|pair| pair[1])
            .collect::<Vec<_>>(),
        ["jackin", "jackin-role", "jackin-usage-broker"]
    );
    assert!(!jackin.contains(&"build-jackin-capsule"));
    assert!(jackin.contains(&"--message-format=json-render-diagnostics"));
    let capsule = build_arguments(
        ArchivePackage::JackinCapsule,
        CAPSULE_TARGETS[0],
        ArchiveProfile::Release,
    );
    assert_eq!(
        &capsule[4..8],
        &["-p", "jackin-capsule", "--bin", "jackin-capsule"]
    );
    assert!(capsule.contains(&"--message-format=json-render-diagnostics"));
    assert_eq!(capsule[0], "zigbuild");
    assert_eq!(jackin[0], "build");
}

#[test]
fn preview_selection_owns_one_pair_and_preserves_each_complete_bin_inventory() {
    let payloads = preview_payloads();
    assert_eq!(payloads.len(), 6);
    for payload in payloads {
        let (package, target) = preview_selection(payload.package, payload.target).unwrap();
        assert_eq!(package_name(package), payload.package);
        assert_eq!(target.rust, payload.target);
        assert_eq!(binaries(package), payload.binaries);
        let command = build_arguments(package, target, ArchiveProfile::Preview);
        assert_eq!(command.iter().filter(|arg| **arg == "--target").count(), 1);
        assert_eq!(
            command.last().unwrap(),
            &"--message-format=json-render-diagnostics"
        );
        assert_eq!(command[command.len() - 2], target.zigbuild);
    }
    preview_selection("jackin-capsule", "aarch64-apple-darwin").unwrap_err();
    preview_selection("foreign", "x86_64-unknown-linux-gnu").unwrap_err();
    preview_selection("jackin", "unknown").unwrap_err();
}

#[cfg(unix)]
#[test]
fn archive_builds_preserve_declared_release_policy_and_exact_target_commands() {
    const CHILD: &str = "JACKIN_ARCHIVE_COMMAND_TEST";
    if let Ok(root) = env::var(CHILD) {
        run_archive_command_probe(Path::new(&root));
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let cargo = temp.path().join("cargo");
    fs::write(
        &cargo,
        r#"#!/bin/sh
printf '%s\n' "$@"
printf '%s\n' "CGU=${CARGO_PROFILE_RELEASE_CODEGEN_UNITS-unset}" "PREVIEW_CGU=${CARGO_PROFILE_PREVIEW_CODEGEN_UNITS-unset}" "JOBS=$CARGO_BUILD_JOBS" "VERSION=$JACKIN_VERSION_OVERRIDE" "TARGET=$CARGO_TARGET_DIR" "SDK=${SDKROOT-unset}" "DEPLOY=${MACOSX_DEPLOYMENT_TARGET-unset}" "ZIG_GLOBAL=$ZIG_GLOBAL_CACHE_DIR" "ZIG_LOCAL=$ZIG_LOCAL_CACHE_DIR"
"#,
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    let status = std::process::Command::new(env::current_exe().unwrap())
        .args([
            "--exact",
            "release_archive::tests::archive_builds_preserve_declared_release_policy_and_exact_target_commands",
        ])
        .env(CHILD, temp.path())
        .env("PATH", temp.path())
        .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1")
        .env("CARGO_PROFILE_PREVIEW_CODEGEN_UNITS", "1")
        .env_remove("SDKROOT")
        .env_remove("MACOSX_DEPLOYMENT_TARGET")
        .status()
        .unwrap();
    assert!(status.success());
    for package in [ArchivePackage::Jackin, ArchivePackage::JackinCapsule] {
        for target in targets(package) {
            for profile in [ArchiveProfile::Release, ArchiveProfile::Preview] {
                let root = temp
                    .path()
                    .join(format!("{profile:?}"))
                    .join(package_name(package))
                    .join(target.rust);
                let layout = ArchiveLayout::new(&root, target.rust, profile).unwrap();
                assert!(layout.artifacts().is_file());
            }
        }
    }
}

#[cfg(unix)]
fn run_archive_command_probe(root: &Path) {
    let cache = ZigCache {
        global: root.join("zig global"),
        local: root.join("zig local"),
    };
    for profile in [ArchiveProfile::Release, ArchiveProfile::Preview] {
        for package in [ArchivePackage::Jackin, ArchivePackage::JackinCapsule] {
            for target in targets(package) {
                probe_command(root, &cache, profile, package, *target);
            }
        }
    }
}

#[cfg(unix)]
fn probe_command(
    root: &Path,
    cache: &ZigCache,
    profile: ArchiveProfile,
    package: ArchivePackage,
    target: TargetSpec,
) {
    let target_dir = root
        .join(format!("{profile:?}"))
        .join(package_name(package))
        .join(target.rust);
    let layout = ArchiveLayout::new(&target_dir, target.rust, profile).unwrap();
    let sdk = root.join("qualified SDK");
    build(
        package,
        target,
        "preview-test",
        3,
        &layout,
        cache,
        Some(&sdk),
    )
    .unwrap();
    let output = fs::read_to_string(layout.artifacts()).unwrap();
    let actual: Vec<_> = output.lines().map(str::to_owned).collect();
    // Independent selector oracle: Release remains stable even with preview-looking version.
    let selector: &[&str] = match profile {
        ArchiveProfile::Release => &["--release"],
        ArchiveProfile::Preview => &["--profile", "preview"],
    };
    assert_eq!(&actual[2..2 + selector.len()], selector);
    assert_eq!(
        actual.iter().any(|v| v == "--release"),
        profile == ArchiveProfile::Release
    );
    assert_eq!(
        actual
            .windows(2)
            .filter(|v| v[0] == "--bin")
            .map(|v| v[1].as_str())
            .collect::<Vec<_>>(),
        binaries(package)
    );
    assert_eq!(actual.iter().filter(|v| *v == "--target").count(), 1);
    let mut expected: Vec<_> = build_arguments(package, target, profile)
        .into_iter()
        .map(str::to_owned)
        .collect();
    expected.extend([
        "CGU=unset".to_owned(),
        "PREVIEW_CGU=unset".to_owned(),
        "JOBS=3".to_owned(),
        "VERSION=preview-test".to_owned(),
        format!("TARGET={}", layout.root().display()),
        format!(
            "SDK={}",
            if target.rust.contains("apple") {
                sdk.to_str().unwrap()
            } else {
                "unset"
            }
        ),
        format!(
            "DEPLOY={}",
            if target.rust.contains("apple") {
                "12.0"
            } else {
                "unset"
            }
        ),
        format!("ZIG_GLOBAL={}", cache.global.display()),
        format!("ZIG_LOCAL={}", cache.local.display()),
    ]);
    assert_eq!(
        actual,
        expected,
        "{profile:?} / {} / {}",
        package_name(package),
        target.rust
    );
}

#[cfg(unix)]
#[test]
fn trusted_target_root_alias_resolves_once_without_admitting_descendant_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real target root");
    fs::create_dir(&real).unwrap();
    let alias = temp.path().join("configured alias");
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let layout =
        ArchiveLayout::new(&alias, "aarch64-apple-darwin", ArchiveProfile::Preview).unwrap();
    assert_eq!(layout.root(), fs::canonicalize(&real).unwrap());
    assert_eq!(
        layout.target_output(),
        layout.root().join("aarch64-apple-darwin/preview")
    );
    assert_eq!(layout.host_output(), layout.root().join("preview"));
    assert_eq!(
        layout.artifacts(),
        layout.root().join("cargo-build-artifacts-preview.jsonl")
    );
    fs::create_dir_all(layout.target_output()).unwrap();
    let selected = layout.target_output().join("jackin");
    fs::write(&selected, b"selected bytes").unwrap();
    layout.validate_path(&selected).unwrap();
    let descendant = layout.target_output().join("alias");
    std::os::unix::fs::symlink(&selected, &descendant).unwrap();
    assert!(
        layout
            .validate_path(&descendant)
            .unwrap_err()
            .to_string()
            .contains("symlink alias")
    );
}
