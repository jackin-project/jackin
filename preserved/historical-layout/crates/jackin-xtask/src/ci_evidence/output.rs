// Descriptor-relative evidence file access. On Unix, each directory
// component is opened with `O_NOFOLLOW`, and files are created/read/renamed
// relative to the pinned parent descriptor so a symlink swap cannot redirect
// an artifact outside the intended tree.

use std::{ffi::{CString, OsStr, OsString}, path::Component};

#[derive(Debug)]
struct OutputLocation {
    parent: OutputDirectory,
    name: OsString,
    path: PathBuf,
}

#[cfg(unix)]
#[derive(Debug)]
struct OutputDirectory {
    file: fs::File,
    path: PathBuf,
}

#[cfg(not(unix))]
#[derive(Debug)]
struct OutputDirectory {
    path: PathBuf,
}

impl OutputLocation {
    fn open(path: &Path, create_parent: bool) -> Result<Self> {
        let name = path
            .file_name()
            .with_context(|| format!("output path has no file name: {}", path.display()))?
            .to_os_string();
        let parent_path = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let parent = OutputDirectory::open(parent_path, create_parent)?;
        Ok(Self {
            parent,
            name,
            path: path.to_path_buf(),
        })
    }
}

#[cfg(unix)]
fn open_directory_component(
    directory: &fs::File,
    name: &CString,
    flags: nix::fcntl::OFlag,
    create: bool,
    resolved: &Path,
) -> Result<std::os::fd::OwnedFd> {
    use nix::{
        errno::Errno,
        fcntl::openat,
        sys::stat::{Mode, mkdirat},
    };

    match openat(directory, name.as_c_str(), flags, Mode::empty()) {
        Ok(fd) => Ok(fd),
        Err(Errno::ENOENT) if create => {
            if let Err(error) = mkdirat(directory, name.as_c_str(), Mode::from_bits_truncate(0o755))
                && error != Errno::EEXIST
            {
                return Err(error).with_context(|| {
                    format!(
                        "creating {}",
                        resolved.join(name.to_string_lossy().as_ref()).display()
                    )
                });
            }
            openat(directory, name.as_c_str(), flags, Mode::empty()).with_context(|| {
                format!(
                    "opening created evidence directory {}",
                    resolved.join(name.to_string_lossy().as_ref()).display()
                )
            })
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "opening evidence directory {} without following symlinks",
                resolved.join(name.to_string_lossy().as_ref()).display()
            )
        }),
    }
}

impl OutputDirectory {
    #[cfg(unix)]
    fn open(path: &Path, create: bool) -> Result<Self> {
        use nix::{
            fcntl::{OFlag, open},
            sys::stat::Mode,
        };
        use std::os::unix::ffi::OsStrExt;

        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            env::current_dir()?.join(path)
        };
        let root = if absolute.is_absolute() {
            Path::new("/")
        } else {
            Path::new(".")
        };
        let root_fd = open(
            root,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .context("opening evidence path root")?;
        let mut directory = fs::File::from(root_fd);
        let mut resolved = root.to_path_buf();
        for component in absolute.components() {
            match component {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(name) => {
                    let name = CString::new(name.as_bytes())
                        .context("evidence path component contains NUL")?;
                    let flags = OFlag::O_RDONLY
                        | OFlag::O_DIRECTORY
                        | OFlag::O_NOFOLLOW
                        | OFlag::O_CLOEXEC;
                    let next =
                        open_directory_component(&directory, &name, flags, create, &resolved)?;
                    directory = fs::File::from(next);
                    resolved.push(name.to_string_lossy().as_ref());
                }
                Component::ParentDir => {
                    bail!("evidence path contains parent traversal: {}", path.display());
                }
                Component::Prefix(_) => {
                    bail!("evidence path has an unsupported prefix: {}", path.display());
                }
            }
        }
        Ok(Self {
            file: directory,
            path: resolved,
        })
    }

    #[cfg(not(unix))]
    fn open(path: &Path, create: bool) -> Result<Self> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            env::current_dir()?.join(path)
        };
        let mut current = PathBuf::new();
        for component in absolute.components() {
            match component {
                Component::Prefix(prefix) => current.push(prefix.as_os_str()),
                Component::RootDir => current.push(component.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => {
                    bail!("evidence path contains parent traversal: {}", path.display());
                }
                Component::Normal(name) => {
                    current.push(name);
                    match fs::symlink_metadata(&current) {
                        Ok(metadata) if metadata.file_type().is_symlink() => {
                            bail!("evidence path contains a symlink: {}", current.display());
                        }
                        Ok(metadata) if metadata.is_dir() => {}
                        Ok(_) => bail!("evidence path component is not a directory: {}", current.display()),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                            fs::create_dir(&current)?;
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        }
        Ok(Self { path: current })
    }

    #[cfg(unix)]
    fn lock(&self) -> Result<()> {
        fs4::FileExt::lock(&self.file)
            .with_context(|| format!("locking evidence output directory {}", self.path.display()))
    }

    #[cfg(not(unix))]
    fn lock(&self) -> Result<()> {
        let directory = fs::File::open(&self.path)?;
        fs4::FileExt::lock(&directory)
            .with_context(|| format!("locking evidence output directory {}", self.path.display()))
    }

    #[cfg(unix)]
    fn entry_kind(&self, name: &OsStr) -> Result<EntryKind> {
        use nix::{
            errno::Errno,
            fcntl::AtFlags,
            sys::stat::{SFlag, fstatat},
        };
        use std::os::unix::ffi::OsStrExt;

        let name = CString::new(name.as_bytes()).context("evidence file name contains NUL")?;
        match fstatat(&self.file, name.as_c_str(), AtFlags::AT_SYMLINK_NOFOLLOW) {
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
            Err(error) => Err(error).context("checking evidence output entry"),
        }
    }

    #[cfg(not(unix))]
    fn entry_kind(&self, name: &OsStr) -> Result<EntryKind> {
        let path = self.path.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Ok(EntryKind::Symlink),
            Ok(metadata) if metadata.is_file() => Ok(EntryKind::File),
            Ok(metadata) if metadata.is_dir() => Ok(EntryKind::Directory),
            Ok(_) => Ok(EntryKind::Other),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(EntryKind::Missing),
            Err(error) => Err(error.into()),
        }
    }

    #[cfg(unix)]
    fn write_new(&self, name: &OsStr, bytes: &[u8]) -> Result<()> {
        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::Mode;
        use std::os::unix::ffi::OsStrExt;

        let name = CString::new(name.as_bytes()).context("evidence file name contains NUL")?;
        let fd = openat(
            &self.file,
            name.as_c_str(),
            OFlag::O_WRONLY
                | OFlag::O_CREAT
                | OFlag::O_EXCL
                | OFlag::O_NOFOLLOW
                | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o666),
        )
        .context("creating evidence file without following symlinks")?;
        let mut file = fs::File::from(fd);
        file.write_all(bytes).context("writing evidence file")?;
        file.sync_all().context("syncing evidence file")
    }

    #[cfg(not(unix))]
    fn write_new(&self, name: &OsStr, bytes: &[u8]) -> Result<()> {
        let path = self.path.join(name);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("creating {}", path.display()))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }

    #[cfg(unix)]
    fn open_file(&self, name: &OsStr) -> Result<fs::File> {
        use nix::{
            fcntl::{OFlag, openat},
            sys::stat::{SFlag, fstat},
        };
        use std::os::unix::ffi::OsStrExt;

        let name = CString::new(name.as_bytes()).context("evidence file name contains NUL")?;
        let fd = openat(
            &self.file,
            name.as_c_str(),
            OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            nix::sys::stat::Mode::empty(),
        )
        .context("opening evidence file without following symlinks")?;
        let metadata = fstat(&fd).context("checking evidence input file type")?;
        if metadata.st_mode & SFlag::S_IFMT.bits() != SFlag::S_IFREG.bits() {
            bail!("evidence input is not a regular file");
        }
        Ok(fs::File::from(fd))
    }

    #[cfg(not(unix))]
    fn open_file(&self, name: &OsStr) -> Result<fs::File> {
        let path = self.path.join(name);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            bail!("evidence input is not a regular file: {}", path.display());
        }
        Ok(fs::File::open(path)?)
    }

    #[cfg(unix)]
    fn rename(&self, from: &OsStr, to: &OsStr) -> Result<()> {
        use nix::fcntl::renameat;
        use std::os::unix::ffi::OsStrExt;

        let from = CString::new(from.as_bytes()).context("temporary file name contains NUL")?;
        let to = CString::new(to.as_bytes()).context("evidence file name contains NUL")?;
        renameat(&self.file, from.as_c_str(), &self.file, to.as_c_str())
            .context("publishing evidence file")
    }

    #[cfg(not(unix))]
    fn rename(&self, from: &OsStr, to: &OsStr) -> Result<()> {
        fs::rename(self.path.join(from), self.path.join(to)).context("publishing evidence file")
    }

    #[cfg(unix)]
    fn create_directory(&self, name: &OsStr) -> Result<()> {
        use nix::{
            errno::Errno,
            sys::stat::{Mode, mkdirat},
        };
        use std::os::unix::ffi::OsStrExt;

        let name = CString::new(name.as_bytes()).context("staging directory name contains NUL")?;
        match mkdirat(&self.file, name.as_c_str(), Mode::from_bits_truncate(0o700)) {
            Ok(()) => Ok(()),
            Err(Errno::EEXIST) => bail!("staging evidence directory already exists"),
            Err(error) => Err(error).context("creating staging evidence directory"),
        }
    }

    #[cfg(not(unix))]
    fn create_directory(&self, name: &OsStr) -> Result<()> {
        fs::create_dir(self.path.join(name)).context("creating staging evidence directory")
    }

    #[cfg(unix)]
    fn open_directory(&self, name: &OsStr) -> Result<Self> {
        use nix::{
            fcntl::{OFlag, openat},
            sys::stat::Mode,
        };
        use std::os::unix::ffi::OsStrExt;

        let name = CString::new(name.as_bytes()).context("staging directory name contains NUL")?;
        let fd = openat(
            &self.file,
            name.as_c_str(),
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .context("opening staging evidence directory")?;
        Ok(Self {
            file: fs::File::from(fd),
            path: self.path.join(name.to_string_lossy().as_ref()),
        })
    }

    #[cfg(not(unix))]
    fn open_directory(&self, name: &OsStr) -> Result<Self> {
        Self::open(&self.path.join(name), false)
    }

    #[cfg(unix)]
    fn remove_file(&self, name: &OsStr) -> Result<()> {
        use nix::{errno::Errno, unistd::{UnlinkatFlags, unlinkat}};
        use std::os::unix::ffi::OsStrExt;

        let name = CString::new(name.as_bytes()).context("evidence file name contains NUL")?;
        match unlinkat(&self.file, name.as_c_str(), UnlinkatFlags::NoRemoveDir) {
            Ok(()) | Err(Errno::ENOENT) => Ok(()),
            Err(error) => Err(error).context("removing staged evidence file"),
        }
    }

    #[cfg(not(unix))]
    fn remove_file(&self, name: &OsStr) -> Result<()> {
        match fs::remove_file(self.path.join(name)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    #[cfg(unix)]
    fn remove_directory(&self, name: &OsStr) -> Result<()> {
        use nix::{errno::Errno, unistd::{UnlinkatFlags, unlinkat}};
        use std::os::unix::ffi::OsStrExt;

        let name = CString::new(name.as_bytes()).context("staging directory name contains NUL")?;
        match unlinkat(&self.file, name.as_c_str(), UnlinkatFlags::RemoveDir) {
            Ok(()) | Err(Errno::ENOENT) => Ok(()),
            Err(error) => Err(error).context("removing staging evidence directory"),
        }
    }

    #[cfg(not(unix))]
    fn remove_directory(&self, name: &OsStr) -> Result<()> {
        match fs::remove_dir(self.path.join(name)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
    Missing,
}

fn read_file_no_symlinks(path: &Path) -> Result<Vec<u8>> {
    let location = OutputLocation::open(path, false)?;
    let mut file = location
        .parent
        .open_file(&location.name)
        .with_context(|| format!("reading {}", location.path.display()))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("reading {}", location.path.display()))?;
    Ok(bytes)
}

fn require_nonempty_file(path: &Path) -> Result<()> {
    let bytes = read_file_no_symlinks(path)
        .with_context(|| format!("checking required evidence output {}", path.display()))?;
    if bytes.is_empty() {
        bail!("required evidence output is empty: {}", path.display());
    }
    Ok(())
}

fn is_not_found(error: &anyhow::Error) -> bool {
    #[cfg(unix)]
    {
        error.downcast_ref::<nix::errno::Errno>() == Some(&nix::errno::Errno::ENOENT)
    }
    #[cfg(not(unix))]
    {
        error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let location = OutputLocation::open(path, true)?;
    location.parent.lock()?;
    match location.parent.entry_kind(&location.name)? {
        EntryKind::Missing | EntryKind::File => {}
        EntryKind::Symlink => bail!("refusing to replace symlink evidence output {}", path.display()),
        EntryKind::Directory | EntryKind::Other => {
            bail!("evidence output is not a regular file: {}", path.display());
        }
    }
    let name = location.name.to_string_lossy();
    let sequence = WRITE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let staged = OsString::from(format!(".{name}.{}.{}.tmp", std::process::id(), sequence));
    let result = (|| -> Result<()> {
        location.parent.write_new(&staged, bytes)?;
        location.parent.rename(&staged, &location.name)
    })();
    if result.is_err() {
        drop(location.parent.remove_file(&staged));
    }
    result.with_context(|| format!("writing evidence output {}", location.path.display()))
}

fn create_staging_directory(parent: &OutputDirectory, prefix: &str) -> Result<(OsString, OutputDirectory)> {
    let sequence = WRITE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let name = OsString::from(format!(".{prefix}.{}.{}.tmp", std::process::id(), sequence));
    parent.create_directory(&name)?;
    let directory = parent.open_directory(&name)?;
    Ok((name, directory))
}

fn repository_output_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::Prefix(_) | Component::RootDir)
        })
    {
        bail!("evidence path must stay relative to the repository: {}", relative.display());
    }
    let root = fs::canonicalize(root).with_context(|| format!("canonicalizing repository root {}", root.display()))?;
    Ok(root.join(relative))
}
