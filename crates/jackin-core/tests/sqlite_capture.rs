// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::time::Duration;

use jackin_core::{
    CaptureLimits, CaptureProblem, FileMode, FileSystem, HostPath, SQLITE_MAGIC, StdFileSystem,
    capture,
};
use jackin_testkit::MemFs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const LIMITS: CaptureLimits = CaptureLimits {
    database_bytes: 4096,
    log_bytes: 2048,
    image_bytes: 3000,
    deadline: Duration::from_secs(30),
};

const REAL: CaptureLimits = CaptureLimits {
    database_bytes: 4096,
    log_bytes: 1024,
    image_bytes: 8192,
    deadline: Duration::from_secs(30),
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

#[tokio::test]
async fn a_database_that_is_a_symbolic_link_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let real = dir.path().join("real.db");
    std::fs::write(&real, database(512))?;
    let link = dir.path().join("agent.db");
    std::os::unix::fs::symlink(&real, &link)?;
    let fs = StdFileSystem::default();
    let refused = capture(&fs, &HostPath::parse(&link)?, LIMITS)
        .await
        .err()
        .ok_or("the capture followed the link")?;
    assert_eq!(refused, CaptureProblem::Symlink);
    capture(&fs, &HostPath::parse(&real)?, LIMITS)
        .await
        .map_err(|problem| format!("{problem:?}"))?;
    Ok(())
}

#[tokio::test]
async fn a_hot_rollback_journal_makes_the_capture_retry_and_then_give_up() -> TestResult {
    let mut hot = vec![0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7];
    hot.resize(64, 0);
    assert_eq!(
        problem(&[("agent.db", database(512)), ("agent.db-journal", hot)]).await?,
        CaptureProblem::Changed
    );
    let fs = fixture(&[
        ("agent.db", database(512)),
        ("agent.db-journal", vec![0; 64]),
    ])
    .await?;
    capture(&fs, &at("agent.db")?, LIMITS)
        .await
        .map_err(|problem| format!("{problem:?}"))?;
    Ok(())
}

#[tokio::test]
async fn a_database_reached_through_a_linked_directory_is_still_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let real = dir.path().join("real");
    std::fs::create_dir(&real)?;
    std::fs::write(real.join("agent.db"), database(512))?;
    std::os::unix::fs::symlink(&real, dir.path().join("via"))?;
    let fs = StdFileSystem::default();
    capture(
        &fs,
        &HostPath::parse(&dir.path().join("via").join("agent.db"))?,
        LIMITS,
    )
    .await
    .map_err(|problem| format!("{problem:?}"))?;
    Ok(())
}

fn page_database(pages: usize) -> Vec<u8> {
    let mut bytes = database(pages * 512);
    bytes[16..18].copy_from_slice(&512_u16.to_be_bytes());
    bytes
}

fn checksum(data: &[u8], mut one: u32, mut two: u32) -> (u32, u32) {
    for pair in data.chunks_exact(8) {
        let first = u32::from_be_bytes([pair[0], pair[1], pair[2], pair[3]]);
        let second = u32::from_be_bytes([pair[4], pair[5], pair[6], pair[7]]);
        one = one.wrapping_add(first).wrapping_add(two);
        two = two.wrapping_add(second).wrapping_add(one);
    }
    (one, two)
}

fn log(frames: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x377f_0683_u32.to_be_bytes());
    out.extend_from_slice(&3_007_000_u32.to_be_bytes());
    out.extend_from_slice(&512_u32.to_be_bytes());
    out.extend_from_slice(&0_u32.to_be_bytes());
    out.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let (mut one, mut two) = checksum(&out, 0, 0);
    out.extend_from_slice(&one.to_be_bytes());
    out.extend_from_slice(&two.to_be_bytes());
    for (page, commit, data) in frames {
        let mut header = Vec::new();
        header.extend_from_slice(&page.to_be_bytes());
        header.extend_from_slice(&commit.to_be_bytes());
        header.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        (one, two) = checksum(&header[0..8], one, two);
        (one, two) = checksum(data, one, two);
        header.extend_from_slice(&one.to_be_bytes());
        header.extend_from_slice(&two.to_be_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(data);
    }
    out
}

fn marked_page(marker: u8) -> Vec<u8> {
    let mut page = page_database(1);
    page[100] = marker;
    page
}

#[tokio::test]
async fn a_log_whose_commit_size_exceeds_the_image_cap_is_refused() -> TestResult {
    for pages in [100, u32::MAX] {
        let wal = log(&[(1, pages, marked_page(9))]);
        assert_eq!(
            problem(&[("agent.db", page_database(1)), ("agent.db-wal", wal)]).await?,
            CaptureProblem::ImageTooLarge,
            "commit size {pages}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_committed_log_grows_the_image_within_its_cap() -> TestResult {
    let wal = log(&[(1, 0, marked_page(9)), (2, 4, vec![7; 512])]);
    let fs = fixture(&[("agent.db", page_database(1)), ("agent.db-wal", wal)]).await?;
    let captured = capture(&fs, &at("agent.db")?, LIMITS)
        .await
        .map_err(|problem| format!("{problem:?}"))?;
    let image = captured.image.as_slice();
    assert_eq!(image.len(), 4 * 512);
    assert_eq!(image[100], 9);
    assert_eq!(image[512], 7);
    assert_eq!(&image[28..32], &4_u32.to_be_bytes());
    Ok(())
}

#[tokio::test]
async fn frames_after_the_last_commit_are_not_applied() -> TestResult {
    let wal = log(&[(1, 0, marked_page(9))]);
    let fs = fixture(&[("agent.db", page_database(1)), ("agent.db-wal", wal)]).await?;
    let captured = capture(&fs, &at("agent.db")?, LIMITS)
        .await
        .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(captured.image.as_slice().len(), 512);
    assert_eq!(captured.image.as_slice()[100], 0);
    Ok(())
}

async fn real_problem(
    files: &[(&str, Vec<u8>)],
) -> Result<CaptureProblem, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    for (name, bytes) in files {
        std::fs::write(dir.path().join(name), bytes)?;
    }
    capture(
        &StdFileSystem::default(),
        &HostPath::parse(&dir.path().join("agent.db"))?,
        REAL,
    )
    .await
    .err()
    .ok_or_else(|| "the capture succeeded".into())
}

#[tokio::test]
async fn a_real_database_over_its_cap_is_refused_and_one_at_the_cap_is_read() -> TestResult {
    assert_eq!(
        real_problem(&[("agent.db", database(REAL.database_bytes + 1))]).await?,
        CaptureProblem::DatabaseTooLarge
    );
    assert_eq!(
        real_problem(&[("agent.db", database(REAL.database_bytes * 64))]).await?,
        CaptureProblem::DatabaseTooLarge
    );
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("agent.db"), database(REAL.database_bytes))?;
    let captured = capture(
        &StdFileSystem::default(),
        &HostPath::parse(&dir.path().join("agent.db"))?,
        REAL,
    )
    .await
    .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(captured.image.as_slice().len(), REAL.database_bytes);
    Ok(())
}

#[tokio::test]
async fn a_real_log_over_its_cap_is_refused_and_one_at_the_cap_is_judged_on_its_header()
-> TestResult {
    assert_eq!(
        real_problem(&[
            ("agent.db", database(512)),
            ("agent.db-wal", vec![0; REAL.log_bytes + 1]),
        ])
        .await?,
        CaptureProblem::LogTooLarge
    );
    assert_eq!(
        real_problem(&[
            ("agent.db", database(512)),
            ("agent.db-wal", vec![0; REAL.log_bytes]),
        ])
        .await?,
        CaptureProblem::LogUnreadable
    );
    Ok(())
}

#[tokio::test]
async fn a_real_hot_rollback_journal_is_noticed_from_its_first_bytes() -> TestResult {
    let mut hot = vec![0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7];
    hot.resize(REAL.log_bytes * 4, 0);
    assert_eq!(
        real_problem(&[("agent.db", database(512)), ("agent.db-journal", hot)]).await?,
        CaptureProblem::Changed
    );
    Ok(())
}

#[tokio::test]
async fn a_real_log_that_is_a_symbolic_link_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("agent.db"), database(512))?;
    std::fs::write(dir.path().join("elsewhere"), log(&[(1, 1, marked_page(9))]))?;
    std::os::unix::fs::symlink(
        dir.path().join("elsewhere"),
        dir.path().join("agent.db-wal"),
    )?;
    let refused = capture(
        &StdFileSystem::default(),
        &HostPath::parse(&dir.path().join("agent.db"))?,
        REAL,
    )
    .await
    .err()
    .ok_or("the capture followed the linked log")?;
    assert_eq!(refused, CaptureProblem::LogUnreadable);
    Ok(())
}

#[tokio::test]
async fn a_pipe_in_place_of_the_database_or_the_log_is_refused_without_blocking() -> TestResult {
    for (pipe, expected) in [
        ("agent.db", CaptureProblem::NotARegularFile),
        ("agent.db-wal", CaptureProblem::LogUnreadable),
    ] {
        let dir = tempfile::tempdir()?;
        if pipe != "agent.db" {
            std::fs::write(dir.path().join("agent.db"), database(512))?;
        }
        let made = std::process::Command::new("mkfifo")
            .arg(dir.path().join(pipe))
            .status()?;
        assert!(made.success(), "mkfifo failed");
        let finished = tokio::time::timeout(
            Duration::from_secs(10),
            capture(
                &StdFileSystem::default(),
                &HostPath::parse(&dir.path().join("agent.db"))?,
                REAL,
            ),
        )
        .await
        .map_err(|_| format!("the capture blocked on the pipe {pipe}"))?;
        assert_eq!(finished.err(), Some(expected), "{pipe}");
    }
    Ok(())
}

#[tokio::test]
async fn a_read_that_never_returns_is_given_up_at_the_deadline() -> TestResult {
    let fs = fixture(&[("agent.db", database(512))]).await?;
    let probe = fs.stall_reads_of("agent.db");
    let limits = CaptureLimits {
        deadline: Duration::from_millis(100),
        ..LIMITS
    };
    let refused = tokio::time::timeout(
        Duration::from_secs(10),
        capture(&fs, &at("agent.db")?, limits),
    )
    .await
    .map_err(|_| "the capture outlived its deadline")?
    .err();
    assert_eq!(refused, Some(CaptureProblem::TimedOut));
    assert_eq!(probe.entered(), 1);
    assert_eq!(probe.abandoned(), 1);
    Ok(())
}

#[tokio::test]
async fn a_capture_that_the_caller_drops_leaves_no_read_behind() -> TestResult {
    let fs = fixture(&[("agent.db", database(512))]).await?;
    let probe = fs.stall_reads_of("agent.db");
    let waited = tokio::time::timeout(
        Duration::from_millis(100),
        capture(&fs, &at("agent.db")?, LIMITS),
    )
    .await;
    assert!(waited.is_err(), "the stalled capture finished");
    assert_eq!(probe.entered(), 1);
    assert_eq!(probe.abandoned(), 1);
    Ok(())
}
