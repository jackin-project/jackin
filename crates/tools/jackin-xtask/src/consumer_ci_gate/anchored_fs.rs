use anyhow::{Context, Result, bail, ensure};
use std::{
    ffi::{CString, OsStr, OsString},
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

#[cfg(unix)]
use std::{
    fs::File,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
};

#[cfg(unix)]
use nix::{
    errno::Errno,
    fcntl::{AtFlags, OFlag, open, openat},
    sys::stat::{SFlag, fstatat},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
    Missing,
}

pub(super) struct RepositoryDirectory {
    root: PathBuf,
    #[cfg(unix)]
    directory: File,
}

#[cfg(unix)]
pub(super) struct BoundDirectory {
    relative: PathBuf,
    file: File,
}

impl RepositoryDirectory {
    pub(super) fn open(root: &Path) -> Result<Self> {
        let root = root.canonicalize().context("canonicalizing source root")?;
        #[cfg(unix)]
        let directory = open_directory_path(&root)?;
        Ok(Self {
            root,
            #[cfg(unix)]
            directory,
        })
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    #[cfg(unix)]
    pub(super) fn open_regular(&self, relative: &Path, label: &str) -> Result<File> {
        open_regular_at(&self.directory, relative, label)
    }

    #[cfg(unix)]
    pub(super) fn entry_kind(&self, relative: &Path) -> Result<EntryKind> {
        validate_relative(relative, "entry")?;
        let components = relative.components().collect::<Vec<_>>();
        let mut directory = self.directory.try_clone()?;
        for component in &components[..components.len() - 1] {
            let Component::Normal(name) = component else {
                unreachable!("relative path was validated");
            };
            match entry_kind_at(&directory, name)? {
                EntryKind::Missing => return Ok(EntryKind::Missing),
                EntryKind::Directory => directory = open_directory_at(&directory, name)?,
                _ => bail!("entry ancestor is not a real directory: {name:?}"),
            }
        }
        let Component::Normal(name) = components[components.len() - 1] else {
            unreachable!("relative path was validated");
        };
        entry_kind_at(&directory, name)
    }

    #[cfg(unix)]
    pub(super) fn open_or_create_directory(&self, relative: &Path) -> Result<BoundDirectory> {
        validate_relative(relative, "bound directory")?;
        let mut directory = self.directory.try_clone()?;
        for component in relative.components() {
            let Component::Normal(name) = component else {
                unreachable!("relative path was validated");
            };
            match entry_kind_at(&directory, name)? {
                EntryKind::Missing => {
                    create_directory_at(&directory, name)
                        .with_context(|| format!("creating bound directory {relative:?}"))?;
                }
                EntryKind::Directory => {}
                _ => bail!("bound directory contains a symlink or non-directory: {name:?}"),
            }
            directory = open_directory_at(&directory, name)?;
            ensure_safe_directory(&directory, relative)?;
        }
        Ok(BoundDirectory {
            relative: relative.to_path_buf(),
            file: directory,
        })
    }

    #[cfg(unix)]
    pub(super) fn verify_directory(&self, relative: &Path, expected: &File) -> Result<()> {
        validate_relative(relative, "bound directory")?;
        let current = open_relative_directory(&self.directory, relative)
            .with_context(|| format!("reopening bound directory {relative:?}"))?;
        ensure_same_directory(&current, expected, "bound directory")
    }

    #[cfg(unix)]
    pub(super) fn read_symlink(&self, relative: &Path) -> Result<OsString> {
        validate_relative(relative, "symlink")?;
        let parent = relative.parent().context("symlink has no parent")?;
        let directory = if parent.as_os_str().is_empty() {
            self.directory.try_clone()?
        } else {
            open_relative_directory(&self.directory, parent)?
        };
        read_link_at(
            &directory,
            relative.file_name().context("symlink has no name")?,
        )
    }

    pub(super) fn read_file(&self, relative: &Path, max_bytes: u64) -> Result<Vec<u8>> {
        validate_relative(relative, "source file")?;
        #[cfg(unix)]
        {
            let mut file = open_regular_at(&self.directory, relative, "source file")?;
            read_bounded_file(&mut file, max_bytes, false)
        }
        #[cfg(not(unix))]
        {
            let path = self.root.join(relative);
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(metadata.is_file() && !metadata.file_type().is_symlink());
            let mut file = fs::File::open(path)?;
            read_bounded_file(&mut file, max_bytes, false)
        }
    }
}

#[cfg(unix)]
impl BoundDirectory {
    pub(super) fn relative(&self) -> &Path {
        &self.relative
    }

    pub(super) fn verify(&self, repository: &RepositoryDirectory) -> Result<()> {
        repository.verify_directory(&self.relative, &self.file)
    }

    pub(super) fn entry_kind(
        &self,
        repository: &RepositoryDirectory,
        name: &OsStr,
    ) -> Result<EntryKind> {
        self.verify(repository)?;
        let kind = entry_kind_at(&self.file, name)?;
        self.verify(repository)?;
        Ok(kind)
    }

    pub(super) fn list_children(
        &self,
        repository: &RepositoryDirectory,
        max_entries: usize,
    ) -> Result<Vec<(OsString, EntryKind)>> {
        self.verify(repository)?;
        let entries = source::list_children_at(&self.file, max_entries)?;
        self.verify(repository)?;
        Ok(entries)
    }

    pub(super) fn open_child_directory(
        &self,
        repository: &RepositoryDirectory,
        name: &OsStr,
    ) -> Result<Self> {
        self.verify(repository)?;
        ensure!(
            entry_kind_at(&self.file, name)? == EntryKind::Directory,
            "bound child is not a real directory: {name:?}"
        );
        let directory = open_directory_at(&self.file, name)?;
        let child = Self {
            relative: self.relative.join(name),
            file: directory,
        };
        ensure_safe_directory(&child.file, &child.relative)?;
        child.verify(repository)?;
        Ok(child)
    }

    pub(super) fn read_file(
        &self,
        repository: &RepositoryDirectory,
        name: &OsStr,
        max_bytes: u64,
        require_single_link: bool,
        label: &str,
    ) -> Result<Vec<u8>> {
        self.verify(repository)?;
        let mut file = open_regular_at(&self.file, Path::new(name), label)?;
        let bytes = read_bounded_file(&mut file, max_bytes, require_single_link)?;
        self.verify(repository)?;
        Ok(bytes)
    }
}

#[cfg(unix)]
pub(super) fn open_directory_path(path: &Path) -> Result<File> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut directory = File::from(
        open(
            "/",
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )
        .context("opening filesystem root without following symlinks")?,
    );
    for component in absolute.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => directory = open_directory_at(&directory, name)?,
            Component::ParentDir => bail!("path contains parent traversal: {}", path.display()),
            Component::Prefix(_) => {
                bail!("path contains an unsupported prefix: {}", path.display())
            }
        }
    }
    Ok(directory)
}

#[cfg(unix)]
pub(super) fn open_relative_directory(directory: &File, relative: &Path) -> Result<File> {
    validate_relative(relative, "directory")?;
    let mut current = directory.try_clone()?;
    for component in relative.components() {
        let Component::Normal(name) = component else {
            unreachable!("relative path was validated");
        };
        current = open_directory_at(&current, name)?;
    }
    Ok(current)
}

#[cfg(unix)]
pub(super) fn open_directory_at(directory: &File, name: &OsStr) -> Result<File> {
    let name = c_string(name, "directory name")?;
    let fd = openat(
        directory,
        name.as_c_str(),
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .with_context(|| format!("opening directory {name:?} without following symlinks"))?;
    Ok(File::from(fd))
}

#[cfg(unix)]
pub(super) fn open_regular_at(directory: &File, relative: &Path, label: &str) -> Result<File> {
    validate_relative(relative, label)?;
    let parent = relative.parent().context("file has no parent")?;
    let file_name = relative.file_name().context("file has no name")?;
    let parent = if parent.as_os_str().is_empty() {
        directory.try_clone()?
    } else {
        open_relative_directory(directory, parent)?
    };
    let name = c_string(file_name, "file name")?;
    let fd = openat(
        &parent,
        name.as_c_str(),
        OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .with_context(|| format!("opening {label} without following symlinks"))?;
    let file = File::from(fd);
    ensure!(file.metadata()?.is_file(), "{label} is not a regular file");
    Ok(file)
}

#[cfg(unix)]
pub(super) fn open_new_file_at(directory: &File, name: &OsStr) -> Result<File> {
    let name = c_string(name, "output filename")?;
    let fd = openat(
        directory,
        name.as_c_str(),
        OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::from_bits_truncate(0o600),
    )
    .context("creating a fresh output file without following symlinks")?;
    Ok(File::from(fd))
}

#[cfg(unix)]
pub(super) fn write_new_file_at(directory: &File, name: &OsStr, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut file = open_new_file_at(directory, name)?;
    file.write_all(bytes)
        .context("writing fresh evidence file")?;
    file.sync_all().context("syncing fresh evidence file")?;
    Ok(())
}

#[cfg(unix)]
pub(super) fn create_directory_at(directory: &File, name: &OsStr) -> nix::Result<()> {
    let name = CString::new(name.as_bytes()).map_err(|_| Errno::EINVAL)?;
    nix::sys::stat::mkdirat(
        directory,
        name.as_c_str(),
        nix::sys::stat::Mode::from_bits_truncate(0o700),
    )
}

#[cfg(unix)]
pub(super) fn entry_kind_at(directory: &File, name: &OsStr) -> Result<EntryKind> {
    let name = c_string(name, "entry name")?;
    match fstatat(directory, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(stat) => {
            let kind = stat.st_mode & SFlag::S_IFMT.bits();
            if kind == SFlag::S_IFREG.bits() {
                Ok(EntryKind::File)
            } else if kind == SFlag::S_IFDIR.bits() {
                Ok(EntryKind::Directory)
            } else if kind == SFlag::S_IFLNK.bits() {
                Ok(EntryKind::Symlink)
            } else {
                Ok(EntryKind::Other)
            }
        }
        Err(Errno::ENOENT) => Ok(EntryKind::Missing),
        Err(error) => Err(error).context("checking entry without following symlinks"),
    }
}

#[cfg(unix)]
pub(super) fn read_link_at(directory: &File, name: &OsStr) -> Result<OsString> {
    let name = c_string(name, "symbolic link name")?;
    nix::fcntl::readlinkat(directory, name.as_c_str())
        .context("reading managed target link relative to the checkout descriptor")
}

#[cfg(unix)]
pub(super) fn read_bounded_file(
    file: &mut File,
    max_bytes: u64,
    require_single_link: bool,
) -> Result<Vec<u8>> {
    let before = file
        .metadata()
        .context("checking opened file before read")?;
    ensure!(before.is_file(), "opened evidence is not a regular file");
    ensure!(before.len() <= max_bytes, "evidence exceeds size bound");
    #[cfg(unix)]
    if require_single_link {
        use std::os::unix::fs::MetadataExt;
        ensure!(before.nlink() == 1, "evidence file has multiple hard links");
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .context("reading opened evidence file")?;
    ensure!(
        bytes.len() as u64 <= max_bytes,
        "evidence grew beyond size bound"
    );
    let after = file.metadata().context("checking opened file after read")?;
    ensure!(
        same_file_state(&before, &after)?,
        "evidence changed while being read"
    );
    Ok(bytes)
}

#[cfg(not(unix))]
pub(super) fn read_bounded_file(
    file: &mut fs::File,
    max_bytes: u64,
    _require_single_link: bool,
) -> Result<Vec<u8>> {
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= max_bytes,
        "evidence is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= max_bytes,
        "evidence grew beyond size bound"
    );
    let after = file.metadata()?;
    ensure!(
        same_file_state(&before, &after)?,
        "evidence changed while being read"
    );
    Ok(bytes)
}

#[cfg(unix)]
fn same_file_state(before: &fs::Metadata, after: &fs::Metadata) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    Ok(before.dev() == after.dev()
        && before.ino() == after.ino()
        && before.nlink() == after.nlink()
        && before.len() == after.len()
        && before.modified()? == after.modified()?)
}

#[cfg(not(unix))]
fn same_file_state(before: &fs::Metadata, after: &fs::Metadata) -> Result<bool> {
    Ok(before.len() == after.len() && before.modified()? == after.modified()?)
}

fn validate_relative(path: &Path, label: &str) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "{label} path must stay below its anchored root"
    );
    Ok(())
}

#[cfg(unix)]
fn ensure_same_directory(left: &File, right: &File, label: &str) -> Result<()> {
    let left = left.metadata()?;
    let right = right.metadata()?;
    ensure!(
        left.is_dir() && right.is_dir() && left.dev() == right.dev() && left.ino() == right.ino(),
        "{label} identity changed"
    );
    Ok(())
}

#[cfg(unix)]
fn ensure_safe_directory(directory: &File, relative: &Path) -> Result<()> {
    let metadata = directory
        .metadata()
        .with_context(|| format!("checking bound directory {relative:?}"))?;
    ensure!(
        metadata.is_dir(),
        "bound path is not a directory: {relative:?}"
    );
    ensure!(
        metadata.uid() == nix::unistd::geteuid().as_raw(),
        "bound directory has the wrong owner: {relative:?}"
    );
    ensure!(
        metadata.mode() & 0o022 == 0,
        "bound directory is group or world writable: {relative:?}"
    );
    Ok(())
}

#[cfg(unix)]
fn c_string(value: &OsStr, label: &str) -> Result<CString> {
    CString::new(value.as_bytes()).with_context(|| format!("{label} contains NUL"))
}

mod source;
