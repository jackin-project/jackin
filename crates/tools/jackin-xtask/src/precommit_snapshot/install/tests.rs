use super::*;
use crate::precommit_snapshot::process::capture;
use std::{
    os::unix::fs::PermissionsExt,
    process::{Command, Output},
};

fn assert_error_contains<T: std::fmt::Debug>(result: Result<T>, expected: &str) {
    let error = result.unwrap_err();
    assert!(error.to_string().contains(expected), "{error:#}");
}

fn hook_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().canonicalize().unwrap().join("repo");
    let scripts = repository.join("scripts");
    fs::create_dir_all(&scripts).unwrap();
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../");
    fs::copy(
        source_root.join("scripts/hooks-install"),
        scripts.join("hooks-install"),
    )
    .unwrap();
    fs::write(
        scripts.join("hooks-native-bootstrap"),
        b"#!/bin/sh\nprintf '%s\\n' bootstrap >> bootstrap-events\n",
    )
    .unwrap();
    fs::write(
        scripts.join("pre-commit-snapshot"),
        b"#!/bin/sh\nprintf '%s\\n' snapshot >> hook-events\n",
    )
    .unwrap();
    fs::set_permissions(
        scripts.join("pre-commit-snapshot"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let global = temporary.path().join("global.gitconfig");
    let system = temporary.path().join("empty-system.gitconfig");
    fs::write(&global, b"").unwrap();
    fs::write(&system, b"").unwrap();
    let init = git_command(&repository, &global, &system, &["init", "-q"]);
    assert!(
        init.status.success(),
        "git init: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(
        git_command(
            &repository,
            &global,
            &system,
            &["config", "user.name", "Fixture"]
        )
        .status
        .success()
    );
    assert!(
        git_command(
            &repository,
            &global,
            &system,
            &["config", "user.email", "fixture@example.invalid"]
        )
        .status
        .success()
    );
    (temporary, repository, global, system)
}

fn git_command(repository: &Path, global: &Path, system: &Path, args: &[&str]) -> Output {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(repository)
        .env("GIT_CONFIG_GLOBAL", global)
        .env("GIT_CONFIG_SYSTEM", system)
        .env("GIT_CONFIG_NOSYSTEM", "1");
    capture(&mut command).unwrap()
}

fn install_hooks(repository: &Path, global: &Path, system: &Path) -> Output {
    let mut command = Command::new("sh");
    command
        .arg("scripts/hooks-install")
        .current_dir(repository)
        .env("GIT_CONFIG_GLOBAL", global)
        .env("GIT_CONFIG_SYSTEM", system)
        .env("GIT_CONFIG_NOSYSTEM", "1");
    capture(&mut command).unwrap()
}

fn manifest() -> Vec<u8> {
    format!("{INSTALL_HEADER}\ngeneration={}\nowner-sha256={}\nsource-tree={}\nbuild-commit={}\nsource-sha256={}\ncargo-sha256={}\nrustc-sha256={}\ntarget-sha256={}\nbuild-sha256={}\n", "a".repeat(64), "b".repeat(64), "c".repeat(40), "d".repeat(40), "e".repeat(64), "f".repeat(64), "1".repeat(64), "2".repeat(64), "3".repeat(64)).into_bytes()
}

#[test]
fn parses_exact_install_manifest() {
    let parsed = InstallManifest::parse(&manifest()).unwrap();
    assert_eq!(parsed.generation, "a".repeat(64));
    assert_eq!(parsed.source_tree, "c".repeat(40));
    validate_provision_manifest(&manifest()).unwrap();
}

#[test]
fn rejects_malformed_manifest_records() {
    let original = manifest();
    let mut bad = original.clone();
    bad.extend_from_slice(b"extra=x\n");
    assert_error_contains(InstallManifest::parse(&bad), "manifest has extra fields");
    let bad = String::from_utf8(original)
        .unwrap()
        .replace("generation=", "generation=Z")
        .into_bytes();
    assert_error_contains(InstallManifest::parse(&bad), "manifest generation");
    assert_error_contains(validate_provision_manifest(&bad), "manifest generation");
}

#[test]
fn requires_single_line_pending_tree_records() {
    assert_eq!(parse_one_line(b"abc\n", "fixture").unwrap(), "abc");
    assert_error_contains(
        parse_one_line(b"abc\nxyz\n", "fixture"),
        "exactly one newline",
    );
    assert_error_contains(parse_one_line(b"abc", "fixture"), "exactly one newline");
}

fn install_fixture() -> (
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    String,
    String,
    PathBuf,
    PathBuf,
) {
    let temporary = tempfile::tempdir().unwrap();
    let common = temporary.path().join("common.git");
    fs::create_dir(&common).unwrap();
    let root = common.join("jackin-precommit-owner");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let generations = root.join("generations");
    fs::create_dir(&generations).unwrap();
    fs::set_permissions(&generations, fs::Permissions::from_mode(0o700)).unwrap();
    let generation_id = "a".repeat(64);
    let generation = generations.join(&generation_id);
    fs::create_dir(&generation).unwrap();
    let owner = generation.join("owner");
    fs::write(&owner, b"fixture native owner").unwrap();
    fs::set_permissions(&owner, fs::Permissions::from_mode(0o500)).unwrap();
    let cargo = temporary.path().join("mbx-cargo");
    let rustc = temporary.path().join("pinned-rustc");
    fs::write(&cargo, b"fixture MBX cargo wrapper").unwrap();
    fs::write(&rustc, b"fixture pinned rustc").unwrap();
    let source_tree = "c".repeat(40);
    let manifest = format!(
        "{INSTALL_HEADER}\ngeneration={generation_id}\nowner-sha256={}\nsource-tree={source_tree}\nbuild-commit={}\nsource-sha256={}\ncargo-sha256={}\nrustc-sha256={}\ntarget-sha256={}\nbuild-sha256={}\n",
        sha256(b"fixture native owner"),
        "d".repeat(40),
        "e".repeat(64),
        sha256(&fs::read(&cargo).unwrap()),
        sha256(&fs::read(&rustc).unwrap()),
        "1".repeat(64),
        "2".repeat(64),
    );
    let manifest_path = generation.join("install.manifest");
    fs::write(&manifest_path, manifest).unwrap();
    fs::set_permissions(&manifest_path, fs::Permissions::from_mode(0o400)).unwrap();
    fs::set_permissions(&generation, fs::Permissions::from_mode(0o500)).unwrap();
    fs::write(root.join("active"), format!("{generation_id}\n")).unwrap();
    fs::set_permissions(root.join("active"), fs::Permissions::from_mode(0o600)).unwrap();
    let pending = root.join("pending");
    fs::create_dir(&pending).unwrap();
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(pending.join(&generation_id), format!("{source_tree}\n")).unwrap();
    fs::set_permissions(
        pending.join(&generation_id),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let common = fs::canonicalize(common).unwrap();
    let generation = fs::canonicalize(generation).unwrap();
    (
        temporary,
        common,
        generation,
        generation_id,
        source_tree,
        cargo,
        rustc,
    )
}

#[test]
fn validates_selected_generation_and_pending_tree() {
    let (_temporary, common, generation, _id, tree, cargo, rustc) = install_fixture();
    let owner = generation.join("owner");
    let install = NativeInstall::open_with_running_path(&common, &generation, &owner).unwrap();
    assert_eq!(install.pending_tree.as_deref(), Some(tree.as_str()));
    install.verify_tool_hashes(&cargo, &rustc).unwrap();
    fs::write(&cargo, b"unexpected Cargo executable").unwrap();
    assert!(install.verify_tool_hashes(&cargo, &rustc).is_err());
    install.check_pending_tree(&tree).unwrap();
    assert!(install.check_pending_tree(&"9".repeat(40)).is_err());
}

#[test]
fn rejects_owner_hash_active_generation_and_permission_mismatches() {
    let (_temporary, common, generation, _id, _tree, _cargo, _rustc) = install_fixture();
    let owner = generation.join("owner");
    let mut changed = fs::read(&owner).unwrap();
    changed.push(b'!');
    fs::set_permissions(&owner, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(&owner, changed).unwrap();
    fs::set_permissions(&owner, fs::Permissions::from_mode(0o500)).unwrap();
    assert!(NativeInstall::open_with_running_path(&common, &generation, &owner).is_err());

    fs::write(
        common.join("jackin-precommit-owner/active"),
        format!("{}\n", "9".repeat(64)),
    )
    .unwrap();
    fs::set_permissions(
        common.join("jackin-precommit-owner/active"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(NativeInstall::open_with_running_path(&common, &generation, &owner).is_err());

    let (_temporary, common, generation, _id, _tree, _cargo, _rustc) = install_fixture();
    let owner = generation.join("owner");
    fs::set_permissions(&owner, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(NativeInstall::open_with_running_path(&common, &generation, &owner).is_err());
}

#[test]
fn finalizes_pending_admission_only_after_head_tree_is_observed() {
    let (_temporary, common, generation, id, tree, _cargo, _rustc) = install_fixture();
    let owner = generation.join("owner");
    let mut install = NativeInstall::open_with_running_path(&common, &generation, &owner).unwrap();
    install.finalize_if_committed(None).unwrap();
    assert!(
        common
            .join("jackin-precommit-owner/pending")
            .join(&id)
            .exists()
    );
    install.finalize_if_committed(Some(&tree)).unwrap();
    assert!(
        !common
            .join("jackin-precommit-owner/pending")
            .join(id)
            .exists()
    );
    assert!(install.pending_tree.is_none());
}

#[test]
fn rejects_pending_marker_symlink_and_malformed_tree() {
    let (_temporary, common, generation, id, _tree, _cargo, _rustc) = install_fixture();
    let pending = common.join("jackin-precommit-owner/pending").join(id);
    fs::remove_file(&pending).unwrap();
    std::os::unix::fs::symlink("missing", &pending).unwrap();
    let owner = generation.join("owner");
    assert!(NativeInstall::open_with_running_path(&common, &generation, &owner).is_err());
}

mod hooks;
