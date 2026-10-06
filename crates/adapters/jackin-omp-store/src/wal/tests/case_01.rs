// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn validates_complete_little_and_big_endian_wals() {
    let little = validate_wal(CURRENT_WAL, header(CURRENT_DB), deadline())
        .expect("little-endian checksum WAL is valid");
    let big = validate_wal(BIG_ENDIAN_WAL, header(BIG_ENDIAN_DB), deadline())
        .expect("big-endian checksum WAL is valid");

    assert!(little.frame_count > 0);
    assert_eq!(little.last_commit.map(|commit| commit.page_count), Some(2));
    assert!(big.frame_count > 0);
    assert_eq!(big.last_commit.map(|commit| commit.page_count), Some(2));
}

#[test]
fn rejects_a_stale_generation_suffix_instead_of_falling_back() {
    for (database, wal) in [
        (REUSED_STALE_DB, REUSED_STALE_WAL),
        (REUSED_UNCOMMITTED_STALE_DB, REUSED_UNCOMMITTED_STALE_WAL),
    ] {
        assert_eq!(
            validate_wal(wal, header(database), deadline()),
            Err(WalError::InvalidSalt)
        );
    }
}

#[test]
fn permits_only_complete_same_generation_uncommitted_frames_after_a_commit() {
    // CURRENT fixture ends with a commit. Append one synthetic
    // same-generation uncommitted frame with a valid chained checksum;
    // the validator must accept it without moving the commit.
    let database = header(CURRENT_DB);
    let mut wal = CURRENT_WAL.to_vec();
    let frame_size = database.page_size + 24;
    let last = wal.len() - frame_size;
    let previous = (
        u32::from_be_bytes(wal[last + 16..last + 20].try_into().unwrap()),
        u32::from_be_bytes(wal[last + 20..last + 24].try_into().unwrap()),
    );
    let little_endian = u32::from_be_bytes(wal[..4].try_into().unwrap()) == 0x377F_0682;
    let mut frame = vec![0_u8; frame_size];
    frame[..4].copy_from_slice(&1_u32.to_be_bytes());
    frame[8..12].copy_from_slice(&wal[16..20]);
    frame[12..16].copy_from_slice(&wal[20..24]);
    let after_header = checksum(&frame[..8], previous, little_endian, deadline()).unwrap();
    let sum = checksum(&frame[24..], after_header, little_endian, deadline()).unwrap();
    frame[16..20].copy_from_slice(&sum.0.to_be_bytes());
    frame[20..24].copy_from_slice(&sum.1.to_be_bytes());
    wal.extend_from_slice(&frame);

    let full = validate_wal(&wal, database, deadline()).expect("spill frame stays readable");
    let commit = full.last_commit.expect("wal contains a commit");
    assert_eq!(commit.final_frame, 1);
    assert!(full.frame_count > commit.final_frame + 1);
}

#[test]
fn rejects_joint_salt_and_checksum_corruption_after_a_valid_commit() {
    let mut wal = CURRENT_WAL.to_vec();
    let valid = validate_wal(&wal, header(CURRENT_DB), deadline()).unwrap();
    let commit = valid.last_commit.expect("fixture has a commit");
    let frame_size = header(CURRENT_DB).page_size + 24;
    let frame = 32 + commit.final_frame * frame_size;
    wal[frame + 8] ^= 0x01;
    wal[frame + 16] ^= 0x80;

    assert!(matches!(
        validate_wal(&wal, header(CURRENT_DB), deadline()),
        Err(WalError::InvalidSalt | WalError::InvalidChecksum)
    ));
}

#[test]
fn rejects_partial_frames_and_present_empty_wal() {
    let mut partial = CURRENT_WAL.to_vec();
    partial.push(0);
    assert_eq!(
        validate_wal(&partial, header(CURRENT_DB), deadline()),
        Err(WalError::InvalidLength)
    );
    assert_eq!(
        validate_wal(&[], header(CURRENT_DB), deadline()),
        Err(WalError::InvalidLength)
    );
}

#[test]
fn rejects_a_wal_paired_with_a_rollback_mode_database() {
    let mut rollback_database = CURRENT_DB.to_vec();
    rollback_database[18] = 1;
    rollback_database[19] = 1;
    let database = header(&rollback_database);
    assert_eq!(
        validate_wal(CURRENT_WAL, database, deadline()),
        Err(WalError::InvalidHeader)
    );
}

#[test]
fn rejects_logical_page_count_overflow_before_sqlite_open() {
    let database = header(CURRENT_DB);
    let valid = validate_wal(CURRENT_WAL, database, deadline()).unwrap();
    let commit = valid.last_commit.unwrap();
    assert!(commit.page_count >= database.page_count);

    assert_eq!(
        validate_wal_with_page_limit(CURRENT_WAL, database, commit.page_count - 1, deadline()),
        Err(WalError::PageLimit)
    );
}

#[test]
fn rejects_expired_deadline_without_scanning_or_exposing_data() {
    let expired = Instant::now()
        .checked_sub(Duration::from_millis(1))
        .unwrap();
    assert_eq!(
        validate_database(CURRENT_DB, expired),
        Err(WalError::Deadline)
    );
}
