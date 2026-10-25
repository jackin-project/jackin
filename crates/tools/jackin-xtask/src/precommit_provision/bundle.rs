// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Fixed evidence bundle admission for native hook provisioning.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

mod cargo;
mod files;
mod git;

pub(super) use cargo::parse_request;
use cargo::{validate_build_argv, validate_cargo_artifact};
use files::{
    current_executable, hash_executable, physical_private_directory, read_evidence,
    read_path_record, read_physical_file, sha256, validate_oid,
};
pub(super) use files::{physical_directory, read_executable};
pub(super) use git::hook_config_paths;
use git::{git_output, git_output_bytes, index_snapshot, verify_tree_context};

const REQUEST_LIMIT: usize = 4096;
const PATH_LIMIT: usize = 64 * 1024;
const TREE_LIMIT: usize = 64 * 1024 * 1024;
const TARGET_LIMIT: usize = 256 * 1024 * 1024;
const JSON_LINE_LIMIT: usize = 1024 * 1024;

pub(super) struct RequestFields {
    pub(super) reviewed_tree: String,
    pub(super) build_commit: String,
    pub(super) index_sha256: String,
    pub(super) source_sha256: String,
    pub(super) cargo_sha256: String,
    pub(super) rustc_sha256: String,
    pub(super) build_sha256: String,
    pub(super) target_sha256: String,
}
pub(super) struct VerifiedBootstrapInstall {
    pub(super) common_git_dir: PathBuf,
    pub(super) repository: PathBuf,
    pub(super) bootstrap_root: PathBuf,
    pub(super) request_bytes: Vec<u8>,
    pub(super) index_snapshot: Vec<u8>,
    pub(super) source_inventory: Vec<u8>,
    pub(super) source_root: PathBuf,
    pub(super) source_tree: String,
    pub(super) build_commit: String,
    pub(super) source_sha256: String,
    pub(super) cargo_path: PathBuf,
    pub(super) cargo_sha256: String,
    pub(super) rustc_path: PathBuf,
    pub(super) rustc_sha256: String,
    pub(super) target_path: PathBuf,
    pub(super) target_sha256: String,
    pub(super) build_sha256: String,
}

struct BootstrapEvidence {
    bootstrap_root: PathBuf,
    request_bytes: Vec<u8>,
    index_snapshot: Vec<u8>,
    source_inventory: Vec<u8>,
    source_root: PathBuf,
    fields: RequestFields,
    cargo_path: PathBuf,
    cargo_sha256: String,
    rustc_path: PathBuf,
    rustc_sha256: String,
    target_path: PathBuf,
}

pub(super) fn verify(
    bootstrap_root: &Path,
    repository: &Path,
    expected_tree: &str,
) -> Result<VerifiedBootstrapInstall> {
    validate_oid(expected_tree)?;
    let repository = physical_directory(repository, "repository")?;
    let evidence = admit_bundle(bootstrap_root, &repository, expected_tree)?;
    crate::precommit_snapshot::verify_provision_tool_paths(
        &evidence.source_root,
        &evidence.cargo_path,
        &evidence.rustc_path,
    )?;
    let common_git_dir = PathBuf::from(git_output(
        &repository,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    let common_git_dir =
        fs::canonicalize(common_git_dir).context("resolving common Git directory")?;
    Ok(VerifiedBootstrapInstall {
        common_git_dir,
        repository,
        bootstrap_root: evidence.bootstrap_root,
        request_bytes: evidence.request_bytes,
        index_snapshot: evidence.index_snapshot,
        source_inventory: evidence.source_inventory,
        source_root: evidence.source_root,
        source_tree: evidence.fields.reviewed_tree,
        build_commit: evidence.fields.build_commit,
        source_sha256: evidence.fields.source_sha256,
        cargo_path: evidence.cargo_path,
        cargo_sha256: evidence.cargo_sha256,
        rustc_path: evidence.rustc_path,
        rustc_sha256: evidence.rustc_sha256,
        target_path: evidence.target_path,
        target_sha256: evidence.fields.target_sha256,
        build_sha256: evidence.fields.build_sha256,
    })
}

fn admit_bundle(
    bootstrap_root: &Path,
    repository: &Path,
    expected_tree: &str,
) -> Result<BootstrapEvidence> {
    let bootstrap = physical_private_directory(bootstrap_root, "bootstrap root")?;
    files::verify_bootstrap_names(&bootstrap)?;
    let evidence = physical_private_directory(&bootstrap.join("evidence"), "evidence directory")?;
    files::verify_evidence_names(&evidence)?;
    let source = physical_private_directory(&bootstrap.join("source"), "private source")?;
    let request_bytes = read_evidence(&evidence, "request", REQUEST_LIMIT)?;
    let fields = parse_request(&request_bytes)?;
    ensure!(
        fields.reviewed_tree == expected_tree,
        "bootstrap tree differs from the requested tree"
    );
    let index_snapshot = read_evidence(&evidence, "index.snapshot", 512)?;
    ensure!(
        sha256(&index_snapshot) == fields.index_sha256,
        "index snapshot digest mismatch"
    );
    let source_inventory = read_evidence(&evidence, "source.inventory", TREE_LIMIT)?;
    ensure!(
        sha256(&source_inventory) == fields.source_sha256,
        "source inventory digest mismatch"
    );
    let build_json = read_evidence(&evidence, "build.jsonl", TREE_LIMIT)?;
    ensure!(
        sha256(&build_json) == fields.build_sha256,
        "Cargo build transcript digest mismatch"
    );
    let build_argv = read_evidence(&evidence, "build.argv", 4096)?;
    validate_build_argv(&build_argv)?;
    let _build_stderr = read_evidence(&evidence, "build.stderr", TREE_LIMIT)?;
    let cargo_path = read_path_record(&evidence, "cargo.path")?;
    let rustc_path = read_path_record(&evidence, "rustc.path")?;
    let target_path = read_path_record(&evidence, "target.path")?;
    let target_bytes = read_executable(&target_path)?;
    ensure!(
        sha256(&target_bytes) == fields.target_sha256,
        "target binary digest mismatch"
    );
    ensure!(
        current_executable()? == target_path,
        "target path is not this provisioning executable"
    );
    validate_cargo_artifact(&build_json, &source, &target_path)?;
    verify_tree_context(
        repository,
        &source,
        &fields,
        expected_tree,
        &index_snapshot,
        &source_inventory,
    )?;
    let cargo_sha256 = hash_executable(&cargo_path)?;
    let rustc_sha256 = hash_executable(&rustc_path)?;
    ensure!(
        cargo_sha256 == fields.cargo_sha256,
        "Cargo executable digest mismatch"
    );
    ensure!(
        rustc_sha256 == fields.rustc_sha256,
        "rustc executable digest mismatch"
    );
    Ok(BootstrapEvidence {
        bootstrap_root: bootstrap,
        request_bytes,
        index_snapshot,
        source_inventory,
        source_root: source,
        fields,
        cargo_path,
        cargo_sha256,
        rustc_path,
        rustc_sha256,
        target_path,
    })
}

pub(super) fn recheck_live(verified: &VerifiedBootstrapInstall) -> Result<()> {
    recheck_evidence(verified)?;
    recheck_source(verified)?;
    recheck_executables(verified)
}

fn recheck_evidence(verified: &VerifiedBootstrapInstall) -> Result<()> {
    let evidence = physical_private_directory(
        &verified.bootstrap_root.join("evidence"),
        "evidence directory",
    )?;
    files::verify_evidence_names(&evidence)?;
    ensure!(
        read_evidence(&evidence, "request", REQUEST_LIMIT)? == verified.request_bytes,
        "bootstrap request changed during provisioning"
    );
    ensure!(
        read_evidence(&evidence, "index.snapshot", 512)? == verified.index_snapshot,
        "index snapshot evidence changed during provisioning"
    );
    ensure!(
        read_evidence(&evidence, "source.inventory", TREE_LIMIT)? == verified.source_inventory,
        "source inventory evidence changed during provisioning"
    );
    let build_json = read_evidence(&evidence, "build.jsonl", TREE_LIMIT)?;
    ensure!(
        sha256(&build_json) == verified.build_sha256,
        "Cargo build transcript changed during provisioning"
    );
    validate_cargo_artifact(&build_json, &verified.source_root, &verified.target_path)?;
    validate_build_argv(&read_evidence(&evidence, "build.argv", 4096)?)?;
    drop(read_evidence(&evidence, "build.stderr", TREE_LIMIT)?);
    ensure!(
        read_path_record(&evidence, "cargo.path")? == verified.cargo_path,
        "Cargo path evidence changed during provisioning"
    );
    ensure!(
        read_path_record(&evidence, "rustc.path")? == verified.rustc_path,
        "rustc path evidence changed during provisioning"
    );
    ensure!(
        read_path_record(&evidence, "target.path")? == verified.target_path,
        "owner path evidence changed during provisioning"
    );
    Ok(())
}

fn recheck_source(verified: &VerifiedBootstrapInstall) -> Result<()> {
    ensure!(
        git_output(&verified.repository, &["write-tree"])? == verified.source_tree,
        "staged tree changed during native hook provisioning"
    );
    ensure!(
        index_snapshot(&verified.repository)? == verified.index_snapshot,
        "Git index changed during native hook provisioning"
    );
    ensure!(
        git_output(&verified.source_root, &["rev-parse", "HEAD"])? == verified.build_commit,
        "private source commit changed during native hook provisioning"
    );
    ensure!(
        git_output_bytes(
            &verified.source_root,
            &["ls-tree", "-r", "-z", &verified.source_tree]
        )? == verified.source_inventory,
        "private source tree changed during native hook provisioning"
    );
    ensure!(
        git_output_bytes(
            &verified.source_root,
            &[
                "--no-pager",
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--quiet",
                "HEAD",
                "--"
            ],
        )
        .is_ok(),
        "private tracked source files changed during native hook provisioning"
    );
    git::verify_private_source_git(&verified.source_root, &verified.repository)?;
    for relative in ["scripts/hooks-native-bootstrap", "scripts/hooks-install"] {
        let path = verified.repository.join(relative);
        let tree_blob = git_output(
            &verified.repository,
            &["rev-parse", &format!("{}:{relative}", verified.source_tree)],
        )?;
        let expected = git_output_bytes(&verified.repository, &["cat-file", "blob", &tree_blob])?;
        let live = read_physical_file(&path, 1024 * 1024)?;
        let private = read_physical_file(&verified.source_root.join(relative), 1024 * 1024)?;
        ensure!(
            live == expected && private == expected,
            "reviewed bootstrap file changed: {relative}"
        );
    }
    Ok(())
}

fn recheck_executables(verified: &VerifiedBootstrapInstall) -> Result<()> {
    ensure!(
        hash_executable(&verified.target_path)? == verified.target_sha256,
        "owner executable changed during provisioning"
    );
    ensure!(
        hash_executable(&verified.cargo_path)? == verified.cargo_sha256,
        "Cargo executable changed during provisioning"
    );
    ensure!(
        hash_executable(&verified.rustc_path)? == verified.rustc_sha256,
        "rustc executable changed during provisioning"
    );
    Ok(())
}
