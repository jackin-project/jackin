use std::fs;

use tempfile::tempdir;

use super::{excluded, reusable_paths};

#[test]
fn reusable_paths_keep_outputs_and_drop_transport_state() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir_all(target.join("debug/deps")).unwrap();
    fs::create_dir_all(target.join("debug/incremental/state")).unwrap();
    fs::create_dir_all(target.join("nextest/default")).unwrap();
    fs::write(target.join("debug/deps/libexample.rlib"), b"output").unwrap();
    fs::write(target.join("debug/incremental/state/data"), b"state").unwrap();
    fs::write(target.join("nextest/default/junit.xml"), b"report").unwrap();

    let paths = reusable_paths(&target).unwrap();

    assert_eq!(paths, vec![target.join("debug/deps/libexample.rlib")]);
}

#[test]
fn transport_exclusions_are_semantic_directories() {
    assert!(excluded("debug/incremental/object".as_ref()));
    assert!(excluded("nextest/ci/junit.xml".as_ref()));
    assert!(excluded("telemetry-volume-ratchet.json".as_ref()));
    assert!(!excluded("debug/deps/libexample.rlib".as_ref()));
}
