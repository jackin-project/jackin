// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use jackin_core::{
    CaptureLimits, CaptureProblem, FileMode, FileSystem, HostPath, SQLITE_MAGIC,
    STORE_QUERY_MAX_STEPS, StdFileSystem, capture, run_bounded,
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

fn word(bytes: &[u8], big_endian: bool) -> u32 {
    let array = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if big_endian {
        u32::from_be_bytes(array)
    } else {
        u32::from_le_bytes(array)
    }
}

fn checksum(data: &[u8], mut one: u32, mut two: u32, big_endian: bool) -> (u32, u32) {
    let (pairs, _) = data.as_chunks::<8>();
    for pair in pairs {
        one = one
            .wrapping_add(word(&pair[0..4], big_endian))
            .wrapping_add(two);
        two = two
            .wrapping_add(word(&pair[4..8], big_endian))
            .wrapping_add(one);
    }
    (one, two)
}

struct Wal {
    big_endian: bool,
    page_size: u32,
    salt: [u8; 8],
    frame_salt: [u8; 8],
}

impl Wal {
    fn standard() -> Wal {
        Wal {
            big_endian: true,
            page_size: 512,
            salt: [1, 2, 3, 4, 5, 6, 7, 8],
            frame_salt: [1, 2, 3, 4, 5, 6, 7, 8],
        }
    }

    fn build(&self, frames: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
        let magic: u32 = if self.big_endian {
            0x377f_0683
        } else {
            0x377f_0682
        };
        let mut out = Vec::new();
        out.extend_from_slice(&magic.to_be_bytes());
        out.extend_from_slice(&3_007_000_u32.to_be_bytes());
        out.extend_from_slice(&self.page_size.to_be_bytes());
        out.extend_from_slice(&0_u32.to_be_bytes());
        out.extend_from_slice(&self.salt);
        let (mut one, mut two) = checksum(&out, 0, 0, self.big_endian);
        out.extend_from_slice(&one.to_be_bytes());
        out.extend_from_slice(&two.to_be_bytes());
        for (page, commit, data) in frames {
            let mut header = Vec::new();
            header.extend_from_slice(&page.to_be_bytes());
            header.extend_from_slice(&commit.to_be_bytes());
            header.extend_from_slice(&self.frame_salt);
            (one, two) = checksum(&header[0..8], one, two, self.big_endian);
            (one, two) = checksum(data, one, two, self.big_endian);
            header.extend_from_slice(&one.to_be_bytes());
            header.extend_from_slice(&two.to_be_bytes());
            out.extend_from_slice(&header);
            out.extend_from_slice(data);
        }
        out
    }
}

fn log(frames: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    Wal::standard().build(frames)
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
    let fs = Arc::new(fixture(&[("agent.db", database(512))]).await?);
    let probe = fs.stall_reads_of("agent.db");
    let running = tokio::spawn({
        let fs = Arc::clone(&fs);
        async move {
            capture(&*fs, &at("agent.db").expect("a path"), LIMITS)
                .await
                .map(|_| ())
        }
    });
    for _ in 0..500 {
        if probe.entered() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(probe.entered(), 1);
    assert_eq!(probe.abandoned(), 0);
    running.abort();
    drop(running.await);
    assert_eq!(probe.abandoned(), 1);
    Ok(())
}

#[tokio::test]
async fn a_little_endian_log_is_applied_like_a_big_endian_one() -> TestResult {
    let wal = Wal {
        big_endian: false,
        ..Wal::standard()
    }
    .build(&[(1, 1, marked_page(9))]);
    let image = image_of(&[("agent.db", page_database(1)), ("agent.db-wal", wal)]).await?;
    assert_eq!(image[100], 9);
    Ok(())
}

#[tokio::test]
async fn a_log_that_cannot_belong_to_the_database_is_unreadable() -> TestResult {
    let mut bad_header = log(&[(1, 1, marked_page(9))]);
    bad_header[28] ^= 0xff;
    let other_pages = Wal {
        page_size: 1024,
        ..Wal::standard()
    }
    .build(&[(1, 1, vec![0; 1024])]);
    let mut odd_database = page_database(1);
    odd_database.extend_from_slice(&[0; 188]);
    for (name, database, wal) in [
        ("header checksum", page_database(1), bad_header),
        ("page size", page_database(1), other_pages),
        (
            "database length",
            odd_database,
            log(&[(1, 1, marked_page(9))]),
        ),
    ] {
        assert_eq!(
            problem(&[("agent.db", database), ("agent.db-wal", wal)]).await?,
            CaptureProblem::LogUnreadable,
            "{name}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn frames_that_do_not_validate_are_not_applied() -> TestResult {
    let mut bad_frame = log(&[(1, 1, marked_page(9))]);
    let last = bad_frame.len() - 1;
    bad_frame[last] ^= 0xff;
    let other_salt = Wal {
        frame_salt: [9; 8],
        ..Wal::standard()
    }
    .build(&[(1, 1, marked_page(9))]);
    let mut truncated = log(&[(1, 1, marked_page(9)), (2, 2, vec![7; 512])]);
    truncated.truncate(truncated.len() - 100);
    for (name, wal, marker) in [
        ("frame checksum", bad_frame, 0),
        ("frame salt", other_salt, 0),
        ("truncated frame", truncated, 9),
    ] {
        let image = image_of(&[("agent.db", page_database(1)), ("agent.db-wal", wal)]).await?;
        assert_eq!(image[100], marker, "{name}");
        assert_eq!(image.len(), 512, "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn a_device_in_place_of_the_database_is_refused() -> TestResult {
    let fs = StdFileSystem::default();
    let refused = capture(&fs, &HostPath::parse(Path::new("/dev/null"))?, REAL)
        .await
        .err();
    assert_eq!(refused, Some(CaptureProblem::NotARegularFile));
    Ok(())
}

async fn swapped_during(
    name: &'static str,
    swap: impl Fn(&MemFs, usize) + Send + Sync + 'static,
    wal: Vec<u8>,
) -> Result<Result<Vec<u8>, CaptureProblem>, Box<dyn std::error::Error>> {
    let fs = fixture(&[("agent.db", page_database(1)), ("agent.db-wal", wal)]).await?;
    fs.after_read_of(name, swap);
    Ok(capture(&fs, &at("agent.db")?, LIMITS)
        .await
        .map(|captured| captured.image.as_slice().to_vec()))
}

#[tokio::test]
async fn a_log_rewritten_between_its_two_reads_is_read_again_until_it_is_stable() -> TestResult {
    let newer = log(&[(1, 1, marked_page(7))]);
    let swapped = {
        let newer = newer.clone();
        move |fs: &MemFs, count: usize| {
            if count == 1 {
                fs.overwrite(&at("agent.db-wal").expect("a path"), newer.clone());
            }
        }
    };
    let image = swapped_during("agent.db-wal", swapped, log(&[(1, 1, marked_page(9))]))
        .await?
        .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(image[100], 7);
    let always = move |fs: &MemFs, count: usize| {
        let marker = u8::try_from(count).unwrap_or(0);
        fs.overwrite(
            &at("agent.db-wal").expect("a path"),
            log(&[(1, 1, marked_page(marker))]),
        );
    };
    let refused = swapped_during("agent.db-wal", always, log(&[(1, 1, marked_page(9))])).await?;
    assert_eq!(refused, Err(CaptureProblem::Changed));
    Ok(())
}

#[tokio::test]
async fn a_log_that_appears_between_the_reads_is_read_again() -> TestResult {
    let fs = fixture(&[("agent.db", page_database(1))]).await?;
    let wal = log(&[(1, 1, marked_page(7))]);
    fs.after_read_of("agent.db", move |fs: &MemFs, count: usize| {
        if count == 3 {
            fs.create(&at("agent.db-wal").expect("a path"), wal.clone());
        }
    });
    let captured = capture(&fs, &at("agent.db")?, LIMITS)
        .await
        .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(captured.image.as_slice()[100], 7);
    Ok(())
}

fn reissue_at(fs: &MemFs, count: usize, target: &str) {
    if let (2, Ok(path)) = (count, at(target)) {
        fs.reissue_inode(&path);
    }
}

async fn image_of(files: &[(&str, Vec<u8>)]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let fs = fixture(files).await?;
    let captured = capture(&fs, &at("agent.db")?, LIMITS)
        .await
        .map_err(|problem| format!("{problem:?}"))?;
    Ok(captured.image.as_slice().to_vec())
}

#[tokio::test]
async fn a_database_log_or_directory_replaced_during_the_capture_is_noticed() -> TestResult {
    for target in ["agent.db", "agent.db-wal", ""] {
        let reads = Arc::new(AtomicUsize::new(0));
        let reissue = {
            let reads = Arc::clone(&reads);
            move |fs: &MemFs, count: usize| {
                reads.store(count, Ordering::SeqCst);
                reissue_at(fs, count, target);
            }
        };
        let image = swapped_during("agent.db", reissue, log(&[(1, 1, marked_page(9))]))
            .await?
            .map_err(|problem| format!("{problem:?}"))?;
        assert_eq!(image[100], 9, "{target:?}");
        assert_eq!(
            reads.load(Ordering::SeqCst),
            5,
            "{target:?}: the capture was not repeated"
        );
        let always = move |fs: &MemFs, _: usize| fs.reissue_inode(&at(target).expect("a path"));
        let refused = swapped_during("agent.db", always, log(&[(1, 1, marked_page(9))])).await?;
        assert_eq!(refused, Err(CaptureProblem::Changed), "{target:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_log_that_only_grows_between_its_reads_is_taken_at_its_first_read() -> TestResult {
    let reads = Arc::new(AtomicUsize::new(0));
    let grown = log(&[(1, 1, marked_page(9)), (2, 2, vec![7; 512])]);
    let grows = {
        let reads = Arc::clone(&reads);
        move |fs: &MemFs, count: usize| {
            reads.store(count, Ordering::SeqCst);
            if count == 1 {
                fs.overwrite(&at("agent.db-wal").expect("a path"), grown.clone());
            }
        }
    };
    let fs = fixture(&[
        ("agent.db", page_database(1)),
        ("agent.db-wal", log(&[(1, 1, marked_page(9))])),
    ])
    .await?;
    let appended = Arc::new(AtomicUsize::new(0));
    fs.after_read_of("agent.db-wal", grows);
    fs.after_read_of("agent.db", {
        let appended = Arc::clone(&appended);
        move |_: &MemFs, count: usize| appended.store(count, Ordering::SeqCst)
    });
    let captured = capture(&fs, &at("agent.db")?, LIMITS)
        .await
        .map_err(|problem| format!("{problem:?}"))?;
    let image = captured.image.as_slice();
    assert_eq!(image.len(), 512);
    assert_eq!(image[100], 9);
    assert_eq!(
        appended.load(Ordering::SeqCst),
        3,
        "the capture was repeated"
    );
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn a_log_whose_first_read_is_only_a_partial_header_is_read_again() -> TestResult {
    let full = log(&[(1, 1, marked_page(7))]);
    let partial = full[..10].to_vec();
    let complete = full.clone();
    let swap = move |fs: &MemFs, count: usize| {
        if count == 1 {
            fs.overwrite(&at("agent.db-wal").expect("a path"), complete.clone());
        }
    };
    let image = swapped_during("agent.db-wal", swap, partial)
        .await?
        .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(image[100], 7);
    Ok(())
}

#[tokio::test]
async fn a_log_that_shrinks_or_restarts_between_its_reads_is_read_again() -> TestResult {
    let restarted = Wal {
        salt: [8, 7, 6, 5, 4, 3, 2, 1],
        frame_salt: [8, 7, 6, 5, 4, 3, 2, 1],
        ..Wal::standard()
    }
    .build(&[(1, 1, marked_page(7)), (2, 2, vec![7; 512])]);
    let swap = move |fs: &MemFs, count: usize| {
        if count == 1 {
            fs.overwrite(&at("agent.db-wal").expect("a path"), restarted.clone());
        }
    };
    let image = swapped_during("agent.db-wal", swap, log(&[(1, 1, marked_page(9))]))
        .await?
        .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(image[100], 7);
    assert_eq!(image.len(), 2 * 512);
    Ok(())
}

async fn settled(flag: &AtomicBool) -> bool {
    for _ in 0..500 {
        if flag.load(Ordering::SeqCst) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

fn spinning(
    started: &Arc<AtomicBool>,
    exited: &Arc<AtomicBool>,
) -> impl FnOnce(jackin_core::QueryBudget) + Send + 'static {
    let (started, exited) = (Arc::clone(started), Arc::clone(exited));
    move |mut budget| {
        started.store(true, Ordering::SeqCst);
        while !budget.spent() {
            std::thread::park_timeout(Duration::from_millis(1));
        }
        exited.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn a_bounded_query_that_the_caller_drops_stops_its_worker() -> TestResult {
    let (started, exited) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let running = tokio::spawn(run_bounded(
        Duration::from_secs(60),
        spinning(&started, &exited),
    ));
    assert!(settled(&started).await, "the worker never started");
    assert!(!exited.load(Ordering::SeqCst));
    running.abort();
    drop(running.await);
    assert!(settled(&exited).await, "the worker outlived its caller");
    Ok(())
}

#[tokio::test]
async fn a_bounded_query_stops_itself_at_its_deadline() -> TestResult {
    let (started, exited) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let outcome = run_bounded(Duration::from_millis(100), spinning(&started, &exited)).await;
    assert_eq!(outcome, Ok(()));
    assert!(exited.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn a_worker_that_ignores_its_budget_is_given_up_on() -> TestResult {
    let release = Arc::new(AtomicBool::new(false));
    let exited = Arc::new(AtomicBool::new(false));
    let worker = {
        let (release, exited) = (Arc::clone(&release), Arc::clone(&exited));
        move |_budget: jackin_core::QueryBudget| {
            while !release.load(Ordering::SeqCst) {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            exited.store(true, Ordering::SeqCst);
        }
    };
    let outcome = run_bounded(Duration::from_millis(100), worker).await;
    assert_eq!(outcome, Err(CaptureProblem::TimedOut));
    assert!(!exited.load(Ordering::SeqCst));
    release.store(true, Ordering::SeqCst);
    assert!(settled(&exited).await);
    Ok(())
}

#[tokio::test]
async fn a_bounded_query_has_a_fixed_budget_of_steps() -> TestResult {
    let calls = run_bounded(Duration::from_secs(60), |mut budget| {
        let mut calls = 0_usize;
        while !budget.spent() {
            calls += 1;
        }
        calls
    })
    .await
    .map_err(|problem| format!("{problem:?}"))?;
    assert_eq!(calls, STORE_QUERY_MAX_STEPS);
    Ok(())
}

#[tokio::test]
async fn only_two_captures_run_at_once_and_a_dropped_one_frees_its_place() -> TestResult {
    let stalled = Arc::new(fixture(&[("agent.db", database(512))]).await?);
    let probe = stalled.stall_reads_of("agent.db");
    let mut holders = Vec::new();
    for _ in 0..2 {
        let fs = Arc::clone(&stalled);
        holders.push(tokio::spawn(async move {
            capture(&*fs, &at("agent.db").expect("a path"), LIMITS)
                .await
                .map(|_| ())
        }));
    }
    for _ in 0..500 {
        if probe.entered() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(probe.entered(), 2);
    let waiting = CaptureLimits {
        deadline: Duration::from_millis(200),
        ..LIMITS
    };
    let free = fixture(&[("agent.db", database(512))]).await?;
    assert_eq!(
        capture(&free, &at("agent.db")?, waiting).await.err(),
        Some(CaptureProblem::TimedOut)
    );
    for holder in &holders {
        holder.abort();
    }
    for holder in holders {
        drop(holder.await);
    }
    capture(&free, &at("agent.db")?, LIMITS)
        .await
        .map_err(|problem| format!("{problem:?}"))?;
    Ok(())
}
