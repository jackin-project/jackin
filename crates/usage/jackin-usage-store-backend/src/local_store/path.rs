use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::local_store::StoreError;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct FileIdentity {
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

impl FileIdentity {
    pub(super) fn from_metadata(metadata: &Metadata) -> Option<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Some(Self {
                device: metadata.dev(),
                inode: metadata.ino(),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = metadata;
            None
        }
    }

    #[cfg(unix)]
    pub(super) fn is_multiply_linked(metadata: &Metadata) -> bool {
        use std::os::unix::fs::MetadataExt;
        metadata.nlink() > 1
    }

    #[cfg(not(unix))]
    pub(super) fn is_multiply_linked(_metadata: &Metadata) -> bool {
        false
    }
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedPath {
    pub path: PathBuf,
    pub parent: PathBuf,
    pub identity: Option<FileIdentity>,
}

pub(super) fn resolve_source_path(path: &Path) -> Result<ResolvedPath, StoreError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| StoreError::io("resolve current directory", source))?
            .join(path)
    };
    let normalized = lexical_normalize(&absolute)?;
    let leaf = normalized
        .file_name()
        .ok_or(StoreError::UnsupportedSourcePath)?
        .to_os_string();
    let parent = normalized
        .parent()
        .ok_or(StoreError::UnsupportedSourcePath)?;
    let canonical_parent = canonicalize_with_missing_suffix(parent)?;
    let canonical_path = canonical_parent.join(leaf);

    let identity = match fs::symlink_metadata(&canonical_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                return Err(StoreError::UnsafeSourceFile);
            }
            if FileIdentity::is_multiply_linked(&metadata) {
                return Err(StoreError::HardLinkedSourceFile);
            }
            FileIdentity::from_metadata(&metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => return Err(StoreError::io("inspect source file", source)),
    };

    Ok(ResolvedPath {
        path: canonical_path,
        parent: canonical_parent,
        identity,
    })
}

pub(super) fn resolve_legacy_path(path: &Path) -> Result<ResolvedPath, StoreError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| StoreError::io("resolve current directory", source))?
            .join(path)
    };
    let normalized = lexical_normalize(&absolute)?;
    if let Ok(canonical) = fs::canonicalize(&normalized) {
        let metadata = fs::metadata(&canonical)
            .map_err(|source| StoreError::io("inspect legacy source file", source))?;
        let parent = canonical
            .parent()
            .ok_or(StoreError::UnsupportedSourcePath)?
            .to_path_buf();
        return Ok(ResolvedPath {
            path: canonical,
            parent,
            identity: metadata
                .is_file()
                .then(|| FileIdentity::from_metadata(&metadata))
                .flatten(),
        });
    }

    let leaf = normalized
        .file_name()
        .ok_or(StoreError::UnsupportedSourcePath)?
        .to_os_string();
    let parent = normalized
        .parent()
        .ok_or(StoreError::UnsupportedSourcePath)?;
    let canonical_parent = canonicalize_with_missing_suffix(parent)?;
    Ok(ResolvedPath {
        path: canonical_parent.join(leaf),
        parent: canonical_parent,
        identity: None,
    })
}

pub(super) fn ensure_same_source(
    expected: &ResolvedPath,
    actual: &ResolvedPath,
) -> Result<(), StoreError> {
    if expected.path != actual.path || expected.identity != actual.identity {
        return Err(StoreError::SourceIdentityChanged);
    }
    Ok(())
}

pub(super) fn ensure_managed_sql_path(path: &Path) -> Result<(), StoreError> {
    let value = path.to_str().ok_or(StoreError::UnsupportedSourcePath)?;
    if !value.is_ascii() || value.contains('\'') || value.contains('"') {
        return Err(StoreError::UnsupportedSourcePath);
    }
    Ok(())
}

pub(super) fn path_digest(path: &Path) -> String {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let lossy = path.to_string_lossy();
    #[cfg(not(unix))]
    let bytes = lossy.as_bytes();
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn identity_fields(identity: Option<FileIdentity>) -> (String, String) {
    match identity {
        #[cfg(unix)]
        Some(identity) => (identity.device.to_string(), identity.inode.to_string()),
        #[cfg(not(unix))]
        Some(_) => ("unknown".to_owned(), "unknown".to_owned()),
        None => ("absent".to_owned(), "absent".to_owned()),
    }
}

pub(super) fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn lexical_normalize(path: &Path) -> Result<PathBuf, StoreError> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return Err(StoreError::UnsupportedSourcePath);
                }
            }
            Component::Normal(value) => result.push(value),
        }
    }
    if result.is_absolute() {
        Ok(result)
    } else {
        Err(StoreError::UnsupportedSourcePath)
    }
}

fn canonicalize_with_missing_suffix(path: &Path) -> Result<PathBuf, StoreError> {
    let mut cursor = path;
    let mut missing = Vec::new();
    loop {
        match fs::canonicalize(cursor) {
            Ok(mut canonical) => {
                for component in missing.into_iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                match fs::symlink_metadata(cursor) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Ok(_) => return Err(StoreError::UnsupportedSourcePath),
                    Err(error) => return Err(StoreError::io("resolve source parent", error)),
                }
                let name = cursor
                    .file_name()
                    .ok_or(StoreError::UnsupportedSourcePath)?
                    .to_os_string();
                missing.push(name);
                cursor = cursor.parent().ok_or(StoreError::UnsupportedSourcePath)?;
            }
            Err(source) => return Err(StoreError::io("resolve source parent", source)),
        }
    }
}
