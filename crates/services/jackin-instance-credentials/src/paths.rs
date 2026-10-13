// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Auth path rejection and private file writers.

use std::path::Path;

use crate::MAX_AUTH_SOURCE_FILE_BYTES;
use crate::auth_directory;

/// Reject symlinks at `path` to prevent a compromised role from
/// redirecting host-side writes to arbitrary files.
///
/// The role's `.claude/` directory is mounted read-write into the
/// container, so an role could replace `.credentials.json` with a
/// symlink.  Without this check, the next `write_private_file` or
/// `repair_permissions` call would follow the symlink and overwrite
/// or chmod the target on the host.
pub fn reject_symlink(path: &Path) -> anyhow::Result<()> {
    // Use symlink_metadata (lstat) — regular metadata() follows symlinks.
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        anyhow::ensure!(
            !meta.file_type().is_symlink(),
            "refusing to write through symlink at {}; \
             this may indicate a compromised role state — \
             remove the symlink and retry",
            path.display()
        );
    }
    Ok(())
}

/// Reject symlink traversal through the destination's existing parent
/// directories as well as at the final path. Missing ancestors are allowed so
/// callers can create a new private tree after this check.
pub fn reject_auth_path(path: &Path) -> anyhow::Result<()> {
    reject_symlink(path)?;
    let mut ancestor = path.parent();
    while let Some(current) = ancestor {
        if !is_platform_root_alias(current) {
            match std::fs::symlink_metadata(current) {
                Ok(meta) => {
                    anyhow::ensure!(
                        !meta.file_type().is_symlink(),
                        "refusing to use auth path through symlink at {}; remove the symlink and retry",
                        current.display()
                    );
                    anyhow::ensure!(
                        meta.is_dir(),
                        "refusing to use auth path through non-directory {}; remove it and retry",
                        current.display()
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        ancestor = current.parent();
    }
    Ok(())
}

/// macOS exposes these root directories as immutable platform aliases (for
/// example `/var` → `/private/var`). They are outside Jackin-owned state and
/// must not make every otherwise-safe temporary test or data path fail.
#[cfg(target_os = "macos")]
pub fn is_platform_root_alias(path: &Path) -> bool {
    matches!(path, p if p == Path::new("/etc")
        || p == Path::new("/tmp")
        || p == Path::new("/var"))
}

#[cfg(not(target_os = "macos"))]
pub const fn is_platform_root_alias(_path: &Path) -> bool {
    false
}

/// Write a file with restricted permissions (`0o600` on Unix) since it
/// may contain authentication credentials.
///
/// Rejects symlinks to prevent a compromised role from redirecting
/// writes to arbitrary host paths.  Uses `tempfile::NamedTempFile` to
/// create an unpredictable temp file (opened with `O_EXCL`, so a
/// pre-planted symlink at the temp path is impossible), then renames
/// it to the destination — closing the TOCTOU window entirely.
pub fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    write_private_bytes(path, content.as_bytes())
}

/// Write raw bytes to `path` with `0o600` permissions, symlink-safe and atomic.
pub fn write_private_bytes(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        auth_directory::replace_private_file(path, content)
    }

    #[cfg(not(unix))]
    {
        reject_auth_path(path)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Create `path` with `content` at `0o600` only when it does not yet exist.
///
/// Race-free via `O_CREAT|O_EXCL`; on `EEXIST` (file already present)
/// the function returns `Ok(())` and leaves the existing content
/// untouched. Use when a process-private skeleton must be seeded
/// before a downstream consumer (e.g. the Claude CLI) may persist
/// real state into the same path.
pub fn create_private_file_if_absent(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        auth_directory::create_private_file_if_absent(path, content)
    }

    #[cfg(not(unix))]
    {
        use anyhow::Context;
        reject_auth_path(path)?;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "auth file provisioning is called from spawn_blocking during launch"
        )]
        match opts.open(path) {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(content)
                    .with_context(|| format!("writing private skeleton at {}", path.display()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(anyhow::Error::new(error)
                .context(format!("creating private skeleton at {}", path.display()))),
        }
    }
}

pub fn read_bounded_local_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    #[expect(
        clippy::disallowed_methods,
        reason = "bounded auth reads run in joined blocking launch/prewarm workers or scoped provisioning OS threads"
    )]
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take((MAX_AUTH_SOURCE_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= MAX_AUTH_SOURCE_FILE_BYTES,
        "selected auth source file exceeds the size limit: {}",
        path.display()
    );
    Ok(bytes)
}
