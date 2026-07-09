use std::fs;

use super::{check, crate_member_dirs};

#[cfg(unix)]
use std::os::unix::fs as unix_fs;

#[test]
#[cfg(unix)]
fn check_accepts_claude_symlink_to_agents() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("AGENTS.md"), "# Rules\n").unwrap();
    unix_fs::symlink("AGENTS.md", root.join("CLAUDE.md")).unwrap();

    check(root, &["."]).unwrap();
}

#[test]
fn check_rejects_regular_claude_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("AGENTS.md"), "# Rules\n").unwrap();
    fs::write(root.join("CLAUDE.md"), "# Copy\n").unwrap();

    let err = check(root, &["."]).unwrap_err().to_string();

    assert!(err.contains("not a symlink"), "{err}");
}

/// `crate_member_dirs` discovers every `crates/*/` directory that owns a
/// `Cargo.toml`, and ignores sibling files and crate-less directories.
#[test]
fn crate_member_dirs_discovers_member_crates() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/jackin-core/src")).unwrap();
    fs::write(root.join("crates/jackin-core/Cargo.toml"), "").unwrap();
    fs::create_dir_all(root.join("crates/jackin-term/src")).unwrap();
    fs::write(root.join("crates/jackin-term/Cargo.toml"), "").unwrap();
    // A directory without a Cargo.toml is not a member crate.
    fs::create_dir_all(root.join("crates/not-a-crate")).unwrap();
    // A file directly under crates/ (e.g. AGENTS.md) must be skipped.
    fs::write(root.join("crates/AGENTS.md"), "").unwrap();

    let mut dirs = crate_member_dirs(root).unwrap();
    dirs.sort();

    assert_eq!(
        dirs,
        vec![
            "crates/jackin-core".to_owned(),
            "crates/jackin-term".to_owned(),
        ]
    );
}

/// The per-crate scan means a member crate missing its `CLAUDE.md` symlink is a
/// real violation, not a no-op.
#[test]
#[cfg(unix)]
fn check_flags_crate_missing_claude_symlink() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let crate_dir = root.join("crates/jackin-core");
    fs::create_dir_all(&crate_dir).unwrap();
    fs::write(crate_dir.join("Cargo.toml"), "").unwrap();
    fs::write(crate_dir.join("AGENTS.md"), "# Rules\n").unwrap();
    // No CLAUDE.md symlink.

    let dirs = crate_member_dirs(root).unwrap();
    let dir_refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
    let err = check(root, &dir_refs).unwrap_err().to_string();

    assert!(err.contains("missing CLAUDE.md"), "{err}");
}
