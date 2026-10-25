use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::{
    Artifacts, ByteOrder, FIXTURE_DIRECTORY, WAL_MAGIC_BIG_CHECKSUMS, WAL_MAGIC_LITTLE_CHECKSUMS,
    big_endian_checksum_fixture, contains, finish_with_cleanup, generate, read_u32_be,
    sqlite_checksum, validate_artifacts, verify_current_generation_checksums, verify_pair,
    verify_wal_checksums, wal_current_generation,
};

const CURRENT_DB: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-real-current.db"
);
const CURRENT_WAL: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-real-current.db-wal"
);
const CHECKPOINTED_DB: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-real-checkpointed.db"
);
const CHECKPOINTED_WAL: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-real-checkpointed.db-wal"
);
const SCHEMA_DB: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-schema-wal-only.db"
);
const SCHEMA_WAL: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-schema-wal-only.db-wal"
);
const REUSED_DB: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-reused-stale-suffix.db"
);
const REUSED_WAL: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-reused-stale-suffix.db-wal"
);
const SPILL_DB: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-reused-uncommitted-stale-suffix.db"
);
const SPILL_WAL: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-reused-uncommitted-stale-suffix.db-wal"
);
const BIG_ENDIAN_DB: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-real-current-big-endian.db"
);
const BIG_ENDIAN_WAL: &[u8] = include_bytes!(
    "../../../../services/jackin-instance-roles/src/tests/fixtures/omp-real-current-big-endian.db-wal"
);

#[test]
fn sqlite_checksum_uses_wal_magic_selected_byte_order() {
    let words = [0, 0, 0, 1, 0, 0, 0, 2];
    assert_eq!(
        sqlite_checksum(&words, (0, 0), ByteOrder::Big).expect("big checksum"),
        (1, 3)
    );
    assert_eq!(
        sqlite_checksum(&words, (0, 0), ByteOrder::Little).expect("little checksum"),
        (0x0100_0000, 0x0300_0000)
    );
    sqlite_checksum(&[0; 7], (0, 0), ByteOrder::Big)
        .expect_err("unaligned checksum input must be rejected");
}

#[test]
fn big_endian_conversion_matches_retained_fixture_and_recomputes_checksums() {
    assert_eq!(
        read_u32_be(CURRENT_WAL, 0).expect("little-endian WAL marker"),
        WAL_MAGIC_LITTLE_CHECKSUMS
    );
    verify_wal_checksums(CURRENT_WAL).expect("retained little-endian WAL checksums");

    let converted = big_endian_checksum_fixture(CURRENT_WAL).expect("big-endian WAL conversion");
    assert_eq!(converted, BIG_ENDIAN_WAL);
    assert_eq!(
        read_u32_be(&converted, 0).expect("big-endian WAL marker"),
        WAL_MAGIC_BIG_CHECKSUMS
    );
    verify_wal_checksums(&converted).expect("converted big-endian WAL checksums");
    assert_eq!(CURRENT_DB, BIG_ENDIAN_DB);
}

#[test]
fn retained_sqlite_pairs_open_and_return_the_last_committed_values() {
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
        verify_pair(database, wal, expected).expect("retained SQLite fixture remains readable");
    }
}

#[test]
fn reused_and_uncommitted_wals_keep_current_generation_only() {
    let reused = wal_current_generation(REUSED_WAL).expect("reused WAL generations");
    assert!(reused.frames > 0);
    assert!(reused.frames < reused.total_frames);
    assert!(reused.frames > 2);
    verify_current_generation_checksums(REUSED_WAL, reused)
        .expect("reused WAL current-generation checksums");
    verify_wal_checksums(REUSED_WAL)
        .expect_err("the complete reused WAL includes an invalid stale suffix");
    let mut corrupted_active_generation = REUSED_WAL.to_vec();
    let page_size = read_u32_be(REUSED_WAL, 8).expect("WAL page size") as usize;
    let frame_size = page_size + super::WAL_FRAME_HEADER_BYTES;
    corrupted_active_generation
        [super::WAL_HEADER_BYTES + 2 * frame_size + super::WAL_FRAME_HEADER_BYTES] ^= 1;
    verify_current_generation_checksums(&corrupted_active_generation, reused)
        .expect_err("a damaged active-generation checksum must be rejected");
    assert!(contains(REUSED_WAL, b"fixture-reused-current-token"));

    let spill = wal_current_generation(SPILL_WAL).expect("spill WAL generations");
    assert!(spill.frames > 0);
    assert!(spill.frames < spill.total_frames);
    assert_eq!(spill.commits, 0);
    verify_current_generation_checksums(SPILL_WAL, spill)
        .expect("spill WAL current-generation checksums");
    assert!(contains(SPILL_DB, b"fixture-main-selected-token"));
    assert!(contains(SPILL_WAL, b"fixture-uncommitted-marker"));
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
fn generated_artifacts_preserve_all_six_semantic_families() {
    let temporary = tempfile::tempdir().expect("temporary OMP generation directory");
    let artifacts = generate(temporary.path()).expect("generate SQLite/WAL artifact set");
    validate_artifacts(&artifacts).expect("validate exact OMP artifact inventory");

    let actual_names = artifacts.keys().map(String::as_str).collect::<Vec<_>>();
    let expected_names = [
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
    ];
    assert_eq!(actual_names.as_slice(), expected_names.as_slice());
    for (name, bytes) in &artifacts {
        assert_eq!(
            bytes.is_empty(),
            name == "omp-real-checkpointed.db-wal",
            "unexpected empty or non-empty generated member: {name}"
        );
    }

    let current_db = artifact(&artifacts, "omp-real-current.db");
    let current_wal = artifact(&artifacts, "omp-real-current.db-wal");
    assert!(!contains(current_db, b"fixture-wal-current-token"));
    assert!(contains(current_wal, b"fixture-wal-current-token"));
    verify_pair(current_db, current_wal, "fixture-wal-current-token")
        .expect("current committed token is supplied by WAL");

    let checkpointed_db = artifact(&artifacts, "omp-real-checkpointed.db");
    let checkpointed_wal = artifact(&artifacts, "omp-real-checkpointed.db-wal");
    assert!(contains(checkpointed_db, b"fixture-wal-current-token"));
    assert!(checkpointed_wal.is_empty());
    verify_pair(
        checkpointed_db,
        checkpointed_wal,
        "fixture-wal-current-token",
    )
    .expect("checkpointed database contains the committed token");

    let schema_db = artifact(&artifacts, "omp-schema-wal-only.db");
    let schema_wal = artifact(&artifacts, "omp-schema-wal-only.db-wal");
    assert!(!contains(schema_db, b"credentials"));
    assert!(!contains(schema_db, b"fixture-schema-wal-token"));
    assert!(contains(schema_wal, b"credentials"));
    assert!(contains(schema_wal, b"fixture-schema-wal-token"));
    verify_pair(schema_db, schema_wal, "fixture-schema-wal-token")
        .expect("schema and credential are readable from WAL");

    let reused_db = artifact(&artifacts, "omp-reused-stale-suffix.db");
    let reused_wal = artifact(&artifacts, "omp-reused-stale-suffix.db-wal");
    let reused_generation =
        wal_current_generation(reused_wal).expect("reused WAL current generation");
    assert!(reused_generation.frames > 0);
    assert!(reused_generation.frames < reused_generation.total_frames);
    verify_current_generation_checksums(reused_wal, reused_generation)
        .expect("reused current generation checksums");
    verify_wal_checksums(reused_wal)
        .expect_err("the complete reused WAL includes an invalid stale suffix");
    verify_pair(reused_db, reused_wal, "fixture-reused-current-token")
        .expect("old-generation suffix does not replace current value");

    let spill_db = artifact(&artifacts, "omp-reused-uncommitted-stale-suffix.db");
    let spill_wal = artifact(&artifacts, "omp-reused-uncommitted-stale-suffix.db-wal");
    let spill_generation = wal_current_generation(spill_wal).expect("spill current generation");
    assert!(spill_generation.frames > 0);
    assert!(spill_generation.frames < spill_generation.total_frames);
    assert_eq!(spill_generation.commits, 0);
    verify_current_generation_checksums(spill_wal, spill_generation)
        .expect("uncommitted spill current-generation checksums");
    assert!(contains(spill_db, b"fixture-main-selected-token"));
    assert!(contains(spill_wal, b"fixture-uncommitted-marker"));
    verify_pair(spill_db, spill_wal, "fixture-main-selected-token")
        .expect("uncommitted spill does not replace committed value");

    let big_endian_db = artifact(&artifacts, "omp-real-current-big-endian.db");
    let big_endian_wal = artifact(&artifacts, "omp-real-current-big-endian.db-wal");
    let expected_big_endian_wal =
        big_endian_checksum_fixture(current_wal).expect("convert generated current WAL");
    assert_eq!(big_endian_db, current_db);
    assert_eq!(big_endian_wal, expected_big_endian_wal.as_slice());
    assert_eq!(
        read_u32_be(big_endian_wal, 0).expect("big-endian checksum marker"),
        WAL_MAGIC_BIG_CHECKSUMS
    );
    verify_wal_checksums(big_endian_wal).expect("big-endian WAL checksums");
    verify_pair(big_endian_db, big_endian_wal, "fixture-wal-current-token")
        .expect("big-endian pair returns the current committed token");
}

#[test]
fn late_nonregular_destination_refuses_without_overwriting_earlier_fixture() {
    let temporary = tempfile::tempdir().expect("temporary fixture root");
    let output = temporary.path().join(FIXTURE_DIRECTORY);
    fs::create_dir_all(&output).expect("fixture output directory");

    let sentinel_path = output.join("omp-real-checkpointed.db");
    let sentinel = b"do not overwrite before destination preflight";
    fs::write(&sentinel_path, sentinel).expect("write sentinel fixture");

    let late_blocker = output.join("omp-schema-wal-only.db-wal");
    fs::create_dir(&late_blocker).expect("create late non-regular destination");

    let error = super::run(temporary.path()).expect_err("non-regular destination must fail");
    assert!(format!("{error:#}").contains("refusing to replace non-regular OMP fixture"));
    assert_eq!(fs::read(&sentinel_path).expect("read sentinel"), sentinel);
    assert!(!output.join("omp-real-checkpointed.db-wal").exists());
}

#[test]
fn cleanup_result_combinations_preserve_body_and_cleanup_errors() {
    assert_eq!(
        finish_with_cleanup(Ok(7), Ok(()), "cleanup context").expect("both operations succeed"),
        7
    );

    let body_error = finish_with_cleanup::<()>(
        Err(anyhow::anyhow!("body failure")),
        Ok(()),
        "cleanup context",
    )
    .expect_err("body failure must remain visible");
    assert_eq!(format!("{body_error:#}"), "body failure");

    let cleanup_error = finish_with_cleanup::<()>(
        Ok(()),
        Err(std::io::Error::other("cleanup failure")),
        "cleanup context",
    )
    .expect_err("cleanup failure must remain visible");
    assert!(format!("{cleanup_error:#}").contains("cleanup failure"));
    assert!(format!("{cleanup_error:#}").contains("cleanup context"));

    let body_source = std::io::Error::other("body root cause");
    let body_error = anyhow::Error::new(body_source).context("body context");
    let both_errors = finish_with_cleanup::<()>(
        Err(body_error),
        Err(std::io::Error::other("cleanup failure")),
        "cleanup context",
    )
    .expect_err("both failures must remain visible");
    let diagnostics = format!("{both_errors:#}");
    assert!(diagnostics.contains("body context"));
    assert!(diagnostics.contains("cleanup context"));
    assert!(diagnostics.contains("cleanup failure"));
    assert_eq!(both_errors.root_cause().to_string(), "body root cause");
}

fn artifact<'a>(artifacts: &'a Artifacts, name: &str) -> &'a [u8] {
    artifacts.get(name).map_or_else(
        || panic!("generated artifact is missing: {name}"),
        Vec::as_slice,
    )
}
