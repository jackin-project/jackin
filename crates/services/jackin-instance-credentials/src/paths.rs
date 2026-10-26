// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use crate::MAX_AUTH_SOURCE_FILE_BYTES;
use crate::auth_directory;

pub fn reject_symlink(path: &Path) -> anyhow::Result<()> {
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

pub fn write_private_file(path: &Path, content: &str) -> anyhow::Result<()> {
    write_private_bytes(path, content.as_bytes())
}

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
