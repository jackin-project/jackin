//! Atomic file writes and workspace filename validation.
//!
//! Uses a per-process counter mixed with the PID so concurrent migrations
//! cannot clobber each other's staged files. Not responsible for config
//! deserialization, migration logic, or mount resolution.

use anyhow::Context;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

// Per-process counter mixed with the PID into the staged-write filename.
// Combined with the PID it produces unique suffixes across concurrent
// migrations, so two writers cannot clobber each other's staged file before
// rename, and a leftover staged file cannot truncate an operator-created
// `<name>.tmp` workspace file.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn validate_workspace_file_stem(name: &str) -> anyhow::Result<()> {
    jackin_core::WorkspaceName::parse(name).map(drop).map_err(Into::into)
}

pub fn atomic_write(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }
    // Place the `.tmp` marker mid-filename rather than as the extension so
    // `load_workspace_files`'s `extension == "toml"` filter ignores leftover
    // staged files. PID + counter make the suffix unique across processes
    // and concurrent in-process writers.
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut staged_name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    staged_name.push(format!(".tmp.{}.{counter}", std::process::id()));
    let tmp = path.with_file_name(staged_name);

    let staged = stage_write(&tmp, contents);
    if let Err(err) = staged {
        drop(std::fs::remove_file(&tmp));
        return Err(err);
    }

    if let Err(rename_err) = std::fs::rename(&tmp, path) {
        // Rename failure leaves the staged file behind; clean up so it does
        // not accumulate.
        drop(std::fs::remove_file(&tmp));
        return Err(rename_err)
            .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()));
    }
    Ok(())
}

fn stage_write(tmp: &Path, contents: &str) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        #[expect(
            clippy::disallowed_methods,
            reason = "config persistence is caller-governed and not run from render loops"
        )]
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }

    #[cfg(not(unix))]
    std::fs::write(tmp, contents)?;

    Ok(())
}
