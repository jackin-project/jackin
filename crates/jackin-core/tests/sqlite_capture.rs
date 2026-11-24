// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{
    CaptureLimits, CaptureProblem, FileMode, FileSystem, HostPath, SQLITE_MAGIC, capture,
};
use jackin_testkit::MemFs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const LIMITS: CaptureLimits = CaptureLimits {
    database_bytes: 4096,
    log_bytes: 1024,
    image_bytes: 3000,
};

fn at(name: &str) -> Result<HostPath, Box<dyn std::error::Error>> {
    Ok(HostPath::parse(&Path::new("/store").join(name))?)
}

async fn fixture(files: &[(&str, Vec<u8>)]) -> Result<MemFs, Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    fs.create_dir_all(&at("")?).await?;
    for (name, bytes) in files {
        fs.write_new(&at(name)?, bytes, FileMode(0o600)).await?;
    }
    Ok(fs)
}

fn database(len: usize) -> Vec<u8> {
    let mut bytes = SQLITE_MAGIC.to_vec();
    bytes.resize(len, 0);
    bytes
}

async fn problem(files: &[(&str, Vec<u8>)]) -> Result<CaptureProblem, Box<dyn std::error::Error>> {
    let fs = fixture(files).await?;
    capture(&fs, &at("agent.db")?, LIMITS)
        .await
        .err()
        .ok_or_else(|| "the capture succeeded".into())
}

#[tokio::test]
async fn a_database_over_its_cap_is_refused() -> TestResult {
    assert_eq!(
        problem(&[("agent.db", database(LIMITS.database_bytes + 1))]).await?,
        CaptureProblem::DatabaseTooLarge
    );
    Ok(())
}

#[tokio::test]
async fn a_log_over_its_cap_is_refused() -> TestResult {
    assert_eq!(
        problem(&[
            ("agent.db", database(512)),
            ("agent.db-wal", vec![0; LIMITS.log_bytes + 1]),
        ])
        .await?,
        CaptureProblem::LogTooLarge
    );
    Ok(())
}

#[tokio::test]
async fn a_database_larger_than_the_image_cap_is_refused() -> TestResult {
    assert_eq!(
        problem(&[("agent.db", database(LIMITS.image_bytes + 1))]).await?,
        CaptureProblem::ImageTooLarge
    );
    Ok(())
}

#[tokio::test]
async fn a_file_that_is_not_a_database_falls_through_whatever_its_size() -> TestResult {
    assert_eq!(
        problem(&[("agent.db", vec![b'x'; LIMITS.database_bytes * 4])]).await?,
        CaptureProblem::NotSqlite
    );
    Ok(())
}

#[tokio::test]
async fn a_log_without_a_valid_header_is_unreadable() -> TestResult {
    assert_eq!(
        problem(&[("agent.db", database(512)), ("agent.db-wal", vec![7; 64]),]).await?,
        CaptureProblem::LogUnreadable
    );
    Ok(())
}

#[tokio::test]
async fn a_missing_database_is_missing_and_a_database_alone_is_the_image() -> TestResult {
    assert_eq!(problem(&[]).await?, CaptureProblem::Missing);
    let fs = fixture(&[("agent.db", database(2048))]).await?;
    let captured = capture(&fs, &at("agent.db")?, LIMITS)
        .await
        .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(captured.image.as_slice().len(), 2048);
    Ok(())
}
