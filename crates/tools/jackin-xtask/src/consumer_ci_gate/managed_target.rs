// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, bail, ensure};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use super::anchored_fs::{
    EntryKind, RepositoryDirectory, create_directory_at, entry_kind_at, open_directory_at,
    open_directory_path, open_regular_at, open_relative_directory, read_bounded_file,
    write_new_file_at,
};

const TARGET_DIR: &str = "target";
const REPORT_DIR: &str = "consumer-ci";
const MAX_DOCTOR_BYTES: usize = 32 * 1024;
const MAX_REPORT_BYTES: usize = 16 * 1024 * 1024;

pub(super) struct TargetDirectory {
    repository: RepositoryDirectory,
    managed_root_path: PathBuf,
    target_path: PathBuf,
    target_id: OsString,
    target: File,
    reports: File,
    reports_path: PathBuf,
}

pub(super) struct RunDirectory {
    file: File,
    path: PathBuf,
    name: OsString,
}

impl TargetDirectory {
    pub(super) fn open(root: &Path) -> Result<Self> {
        let repository = RepositoryDirectory::open(root)?;
        let mut doctor = crate::cmd::command("mbx");
        doctor.current_dir(repository.root()).arg("doctor");
        let output = crate::cmd::output_timeout_bounded(
            &mut doctor,
            Duration::from_secs(30),
            MAX_DOCTOR_BYTES,
            MAX_DOCTOR_BYTES,
        )?;
        let report = String::from_utf8(output).context("mbx doctor output is not UTF-8")?;
        let managed_root = parse_managed_target_root(&report)?;
        Self::open_with_target_root(repository, &managed_root)
    }

    pub(super) fn open_with_target_root(
        repository: RepositoryDirectory,
        managed_root: &Path,
    ) -> Result<Self> {
        let managed_root_path = normalize_absolute(managed_root)?;
        ensure!(
            managed_root_path.canonicalize()? == managed_root_path,
            "MBX-managed target root contains a symlinked path component"
        );
        let managed_root_fd = open_directory_path(&managed_root_path)
            .context("opening MBX-managed targets root without symlinks")?;
        let checkout_target = Path::new(TARGET_DIR);
        ensure!(
            repository.entry_kind(checkout_target)? == EntryKind::Symlink,
            "MBX-managed Cargo target must be a symlink"
        );
        let link_text = repository.read_symlink(checkout_target)?;
        let target_path = resolve_link(repository.root(), Path::new(&link_text))?;
        let relative = target_path
            .strip_prefix(&managed_root_path)
            .context("target link escapes MBX-managed targets root")?;
        let mut components = relative.components();
        ensure!(
            matches!(components.next(), Some(Component::Normal(name)) if name == "v1"),
            "target link is outside MBX managed-target v1 namespace"
        );
        let target_id = match (components.next(), components.next()) {
            (Some(Component::Normal(id)), None) if !id.is_empty() => id.to_os_string(),
            _ => bail!("MBX managed target must be one v1 checkout directory"),
        };
        let target_relative = PathBuf::from("v1").join(&target_id);
        let target = open_relative_directory(&managed_root_fd, &target_relative)
            .context("opening validated MBX managed checkout target")?;

        let reports = match entry_kind_at(&target, OsStr::new(REPORT_DIR))? {
            EntryKind::Missing => {
                create_directory_at(&target, OsStr::new(REPORT_DIR))
                    .context("creating owned consumer report directory")?;
                open_directory_at(&target, OsStr::new(REPORT_DIR))?
            }
            EntryKind::Directory => open_directory_at(&target, OsStr::new(REPORT_DIR))?,
            _ => bail!("target/consumer-ci must be a real directory"),
        };
        ensure_owned_directory(&reports, "target/consumer-ci")?;
        let reports_path = target_path.join(REPORT_DIR);
        Ok(Self {
            repository,
            managed_root_path,
            target_path,
            target_id,
            target,
            reports,
            reports_path,
        })
    }

    pub(super) fn repository(&self) -> &RepositoryDirectory {
        &self.repository
    }

    pub(super) fn preflight_report(&self, name: &OsStr) -> Result<()> {
        self.verify_link()?;
        validate_report_name(name)?;
        ensure!(
            entry_kind_at(&self.reports, name)? == EntryKind::Missing,
            "preexisting consumer report rejected"
        );
        Ok(())
    }

    pub(super) fn write_report(&self, name: &OsStr, bytes: &[u8]) -> Result<()> {
        ensure!(
            bytes.len() <= MAX_REPORT_BYTES,
            "consumer report exceeds 16 MiB bound"
        );
        self.preflight_report(name)?;
        write_new_file_at(&self.reports, name, bytes)?;
        self.verify_link()
    }

    pub(super) fn create_run_directory(&self) -> Result<RunDirectory> {
        ensure_owned_directory(&self.reports, "target/consumer-ci")?;
        self.verify_link()?;
        let name = format!("run-{}", uuid::Uuid::new_v4().simple());
        create_directory_at(&self.reports, OsStr::new(&name))
            .context("creating fresh consumer lane directory")?;
        let file = open_directory_at(&self.reports, OsStr::new(&name))?;
        ensure_owned_directory(&file, "consumer lane directory")?;
        let run = RunDirectory {
            file,
            path: self.reports_path.join(&name),
            name: OsString::from(name),
        };
        self.verify_run_directory(&run)?;
        Ok(run)
    }

    pub(super) fn open_artifact(&self, relative: &Path) -> Result<File> {
        self.verify_link()?;
        open_regular_at(&self.target, relative, "build artifact")
    }

    pub(super) fn artifact_path(&self, path: &Path) -> Result<PathBuf> {
        let relative = path
            .strip_prefix(&self.target_path)
            .context("build artifact is outside the MBX managed target")?;
        ensure!(
            !relative.as_os_str().is_empty()
                && relative
                    .components()
                    .all(|component| matches!(component, Component::Normal(_))),
            "build artifact path must stay below the MBX managed target"
        );
        Ok(Path::new(TARGET_DIR).join(relative))
    }

    pub(super) fn verify_link(&self) -> Result<()> {
        let link = self.repository.read_symlink(Path::new(TARGET_DIR))?;
        let resolved = resolve_link(self.repository.root(), Path::new(&link))?;
        ensure!(
            resolved == self.target_path,
            "MBX managed target link changed during gate"
        );
        let managed_root = open_directory_path(&self.managed_root_path)?;
        let current =
            open_relative_directory(&managed_root, &Path::new("v1").join(&self.target_id))?;
        ensure_same_directory(&current, &self.target)?;
        let current_reports = open_directory_at(&current, OsStr::new(REPORT_DIR))
            .context("reopening visible target/consumer-ci without following symlinks")?;
        ensure_same_directory(&current_reports, &self.reports)
            .context("target/consumer-ci directory identity changed")?;
        ensure_owned_directory(&current_reports, "target/consumer-ci")?;
        Ok(())
    }

    pub(super) fn verify_run_directory(&self, run: &RunDirectory) -> Result<()> {
        self.verify_link()?;
        let current = open_directory_at(&self.reports, &run.name)
            .context("reopening visible consumer run directory without following symlinks")?;
        ensure_same_directory(&current, &run.file)
            .context("consumer run directory identity changed")?;
        ensure_owned_directory(&current, "consumer lane directory")
    }
}

impl RunDirectory {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn id(&self) -> Result<&OsStr> {
        Ok(&self.name)
    }

    pub(super) fn write_file(
        &self,
        target: &TargetDirectory,
        name: &str,
        bytes: &[u8],
    ) -> Result<()> {
        target.verify_run_directory(self)?;
        validate_simple_name(OsStr::new(name), "lane evidence filename")?;
        write_new_file_at(&self.file, OsStr::new(name), bytes)?;
        target.verify_run_directory(self)
    }

    pub(super) fn read_relative_file(
        &self,
        target: &TargetDirectory,
        relative: &Path,
        max_bytes: u64,
    ) -> Result<Vec<u8>> {
        target.verify_run_directory(self)?;
        ensure!(
            !relative.as_os_str().is_empty()
                && relative
                    .components()
                    .all(|component| matches!(component, Component::Normal(_))),
            "lane evidence path must remain below its anchored directory"
        );
        let mut file = open_regular_at(&self.file, relative, "lane evidence")?;
        let bytes = read_bounded_file(&mut file, max_bytes, true)?;
        target.verify_run_directory(self)?;
        Ok(bytes)
    }

    pub(super) fn create_child_directory(
        &self,
        target: &TargetDirectory,
        name: &str,
    ) -> Result<PathBuf> {
        target.verify_run_directory(self)?;
        validate_simple_name(OsStr::new(name), "lane directory name")?;
        create_directory_at(&self.file, OsStr::new(name))
            .context("creating private lane subdirectory")?;
        let directory = open_directory_at(&self.file, OsStr::new(name))?;
        ensure_owned_directory(&directory, "lane subdirectory")?;
        let visible = open_directory_at(&self.file, OsStr::new(name))?;
        ensure_same_directory(&visible, &directory)?;
        target.verify_run_directory(self)?;
        Ok(self.path.join(name))
    }
}

fn parse_managed_target_root(report: &str) -> Result<PathBuf> {
    const MARKER: &str = "managed targets enabled at ";
    let matches = report.match_indices(MARKER).collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "mbx doctor did not report one managed-target root"
    );
    let start = matches[0].0 + MARKER.len();
    let tail = &report[start..];
    let (path, _) = tail
        .split_once(';')
        .context("mbx doctor managed-target line is malformed")?;
    let path = path.trim();
    ensure!(!path.is_empty(), "mbx doctor managed-target root is empty");
    let path = PathBuf::from(path);
    ensure!(
        path.is_absolute(),
        "mbx doctor managed-target root is not absolute"
    );
    Ok(path)
}

fn resolve_link(root: &Path, link: &Path) -> Result<PathBuf> {
    let joined = if link.is_absolute() {
        link.to_path_buf()
    } else {
        root.join(link)
    };
    normalize_absolute(&joined)
}

fn normalize_absolute(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "managed target path is not absolute");
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => normalized.push(name),
            Component::ParentDir => {
                ensure!(normalized.pop(), "managed target path traverses above root");
            }
            Component::Prefix(_) => bail!("unsupported managed target path prefix"),
        }
    }
    Ok(normalized)
}

fn ensure_same_directory(left: &File, right: &File) -> Result<()> {
    let left = left.metadata()?;
    let right = right.metadata()?;
    ensure!(
        left.is_dir() && right.is_dir() && left.dev() == right.dev() && left.ino() == right.ino(),
        "managed target directory identity changed"
    );
    Ok(())
}

fn ensure_owned_directory(directory: &File, label: &str) -> Result<()> {
    let metadata = directory
        .metadata()
        .with_context(|| format!("checking {label}"))?;
    ensure!(metadata.is_dir(), "{label} is not a directory");
    ensure!(
        metadata.uid() == nix::unistd::geteuid().as_raw(),
        "{label} has the wrong owner"
    );
    ensure!(
        metadata.mode() & 0o7777 == 0o700,
        "{label} must be owner-only mode 0700"
    );
    Ok(())
}

fn validate_report_name(name: &OsStr) -> Result<()> {
    validate_simple_name(name, "report filename")?;
    ensure!(
        Path::new(name)
            .extension()
            .is_some_and(|extension| extension == "json"),
        "report filename must end in .json"
    );
    Ok(())
}

fn validate_simple_name(name: &OsStr, label: &str) -> Result<()> {
    let name = name
        .to_str()
        .with_context(|| format!("{label} is not UTF-8"))?;
    ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
        "invalid {label}"
    );
    Ok(())
}
