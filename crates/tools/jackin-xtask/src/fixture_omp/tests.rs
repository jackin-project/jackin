// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;

use anyhow::{Result, anyhow};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

use super::{
    ByteOrder, WAL_MAGIC_BIG_CHECKSUMS, WAL_MAGIC_LITTLE_CHECKSUMS, big_endian_checksum_fixture,
    contains, finish_with_cleanup, read_u32_be, sqlite_checksum,
    verify_current_generation_checksums, verify_wal_checksums, wal_current_generation,
};

const CURRENT_DB: &[u8] = include_bytes!("fixtures/omp-real-current.db");
const CURRENT_WAL: &[u8] = include_bytes!("fixtures/omp-real-current.db-wal");
const CHECKPOINTED_DB: &[u8] = include_bytes!("fixtures/omp-real-checkpointed.db");
const CHECKPOINTED_WAL: &[u8] = include_bytes!("fixtures/omp-real-checkpointed.db-wal");
const SCHEMA_DB: &[u8] = include_bytes!("fixtures/omp-schema-wal-only.db");
const SCHEMA_WAL: &[u8] = include_bytes!("fixtures/omp-schema-wal-only.db-wal");
const REUSED_DB: &[u8] = include_bytes!("fixtures/omp-reused-stale-suffix.db");
const REUSED_WAL: &[u8] = include_bytes!("fixtures/omp-reused-stale-suffix.db-wal");
const SPILL_DB: &[u8] = include_bytes!("fixtures/omp-reused-uncommitted-stale-suffix.db");
const SPILL_WAL: &[u8] = include_bytes!("fixtures/omp-reused-uncommitted-stale-suffix.db-wal");
const BIG_ENDIAN_DB: &[u8] = include_bytes!("fixtures/omp-real-current-big-endian.db");
const BIG_ENDIAN_WAL: &[u8] = include_bytes!("fixtures/omp-real-current-big-endian.db-wal");

#[test]
fn sqlite_checksum_uses_wal_magic_selected_byte_order() -> Result<()> {
    let words = [0, 0, 0, 1, 0, 0, 0, 2];
    assert_eq!(sqlite_checksum(&words, (0, 0), ByteOrder::Big)?, (1, 3));
    assert_eq!(
        sqlite_checksum(&words, (0, 0), ByteOrder::Little)?,
        (0x0100_0000, 0x0300_0000)
    );
    let _error = sqlite_checksum(&[0; 7], (0, 0), ByteOrder::Big)
        .err()
        .ok_or_else(|| anyhow!("unaligned SQLite checksum input must fail"))?;
    Ok(())
}

#[test]
fn big_endian_conversion_matches_retained_fixture_and_recomputes_checksums() -> Result<()> {
    assert_eq!(read_u32_be(CURRENT_WAL, 0)?, WAL_MAGIC_LITTLE_CHECKSUMS);
    verify_wal_checksums(CURRENT_WAL)?;

    let converted = big_endian_checksum_fixture(CURRENT_WAL)?;
    assert_eq!(converted, BIG_ENDIAN_WAL);
    assert_eq!(read_u32_be(&converted, 0)?, WAL_MAGIC_BIG_CHECKSUMS);
    verify_wal_checksums(&converted)?;
    assert_eq!(CURRENT_DB, BIG_ENDIAN_DB);
    Ok(())
}

#[test]
fn retained_sqlite_pairs_open_and_return_the_last_committed_values() -> Result<()> {
    for (database, wal, expected) in [
        (CURRENT_DB, CURRENT_WAL, "fixture-wal-current-token"),
        (
            CHECKPOINTED_DB,
            CHECKPOINTED_WAL,
            "fixture-wal-current-token",
        ),
        (SCHEMA_DB, SCHEMA_WAL, "fixture-schema-wal-token"),
        (REUSED_DB, REUSED_WAL, "fixture-reused-current-token"),
        (SPILL_DB, SPILL_WAL, "fixture-main-selected-token"),
        (BIG_ENDIAN_DB, BIG_ENDIAN_WAL, "fixture-wal-current-token"),
    ] {
        assert_sqlite_pair(database, wal, expected)?;
    }
    Ok(())
}

#[test]
fn reused_and_uncommitted_wals_keep_current_generation_only() -> Result<()> {
    let reused = wal_current_generation(REUSED_WAL)?;
    assert!(reused.frames > 0);
    assert!(reused.frames < reused.total_frames);
    assert!(reused.frames > 2);
    verify_current_generation_checksums(REUSED_WAL, reused)?;
    assert!(verify_wal_checksums(REUSED_WAL).is_err());
    let mut corrupted_active_generation = REUSED_WAL.to_vec();
    let page_size = read_u32_be(REUSED_WAL, 8)? as usize;
    let frame_size = page_size + super::WAL_FRAME_HEADER_BYTES;
    corrupted_active_generation
        [super::WAL_HEADER_BYTES + 2 * frame_size + super::WAL_FRAME_HEADER_BYTES] ^= 1;
    assert!(verify_current_generation_checksums(&corrupted_active_generation, reused).is_err());
    assert!(contains(REUSED_WAL, b"fixture-reused-current-token"));

    let spill = wal_current_generation(SPILL_WAL)?;
    assert!(spill.frames > 0);
    assert!(spill.frames < spill.total_frames);
    assert_eq!(spill.commits, 0);
    verify_current_generation_checksums(SPILL_WAL, spill)?;
    assert!(contains(SPILL_DB, b"fixture-main-selected-token"));
    assert!(contains(SPILL_WAL, b"fixture-uncommitted-marker"));
    Ok(())
}

#[test]
fn tracked_sqlite_fixture_hashes_remain_the_authored_baseline() {
    let fixtures: [(&str, &[u8], &str); 12] = [
        (
            "omp-real-checkpointed.db",
            CHECKPOINTED_DB,
            "9036862e170211fe3ce43cf103d976dd7017274791525b80fcbb4f9a698fbb83",
        ),
        (
            "omp-real-checkpointed.db-wal",
            CHECKPOINTED_WAL,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "omp-real-current-big-endian.db",
            BIG_ENDIAN_DB,
            "bf03150d91672968600dac5b7388619c25e0cbeb364f261e2753f67383c2b042",
        ),
        (
            "omp-real-current-big-endian.db-wal",
            BIG_ENDIAN_WAL,
            "9cd5743a6629100f42a9527ea636b89b814876a22d80bd0aac5082f0b1d2b573",
        ),
        (
            "omp-real-current.db",
            CURRENT_DB,
            "bf03150d91672968600dac5b7388619c25e0cbeb364f261e2753f67383c2b042",
        ),
        (
            "omp-real-current.db-wal",
            CURRENT_WAL,
            "e7926f8dbd83b24a28030e5d2f3d1cd40928e112b1144b9eb176614949ea8285",
        ),
        (
            "omp-reused-stale-suffix.db",
            REUSED_DB,
            "29ae781a20cbbc6dbe6a665813e8afd6e191f3786c513a20f3201ec49a311242",
        ),
        (
            "omp-reused-stale-suffix.db-wal",
            REUSED_WAL,
            "97bc0ea57a03053b50c34e38da5883c7431599010ab23185017eaf4914cd646e",
        ),
        (
            "omp-reused-uncommitted-stale-suffix.db",
            SPILL_DB,
            "82f8d33245f20d56ef6172213e7d8a49f0df5f8baef55a28732415363d40b2d4",
        ),
        (
            "omp-reused-uncommitted-stale-suffix.db-wal",
            SPILL_WAL,
            "7738f2230e9b90296122e6194a70eb798b889b0810a3ab0b7f8f09d02f1d7859",
        ),
        (
            "omp-schema-wal-only.db",
            SCHEMA_DB,
            "56642fddc9fdc72c80af2e05fedd507c43e64bf57ab72fb8467075aa4b32deb4",
        ),
        (
            "omp-schema-wal-only.db-wal",
            SCHEMA_WAL,
            "3c1dc52d6f6227688ab6cae3c84084f574b215fed8c8a8bbb28abe173179254a",
        ),
    ];
    for (name, bytes, expected) in fixtures {
        assert_eq!(
            hex::encode(Sha256::digest(bytes)),
            expected,
            "retained fixture bytes changed: {}",
            Path::new(name).display()
        );
    }
}

#[test]
fn cleanup_result_combinations_preserve_body_and_cleanup_errors() -> Result<()> {
    assert_eq!(finish_with_cleanup(Ok(7), Ok(()), "cleanup context")?, 7);

    let body_error = finish_with_cleanup::<()>(
        Err(anyhow::anyhow!("body failure")),
        Ok(()),
        "cleanup context",
    )
    .err()
    .ok_or_else(|| anyhow!("body failure must remain visible"))?;
    assert_eq!(format!("{body_error:#}"), "body failure");

    let cleanup_error = finish_with_cleanup::<()>(
        Ok(()),
        Err(std::io::Error::other("cleanup failure")),
        "cleanup context",
    )
    .err()
    .ok_or_else(|| anyhow!("cleanup failure must remain visible"))?;
    assert!(format!("{cleanup_error:#}").contains("cleanup failure"));
    assert!(format!("{cleanup_error:#}").contains("cleanup context"));

    let body_source = std::io::Error::other("body root cause");
    let body_error = anyhow::Error::new(body_source).context("body context");
    let both_errors = finish_with_cleanup::<()>(
        Err(body_error),
        Err(std::io::Error::other("cleanup failure")),
        "cleanup context",
    )
    .err()
    .ok_or_else(|| anyhow!("both failures must remain visible"))?;
    let diagnostics = format!("{both_errors:#}");
    assert!(diagnostics.contains("body context"));
    assert!(diagnostics.contains("cleanup context"));
    assert!(diagnostics.contains("cleanup failure"));
    assert_eq!(both_errors.root_cause().to_string(), "body root cause");
    Ok(())
}

fn assert_sqlite_pair(database: &[u8], wal: &[u8], expected: &str) -> Result<()> {
    let directory = tempdir()?;
    let database_path = directory.path().join("retained.db");
    let wal_path = directory.path().join("retained.db-wal");
    fs::write(&database_path, database)?;
    fs::write(&wal_path, wal)?;
    let connection = Connection::open(database_path)?;
    let actual = connection.query_row("SELECT value FROM credentials LIMIT 1", [], |row| {
        row.get::<_, String>(0)
    })?;
    assert_eq!(actual, expected);
    Ok(())
}
