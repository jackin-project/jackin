use std::fs;

use super::check;

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
