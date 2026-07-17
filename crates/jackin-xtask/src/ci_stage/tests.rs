use std::fs;

use tempfile::tempdir;

use super::{TOOLS, copy_cached_tools, copy_dir_files};

#[test]
fn cached_tool_set_is_atomic_and_complete() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(&source).unwrap();
    fs::create_dir_all(&destination).unwrap();
    for tool in TOOLS {
        fs::write(source.join(tool), tool).unwrap();
    }

    copy_cached_tools(&source, &destination).unwrap();

    for tool in TOOLS {
        assert_eq!(fs::read_to_string(destination.join(tool)).unwrap(), *tool);
    }
}

#[test]
fn combined_stage_copies_only_files() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(source.join("ignored-directory")).unwrap();
    fs::create_dir_all(&destination).unwrap();
    fs::write(source.join("jackin-xtask"), "binary").unwrap();

    copy_dir_files(&source, &destination).unwrap();

    assert_eq!(
        fs::read_to_string(destination.join("jackin-xtask")).unwrap(),
        "binary"
    );
    assert!(!destination.join("ignored-directory").exists());
}
