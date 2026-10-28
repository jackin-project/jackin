// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use rusqlite::Connection;
use tempfile::TempDir;

const FIXTURE_DIRECTORY: &str = "crates/services/jackin-instance-roles/src/tests/fixtures";
const WAL_HEADER_BYTES: usize = 32;
const WAL_FRAME_HEADER_BYTES: usize = 24;
type TestResult<T> = Result<T, Box<dyn Error>>;

#[test]
fn generated_artifacts_preserve_all_six_semantic_families() -> TestResult<()> {
    let root = repository_root()?;
    let result = run_fixture_omp(root.path())?;
    assert!(
        result.status.success(),
        "fixture-omp failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let output = root.path().join(FIXTURE_DIRECTORY);
    assert_artifact_inventory(&output)?;
    let current_db = fs::read(output.join("omp-real-current.db"))?;
    let current_wal = fs::read(output.join("omp-real-current.db-wal"))?;
    assert!(!contains(&current_db, b"fixture-wal-current-token"));
    assert!(contains(&current_wal, b"fixture-wal-current-token"));

    let checkpointed_db = fs::read(output.join("omp-real-checkpointed.db"))?;
    let checkpointed_wal = fs::read(output.join("omp-real-checkpointed.db-wal"))?;
    assert!(contains(&checkpointed_db, b"fixture-wal-current-token"));
    assert!(checkpointed_wal.is_empty());

    let schema_db = fs::read(output.join("omp-schema-wal-only.db"))?;
    let schema_wal = fs::read(output.join("omp-schema-wal-only.db-wal"))?;
    assert!(!contains(&schema_db, b"credentials"));
    assert!(!contains(&schema_db, b"fixture-schema-wal-token"));
    assert!(contains(&schema_wal, b"credentials"));
    assert!(contains(&schema_wal, b"fixture-schema-wal-token"));

    let reused_wal = fs::read(output.join("omp-reused-stale-suffix.db-wal"))?;
    let spill_db = fs::read(output.join("omp-reused-uncommitted-stale-suffix.db"))?;
    let spill_wal = fs::read(output.join("omp-reused-uncommitted-stale-suffix.db-wal"))?;
    assert_stale_generation_tail(&reused_wal);
    assert_stale_generation_tail(&spill_wal);
    assert!(contains(&spill_db, b"fixture-main-selected-token"));
    assert!(contains(&spill_wal, b"fixture-uncommitted-marker"));

    let big_endian_db = fs::read(output.join("omp-real-current-big-endian.db"))?;
    let big_endian_wal = fs::read(output.join("omp-real-current-big-endian.db-wal"))?;
    assert_eq!(big_endian_db, current_db);
    assert!(big_endian_wal.len() >= 4);
    assert_eq!(
        u32::from_be_bytes([
            big_endian_wal[0],
            big_endian_wal[1],
            big_endian_wal[2],
            big_endian_wal[3]
        ]),
        0x377f_0683
    );

    let scratch = root.path().join("pair-reads");
    fs::create_dir(&scratch)?;
    for (stem, expected) in [
        ("omp-real-current", "fixture-wal-current-token"),
        ("omp-real-checkpointed", "fixture-wal-current-token"),
        ("omp-schema-wal-only", "fixture-schema-wal-token"),
        ("omp-reused-stale-suffix", "fixture-reused-current-token"),
        (
            "omp-reused-uncommitted-stale-suffix",
            "fixture-main-selected-token",
        ),
        ("omp-real-current-big-endian", "fixture-wal-current-token"),
    ] {
        assert_sqlite_pair(&output, &scratch, stem, expected)?;
    }
    Ok(())
}

#[test]
fn late_nonregular_destination_refuses_without_overwriting_earlier_fixture() -> TestResult<()> {
    let root = repository_root()?;
    let output = root.path().join(FIXTURE_DIRECTORY);
    fs::create_dir_all(&output)?;
    let sentinel_path = output.join("omp-real-checkpointed.db");
    let sentinel = b"do not overwrite before destination preflight";
    fs::write(&sentinel_path, sentinel)?;
    fs::create_dir(output.join("omp-schema-wal-only.db-wal"))?;

    let result = run_fixture_omp(root.path())?;
    assert!(!result.status.success());
    assert_eq!(fs::read(sentinel_path)?, sentinel);
    assert!(!output.join("omp-real-checkpointed.db-wal").exists());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("refusing to replace non-regular OMP fixture")
    );
    Ok(())
}

fn repository_root() -> TestResult<TempDir> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("docs/content"))?;
    Ok(root)
}

fn run_fixture_omp(root: &Path) -> TestResult<Output> {
    let output = Command::new(env!("CARGO_BIN_EXE_jackin-xtask"))
        .arg("fixture-omp")
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    Ok(output)
}

fn assert_artifact_inventory(output: &Path) -> TestResult<()> {
    let mut actual = fs::read_dir(output)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    actual.sort();
    let expected = [
        "omp-real-checkpointed.db",
        "omp-real-checkpointed.db-wal",
        "omp-real-current-big-endian.db",
        "omp-real-current-big-endian.db-wal",
        "omp-real-current.db",
        "omp-real-current.db-wal",
        "omp-reused-stale-suffix.db",
        "omp-reused-stale-suffix.db-wal",
        "omp-reused-uncommitted-stale-suffix.db",
        "omp-reused-uncommitted-stale-suffix.db-wal",
        "omp-schema-wal-only.db",
        "omp-schema-wal-only.db-wal",
    ]
    .map(OsString::from)
    .into_iter()
    .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    for name in expected {
        assert_eq!(
            fs::metadata(output.join(&name))?.len() == 0,
            name == "omp-real-checkpointed.db-wal",
            "unexpected fixture size for {name:?}"
        );
    }
    Ok(())
}

fn assert_stale_generation_tail(wal: &[u8]) {
    assert!(wal.len() >= WAL_HEADER_BYTES + WAL_FRAME_HEADER_BYTES);
    let page_size = u32::from_be_bytes([wal[8], wal[9], wal[10], wal[11]]) as usize;
    let page_size = if page_size == 1 { 65_536 } else { page_size };
    let frame_size = page_size + WAL_FRAME_HEADER_BYTES;
    let frame_bytes = &wal[WAL_HEADER_BYTES..];
    assert_eq!(frame_bytes.len() % frame_size, 0);
    let frame_count = frame_bytes.len() / frame_size;
    assert!(frame_count > 1);
    let current_salts = &wal[16..24];
    assert_eq!(
        &wal[WAL_HEADER_BYTES + 8..WAL_HEADER_BYTES + 16],
        current_salts
    );
    let last_frame = WAL_HEADER_BYTES + (frame_count - 1) * frame_size;
    assert_ne!(&wal[last_frame + 8..last_frame + 16], current_salts);
}

fn assert_sqlite_pair(output: &Path, scratch: &Path, stem: &str, expected: &str) -> TestResult<()> {
    let pair_directory = scratch.join(stem);
    fs::create_dir(&pair_directory)?;
    let database = format!("{stem}.db");
    let wal = format!("{stem}.db-wal");
    fs::copy(output.join(&database), pair_directory.join(&database))?;
    fs::copy(output.join(&wal), pair_directory.join(&wal))?;
    let connection = Connection::open(pair_directory.join(database))?;
    let actual = connection.query_row("SELECT value FROM credentials LIMIT 1", [], |row| {
        row.get::<_, String>(0)
    })?;
    assert_eq!(actual, expected);
    Ok(())
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}
