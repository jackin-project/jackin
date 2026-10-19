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
    let path = package(ArchivePackage::Jackin, target, temp.path(), temp.path()).unwrap();
    let first = fs::read(&path).unwrap();
    let path = package(ArchivePackage::Jackin, target, temp.path(), temp.path()).unwrap();
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
    let jackin = build_arguments(ArchivePackage::Jackin, JACKIN_TARGETS[0]);
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
    let capsule = build_arguments(ArchivePackage::JackinCapsule, CAPSULE_TARGETS[0]);
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
        let command = build_arguments(package, target);
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
