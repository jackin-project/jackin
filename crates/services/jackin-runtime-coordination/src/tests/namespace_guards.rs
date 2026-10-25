// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::JackinPaths;

use crate::coordination::{
    StatePublishError, ensure_prunable, open_in_namespace, publish_state_bytes_in_namespace,
    read_state_bytes_in_namespace,
};

fn unexpected(message: String) -> std::io::Error {
    std::io::Error::other(message)
}

#[test]
fn rejects_traversal_and_empty_keys() -> std::io::Result<()> {
    let temp = tempfile::tempdir()?;
    let dir = temp.path().join(".jackin-coordination").join("records");
    for key in ["..", ".", "../evil", "a/b", ""] {
        let Err(error) = open_in_namespace(&dir, key) else {
            return Err(unexpected(format!("key {key:?} was accepted")));
        };
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
    Ok(())
}

#[tokio::test]
async fn publishes_and_reads_back_state() -> std::io::Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().canonicalize()?;
    let dir = root.join(".jackin-coordination").join("records");
    let bytes = br#"{"probe":true}"#;
    if let Err(error) = publish_state_bytes_in_namespace(&dir, "probe", bytes, 1024) {
        return Err(unexpected(error.to_string()));
    }
    let stored = read_state_bytes_in_namespace(&dir, "probe", 1024)?;
    assert_eq!(stored, Some(bytes.to_vec()));
    Ok(())
}

#[test]
fn rejects_oversized_state_without_fs_side_effects() -> std::io::Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().canonicalize()?;
    let dir = root.join(".jackin-coordination").join("records");
    let rejected = matches!(
        publish_state_bytes_in_namespace(&dir, "probe", &[0_u8; 16], 8),
        Err(StatePublishError::NotPublished(_))
    );
    assert!(rejected);
    assert!(!dir.exists());
    Ok(())
}

#[test]
fn rejects_prune_of_namespace_but_allows_disjoint_dir() -> std::io::Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().canonicalize()?;
    std::fs::create_dir_all(root.join("home"))?;
    std::fs::create_dir_all(root.join("elsewhere"))?;
    let paths = JackinPaths::for_tests(&root);
    let namespace = paths.home_dir.join(".jackin-coordination");
    let error = match ensure_prunable(&paths, &namespace) {
        Ok(()) => return Err(unexpected("namespace prune was allowed".to_owned())),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    ensure_prunable(&paths, &root.join("elsewhere"))?;
    Ok(())
}
