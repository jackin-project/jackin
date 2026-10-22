// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const FILE_LOCK_CHILD: &str = "JACKIN_USAGE_STORE_FILE_LOCK_CHILD";

#[test]
fn durable_store_open_rejects_explicit_backend_file_lock_disable() -> anyhow::Result<()> {
    if std::env::var_os(FILE_LOCK_CHILD).is_some() {
        let error = require_default_file_lock()
            .expect_err("durable local stores must reject the pinned backend's file-lock opt-out");
        anyhow::ensure!(error.contains("requires Turso's default file lock"));
        return Ok(());
    }

    let status = std::process::Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("tests::case_03::durable_store_open_rejects_explicit_backend_file_lock_disable")
        .arg("--nocapture")
        .env(FILE_LOCK_CHILD, "1")
        .env("LIMBO_DISABLE_FILE_LOCK", "1")
        .status()?;
    anyhow::ensure!(
        status.success(),
        "isolated file-lock rejection check failed"
    );
    Ok(())
}
