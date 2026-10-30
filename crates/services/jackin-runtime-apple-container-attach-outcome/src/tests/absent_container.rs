// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering};

use jackin_core::JackinPaths;

fn unique_root(case: &str) -> anyhow::Result<std::path::PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "jackin-attach-outcome-{case}-{pid}-{id}",
        pid = std::process::id()
    ));
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

fn state_entry_count(paths: &JackinPaths) -> anyhow::Result<usize> {
    let count = match std::fs::read_dir(&paths.data_dir) {
        Ok(entries) => entries.count(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error.into()),
    };
    Ok(count)
}

#[tokio::test]
async fn absent_container_records_nothing() -> anyhow::Result<()> {
    let root = unique_root("absent")?;
    let paths = JackinPaths::for_tests(&root);
    crate::attach_outcome::record_attach_outcome(&paths, "jackin-test-absent-container", None)
        .await;
    assert_eq!(state_entry_count(&paths)?, 0);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[tokio::test]
async fn absent_container_with_exit_code_records_nothing() -> anyhow::Result<()> {
    let root = unique_root("exit-code")?;
    let paths = JackinPaths::for_tests(&root);
    crate::attach_outcome::record_attach_outcome(&paths, "jackin-test-absent-container", Some(3))
        .await;
    assert_eq!(state_entry_count(&paths)?, 0);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
