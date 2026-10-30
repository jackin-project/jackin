// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Runtime validation for the immutable, locally provisioned native hook.

use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use jackin_process_directory::{CheckedInstallRoot, InstallFileKind};
use sha2::{Digest, Sha256};

const INSTALL_HEADER: &str = "jackin-native-precommit-install/1";
const MANIFEST_LIMIT: usize = 4096;
const OWNER_LIMIT: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
struct InstallManifest {
    generation: String,
    owner_sha256: String,
    source_tree: String,
    build_commit: String,
    source_sha256: String,
    cargo_sha256: String,
    rustc_sha256: String,
    target_sha256: String,
    build_sha256: String,
}

pub(super) struct NativeInstall {
    root: PathBuf,
    manifest: InstallManifest,
    pending_tree: Option<String>,
}

impl NativeInstall {
    pub(super) fn open(common_git_dir: &Path, selected_generation: &Path) -> Result<Self> {
        let running = std::env::current_exe().context("resolving running native hook owner")?;
        Self::open_with_running_path(common_git_dir, selected_generation, &running)
    }

    fn open_with_running_path(
        common_git_dir: &Path,
        selected_generation: &Path,
        running_executable: &Path,
    ) -> Result<Self> {
        let common_git_dir =
            fs::canonicalize(common_git_dir).context("resolving physical Git common directory")?;
        let owner_root = common_git_dir.join("jackin-precommit-owner");
        let owner_root =
            fs::canonicalize(&owner_root).context("resolving native hook install root")?;
        ensure!(
            owner_root == common_git_dir.join("jackin-precommit-owner"),
            "native hook install root is not physical"
        );
        let root = CheckedInstallRoot::from_absolute_directory(&owner_root)
            .context("checking native hook install root")?;
        let active = root
            .open_file(Path::new("active"), InstallFileKind::Receipt)
            .context("opening active native hook generation")?;
        let active_id = read_checked(&active, 65, "active generation")?;
        let active_id = parse_one_line(&active_id, "active generation")?;
        validate_hex(active_id, 64, "active generation")?;

        let expected = owner_root.join("generations").join(active_id);
        ensure!(
            selected_generation.is_absolute(),
            "selected native hook generation is not absolute"
        );
        ensure!(
            selected_generation == expected,
            "launcher selected a generation other than the active generation"
        );
        let generation = fs::canonicalize(selected_generation)
            .context("resolving selected native hook generation")?;
        ensure!(
            generation == selected_generation,
            "selected native hook generation traverses a symlink"
        );
        validate_directory(
            &owner_root.join("generations"),
            0o700,
            "native hook generations directory",
        )?;
        validate_directory(&generation, 0o500, "native hook generation")?;

        let generations_root =
            CheckedInstallRoot::from_absolute_directory(&owner_root.join("generations"))
                .context("checking immutable native hook generations root")?;
        let manifest_file = generations_root
            .open_file(
                Path::new(&format!("{active_id}/install.manifest")),
                InstallFileKind::Script,
            )
            .context("opening immutable native hook manifest")?;
        let manifest_bytes = read_checked(&manifest_file, MANIFEST_LIMIT, "native hook manifest")?;
        let manifest = InstallManifest::parse(&manifest_bytes)?;
        ensure!(
            manifest.generation == active_id,
            "native hook manifest generation mismatch"
        );
        let owner_path = generation.join("owner");
        let owner_file = generations_root
            .open_file(
                Path::new(&format!("{active_id}/owner")),
                InstallFileKind::SealedCopy,
            )
            .context("opening immutable native hook owner")?;
        let owner_bytes = read_checked(&owner_file, OWNER_LIMIT as usize, "native hook owner")?;
        ensure!(
            sha256(&owner_bytes) == manifest.owner_sha256,
            "native hook owner hash mismatch"
        );
        let running = fs::canonicalize(running_executable)
            .context("resolving running native hook owner path")?;
        ensure!(
            running == owner_path,
            "running native hook owner does not match selected generation"
        );

        let pending_tree = read_pending(&owner_root, active_id)?;
        ensure!(
            pending_tree
                .as_deref()
                .is_none_or(|tree| tree == manifest.source_tree),
            "pending first-commit tree does not match installed source tree"
        );
        Ok(Self {
            root: owner_root,
            manifest,
            pending_tree,
        })
    }

    /// A pending source admission expires only after a later read observes
    /// HEAD's tree equal to the reviewed staged tree. The immutable generation
    /// and manifest are never modified by this finalization.
    pub(super) fn finalize_if_committed(&mut self, head_tree: Option<&str>) -> Result<()> {
        let Some(expected) = self.pending_tree.as_deref() else {
            return Ok(());
        };
        if head_tree == Some(expected) {
            let path = self.root.join("pending").join(&self.manifest.generation);
            validate_directory(
                &self.root.join("pending"),
                0o700,
                "native hook pending directory",
            )?;
            let bytes = read_pending_file(&self.root, &self.manifest.generation)?;
            ensure!(
                parse_one_line(&bytes, "pending native hook tree")? == expected,
                "pending native hook tree changed before finalization"
            );
            fs::remove_file(path).context("finalizing completed native hook bootstrap")?;
            self.pending_tree = None;
        }
        Ok(())
    }

    pub(super) fn check_pending_tree(&self, tree: &str) -> Result<()> {
        if let Some(expected) = &self.pending_tree {
            ensure!(
                tree == expected,
                "staged tree differs from the reviewed first-commit bootstrap tree"
            );
        }
        Ok(())
    }

    pub(super) fn verify_tool_hashes(&self, cargo: &Path, rustc: &Path) -> Result<()> {
        ensure!(
            sha256_file(cargo)? == self.manifest.cargo_sha256,
            "qualified MBX Cargo hash differs from install manifest"
        );
        ensure!(
            sha256_file(rustc)? == self.manifest.rustc_sha256,
            "qualified rustc hash differs from install manifest"
        );
        Ok(())
    }
}

impl InstallManifest {
    fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MANIFEST_LIMIT,
            "native hook manifest exceeds its size limit"
        );
        ensure!(
            bytes.is_ascii() && !bytes.contains(&0) && !bytes.contains(&b'\r'),
            "native hook manifest is not canonical ASCII"
        );
        ensure!(
            bytes.last() == Some(&b'\n'),
            "native hook manifest lacks final newline"
        );
        let text = std::str::from_utf8(bytes).context("decoding native hook manifest")?;
        let mut lines = text
            .strip_suffix('\n')
            .context("native hook manifest lacks final newline")?
            .split('\n');
        ensure!(
            lines.next() == Some(INSTALL_HEADER),
            "native hook manifest header mismatch"
        );
        let generation = field(&mut lines, "generation")?;
        let owner_sha256 = digest_field(&mut lines, "owner-sha256")?;
        let source_tree = field(&mut lines, "source-tree")?;
        let build_commit = field(&mut lines, "build-commit")?;
        let source_sha256 = digest_field(&mut lines, "source-sha256")?;
        let cargo_sha256 = digest_field(&mut lines, "cargo-sha256")?;
        let rustc_sha256 = digest_field(&mut lines, "rustc-sha256")?;
        let target_sha256 = digest_field(&mut lines, "target-sha256")?;
        let build_sha256 = digest_field(&mut lines, "build-sha256")?;
        ensure!(
            lines.next().is_none(),
            "native hook manifest has extra fields"
        );
        validate_hex(&generation, 64, "manifest generation")?;
        validate_oid(&source_tree, "manifest source tree")?;
        validate_oid(&build_commit, "manifest build commit")?;
        Ok(Self {
            generation,
            owner_sha256,
            source_tree,
            build_commit,
            source_sha256,
            cargo_sha256,
            rustc_sha256,
            target_sha256,
            build_sha256,
        })
    }
}

pub(super) fn validate_provision_manifest(bytes: &[u8]) -> Result<()> {
    InstallManifest::parse(bytes).map(|_| ())
}

fn field(lines: &mut std::str::Split<'_, char>, name: &str) -> Result<String> {
    let line = lines
        .next()
        .with_context(|| format!("native hook manifest is missing {name}"))?;
    let (key, value) = line
        .split_once('=')
        .with_context(|| format!("invalid native hook manifest field {name}"))?;
    ensure!(
        key == name && !value.is_empty(),
        "native hook manifest expected {name}"
    );
    Ok(value.to_owned())
}

fn digest_field(lines: &mut std::str::Split<'_, char>, name: &str) -> Result<String> {
    let value = field(lines, name)?;
    validate_hex(&value, 64, name)?;
    Ok(value)
}

fn validate_hex(value: &str, length: usize, role: &str) -> Result<()> {
    ensure!(
        value.len() == length
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{role} is not canonical lowercase hexadecimal"
    );
    Ok(())
}

fn validate_oid(value: &str, role: &str) -> Result<()> {
    ensure!(
        [40, 64].contains(&value.len()),
        "{role} has an unsupported Git object ID length"
    );
    ensure!(
        value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{role} is not canonical lowercase hexadecimal"
    );
    Ok(())
}

fn parse_one_line<'a>(bytes: &'a [u8], role: &str) -> Result<&'a str> {
    ensure!(
        bytes.last() == Some(&b'\n') && bytes.split(|byte| *byte == b'\n').count() == 2,
        "{role} must contain exactly one newline-terminated record"
    );
    let value = std::str::from_utf8(&bytes[..bytes.len() - 1])
        .with_context(|| format!("decoding {role}"))?;
    ensure!(
        !value.is_empty() && value.is_ascii(),
        "{role} is empty or non-ASCII"
    );
    Ok(value)
}

fn read_checked(
    file: &jackin_process_directory::CheckedInstallFile,
    limit: usize,
    role: &str,
) -> Result<Vec<u8>> {
    file.recheck()
        .with_context(|| format!("rechecking {role}"))?;
    let mut bytes = Vec::new();
    file.file()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading {role}"))?;
    ensure!(bytes.len() <= limit, "{role} exceeds its size limit");
    file.recheck()
        .with_context(|| format!("rechecking {role} after read"))?;
    Ok(bytes)
}

fn validate_directory(path: &Path, expected_mode: u32, role: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path).with_context(|| format!("checking {role}"))?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "{role} is not a physical directory"
    );
    ensure!(
        metadata.uid() == nix::unistd::geteuid().as_raw(),
        "{role} has unexpected owner"
    );
    ensure!(
        metadata.permissions().mode() & 0o777 == expected_mode,
        "{role} has unexpected permissions"
    );
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sha256_file(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("checking qualified tool {}", path.display()))?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "qualified tool is not a physical regular file"
    );
    let bytes =
        fs::read(path).with_context(|| format!("reading qualified tool {}", path.display()))?;
    Ok(sha256(&bytes))
}

fn read_pending(root: &Path, generation: &str) -> Result<Option<String>> {
    let directory = root.join("pending");
    match fs::symlink_metadata(&directory) {
        Ok(_) => validate_directory(&directory, 0o700, "native hook pending directory")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("checking native hook pending directory"),
    }
    let path = directory.join(generation);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("checking pending native hook tree"),
        Ok(_) => {
            let bytes = read_pending_file(root, generation)?;
            let tree = parse_one_line(&bytes, "pending native hook tree")?.to_owned();
            validate_oid(&tree, "pending native hook tree")?;
            Ok(Some(tree))
        }
    }
}

fn read_pending_file(root: &Path, generation: &str) -> Result<Vec<u8>> {
    let pending_root = CheckedInstallRoot::from_absolute_directory(&root.join("pending"))
        .context("checking native hook pending directory")?;
    let pending = pending_root
        .open_file(Path::new(generation), InstallFileKind::Receipt)
        .context("opening pending native hook tree")?;
    read_checked(&pending, 65, "pending native hook tree")
}

#[cfg(test)]
mod tests;
