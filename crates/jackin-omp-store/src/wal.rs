// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Strict validation for a captured SQLite WAL before SQLite opens the copy.
//!
//! A WAL can be reused without truncating its previous-generation tail. That
//! tail is ambiguous with corruption of a current frame, so this validator
//! accepts only a complete, single-generation checksum chain through EOF.
//! It never falls back to an earlier commit when a later frame is invalid.

use std::time::Instant;

use thiserror::Error;

pub(crate) const MAX_DATABASE_BYTES: usize = crate::MAX_STANDALONE_DATABASE_BYTES;
pub(crate) const MAX_WAL_BYTES: usize = 8 * 1024 * 1024;
const SQLITE_DATABASE_HEADER_BYTES: usize = 100;
const WAL_HEADER_BYTES: usize = 32;
const WAL_FRAME_HEADER_BYTES: usize = 24;
const WAL_FORMAT_VERSION: u32 = 3_007_000;
const CHECKSUM_DEADLINE_INTERVAL: usize = 4096;

/// Secret-free reason that a captured SQLite image could not be validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum WalError {
    /// The main file is not a bounded, well-formed SQLite database image.
    #[error("OMP database image is invalid or exceeds its size limit")]
    InvalidDatabase,
    /// A present WAL has no complete, valid header or does not match the DB.
    #[error("OMP WAL header is invalid")]
    InvalidHeader,
    /// A present WAL is too large.
    #[error("OMP WAL exceeds its size limit")]
    WalTooLarge,
    /// A present WAL does not end at a complete frame boundary.
    #[error("OMP WAL has a truncated frame or trailing bytes")]
    InvalidLength,
    /// A frame salt does not belong to this WAL generation.
    #[error("OMP WAL contains a frame from another generation")]
    InvalidSalt,
    /// A header or frame checksum is invalid.
    #[error("OMP WAL checksum validation failed")]
    InvalidChecksum,
    /// A page reference or committed logical size exceeds the configured cap.
    #[error("OMP database exceeds its logical page limit")]
    PageLimit,
    /// Validation exceeded the capture operation's deadline.
    #[error("OMP database validation timed out")]
    Deadline,
}

/// Validated geometry for the main SQLite image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DatabaseHeader {
    pub(crate) page_size: usize,
    pub(crate) page_count: usize,
    /// Whether the file header declares WAL journal mode.
    pub(crate) wal_mode: bool,
}

/// The final committed database boundary represented by a valid WAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WalCommit {
    pub(crate) final_frame: usize,
    pub(crate) page_count: usize,
}

/// Summary of a complete, valid WAL. A WAL may contain valid uncommitted frames
/// after its final commit; SQLite ignores those frames when materializing data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ValidatedWal {
    pub(crate) frame_count: usize,
    pub(crate) last_commit: Option<WalCommit>,
}

/// Validate the main database header and enforce both physical and logical
/// size bounds before it is opened by SQLite.
pub(crate) fn validate_database(
    database: &[u8],
    deadline: Instant,
) -> Result<DatabaseHeader, WalError> {
    check_deadline(deadline)?;
    if database.len() > MAX_DATABASE_BYTES || database.len() < SQLITE_DATABASE_HEADER_BYTES {
        return Err(WalError::InvalidDatabase);
    }
    if database.get(..16) != Some(b"SQLite format 3\0".as_slice()) {
        return Err(WalError::InvalidDatabase);
    }

    let encoded_page_size = read_u16_be(database, 16).ok_or(WalError::InvalidDatabase)?;
    let page_size = if encoded_page_size == 1 {
        65_536
    } else {
        usize::from(encoded_page_size)
    };
    if !(512..=65_536).contains(&page_size) || !page_size.is_power_of_two() {
        return Err(WalError::InvalidDatabase);
    }
    if !database.len().is_multiple_of(page_size) {
        return Err(WalError::InvalidDatabase);
    }
    let wal_mode = match (database[18], database[19]) {
        (1, 1) => false,
        (2, 2) => true,
        _ => return Err(WalError::InvalidDatabase),
    };

    let page_count = database.len() / page_size;
    if page_count == 0 || page_count > MAX_DATABASE_BYTES / page_size {
        return Err(WalError::PageLimit);
    }

    Ok(DatabaseHeader {
        page_size,
        page_count,
        wal_mode,
    })
}

/// Validate every byte of a present WAL. `None` means no WAL file existed;
/// `Some(&[])` is an invalid truncated WAL and is deliberately not treated as
/// absence.
pub(crate) fn validate_wal(
    wal: &[u8],
    database: DatabaseHeader,
    deadline: Instant,
) -> Result<ValidatedWal, WalError> {
    validate_wal_with_page_limit(
        wal,
        database,
        MAX_DATABASE_BYTES / database.page_size,
        deadline,
    )
}

fn validate_wal_with_page_limit(
    wal: &[u8],
    database: DatabaseHeader,
    max_pages: usize,
    deadline: Instant,
) -> Result<ValidatedWal, WalError> {
    check_deadline(deadline)?;
    if !database.wal_mode {
        return Err(WalError::InvalidHeader);
    }
    if wal.len() > MAX_WAL_BYTES {
        return Err(WalError::WalTooLarge);
    }
    if wal.len() < WAL_HEADER_BYTES {
        return Err(WalError::InvalidLength);
    }

    let magic = read_u32_be(wal, 0).ok_or(WalError::InvalidHeader)?;
    let checksum_little_endian = match magic {
        0x377F_0682 => true,
        0x377F_0683 => false,
        _ => return Err(WalError::InvalidHeader),
    };
    if read_u32_be(wal, 4) != Some(WAL_FORMAT_VERSION)
        || usize::try_from(read_u32_be(wal, 8).ok_or(WalError::InvalidHeader)?).ok()
            != Some(database.page_size)
    {
        return Err(WalError::InvalidHeader);
    }

    let header_checksum = checksum(
        wal.get(..24).ok_or(WalError::InvalidHeader)?,
        (0, 0),
        checksum_little_endian,
        deadline,
    )?;
    if header_checksum
        != (
            read_u32_be(wal, 24).ok_or(WalError::InvalidHeader)?,
            read_u32_be(wal, 28).ok_or(WalError::InvalidHeader)?,
        )
    {
        return Err(WalError::InvalidChecksum);
    }

    let frame_size = database
        .page_size
        .checked_add(WAL_FRAME_HEADER_BYTES)
        .ok_or(WalError::InvalidLength)?;
    let body_len = wal
        .len()
        .checked_sub(WAL_HEADER_BYTES)
        .ok_or(WalError::InvalidLength)?;
    if !body_len.is_multiple_of(frame_size) {
        return Err(WalError::InvalidLength);
    }
    let frame_count = body_len / frame_size;
    let max_reachable_page = database
        .page_count
        .checked_add(frame_count)
        .ok_or(WalError::PageLimit)?
        .min(max_pages);
    let salt_one = read_u32_be(wal, 16).ok_or(WalError::InvalidHeader)?;
    let salt_two = read_u32_be(wal, 20).ok_or(WalError::InvalidHeader)?;
    let mut previous_checksum = header_checksum;
    let mut last_commit = None;

    for index in 0..frame_count {
        check_deadline(deadline)?;
        let frame_start = WAL_HEADER_BYTES
            .checked_add(index.checked_mul(frame_size).ok_or(WalError::InvalidLength)?)
            .ok_or(WalError::InvalidLength)?;
        let header_end = frame_start
            .checked_add(8)
            .ok_or(WalError::InvalidLength)?;
        let page_start = frame_start
            .checked_add(WAL_FRAME_HEADER_BYTES)
            .ok_or(WalError::InvalidLength)?;
        let page_end = page_start
            .checked_add(database.page_size)
            .ok_or(WalError::InvalidLength)?;
        let frame_header = wal
            .get(frame_start..header_end)
            .ok_or(WalError::InvalidLength)?;
        let frame_page = wal.get(page_start..page_end).ok_or(WalError::InvalidLength)?;

        if read_u32_be(wal, frame_start + 8) != Some(salt_one)
            || read_u32_be(wal, frame_start + 12) != Some(salt_two)
        {
            return Err(WalError::InvalidSalt);
        }

        let checksum_after_header = checksum(
            frame_header,
            previous_checksum,
            checksum_little_endian,
            deadline,
        )?;
        let computed_checksum = checksum(
            frame_page,
            checksum_after_header,
            checksum_little_endian,
            deadline,
        )?;
        let stored_checksum = (
            read_u32_be(wal, frame_start + 16).ok_or(WalError::InvalidLength)?,
            read_u32_be(wal, frame_start + 20).ok_or(WalError::InvalidLength)?,
        );
        if computed_checksum != stored_checksum {
            return Err(WalError::InvalidChecksum);
        }

        let page_number = read_u32_be(wal, frame_start).ok_or(WalError::InvalidLength)?;
        if page_number == 0
            || usize::try_from(page_number).map_or(true, |page| page > max_reachable_page)
        {
            return Err(WalError::PageLimit);
        }

        let committed_pages = read_u32_be(wal, frame_start + 4).ok_or(WalError::InvalidLength)?;
        if committed_pages != 0 {
            let page_count =
                usize::try_from(committed_pages).map_err(|_| WalError::PageLimit)?;
            if page_count > max_pages {
                return Err(WalError::PageLimit);
            }
            last_commit = Some(WalCommit {
                final_frame: index,
                page_count,
            });
        }
        previous_checksum = computed_checksum;
    }

    check_deadline(deadline)?;
    Ok(ValidatedWal {
        frame_count,
        last_commit,
    })
}

fn checksum(
    bytes: &[u8],
    mut sum: (u32, u32),
    little_endian: bool,
    deadline: Instant,
) -> Result<(u32, u32), WalError> {
    if !bytes.len().is_multiple_of(8) {
        return Err(WalError::InvalidLength);
    }
    for (index, words) in bytes.chunks_exact(8).enumerate() {
        if index.is_multiple_of(CHECKSUM_DEADLINE_INTERVAL / 8) {
            check_deadline(deadline)?;
        }
        let first: [u8; 4] = words[..4]
            .try_into()
            .map_err(|_| WalError::InvalidLength)?;
        let second: [u8; 4] = words[4..]
            .try_into()
            .map_err(|_| WalError::InvalidLength)?;
        let (first, second) = if little_endian {
            (u32::from_le_bytes(first), u32::from_le_bytes(second))
        } else {
            (u32::from_be_bytes(first), u32::from_be_bytes(second))
        };
        sum.0 = sum.0.wrapping_add(first).wrapping_add(sum.1);
        sum.1 = sum.1.wrapping_add(second).wrapping_add(sum.0);
    }
    check_deadline(deadline)?;
    Ok(sum)
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_be_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_be_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn check_deadline(deadline: Instant) -> Result<(), WalError> {
    if Instant::now() >= deadline {
        Err(WalError::Deadline)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        DatabaseHeader, WalError, validate_database, validate_wal,
        validate_wal_with_page_limit,
    };

    const CURRENT_DB: &[u8] =
        include_bytes!("../../jackin-instance/src/auth/tests/fixtures/omp-real-current.db");
    const CURRENT_WAL: &[u8] =
        include_bytes!("../../jackin-instance/src/auth/tests/fixtures/omp-real-current.db-wal");
    const BIG_ENDIAN_DB: &[u8] = include_bytes!(
        "../../jackin-instance/src/auth/tests/fixtures/omp-real-current-big-endian.db"
    );
    const BIG_ENDIAN_WAL: &[u8] = include_bytes!(
        "../../jackin-instance/src/auth/tests/fixtures/omp-real-current-big-endian.db-wal"
    );
    const REUSED_STALE_DB: &[u8] =
        include_bytes!("../../jackin-instance/src/auth/tests/fixtures/omp-reused-stale-suffix.db");
    const REUSED_STALE_WAL: &[u8] = include_bytes!(
        "../../jackin-instance/src/auth/tests/fixtures/omp-reused-stale-suffix.db-wal"
    );
    const REUSED_UNCOMMITTED_STALE_DB: &[u8] = include_bytes!(
        "../../jackin-instance/src/auth/tests/fixtures/omp-reused-uncommitted-stale-suffix.db"
    );
    const REUSED_UNCOMMITTED_STALE_WAL: &[u8] = include_bytes!(
        "../../jackin-instance/src/auth/tests/fixtures/omp-reused-uncommitted-stale-suffix.db-wal"
    );

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(2)
    }

    fn header(database: &[u8]) -> DatabaseHeader {
        validate_database(database, deadline()).expect("checked-in fixture database is valid")
    }

    fn first_stale_frame(wal: &[u8], page_size: usize) -> usize {
        let frame_size = page_size + 24;
        let salts = (&wal[16..20], &wal[20..24]);
        let frames = wal[32..]
            .chunks_exact(frame_size)
            .enumerate()
            .find(|(_, frame)| frame[8..12] != salts.0 || frame[12..16] != salts.1);
        frames
            .map(|(index, _)| 32 + index * frame_size)
            .expect("stale fixture has a generation transition")
    }

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
        let stop = first_stale_frame(REUSED_UNCOMMITTED_STALE_WAL, header(CURRENT_DB).page_size);
        let valid_prefix = &REUSED_UNCOMMITTED_STALE_WAL[..stop];
        let full = validate_wal(valid_prefix, header(CURRENT_DB), deadline())
            .expect("valid commit followed by valid spill frames stays readable");

        let commit = full.last_commit.expect("fixture prefix contains a commit");
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
        assert!(commit.page_count > database.page_count);

        assert_eq!(
            validate_wal_with_page_limit(
                CURRENT_WAL,
                database,
                commit.page_count - 1,
                deadline()
            ),
            Err(WalError::PageLimit)
        );
    }

    #[test]
    fn rejects_expired_deadline_without_scanning_or_exposing_data() {
        let expired = Instant::now() - Duration::from_millis(1);
        assert_eq!(
            validate_database(CURRENT_DB, expired),
            Err(WalError::Deadline)
        );
    }
}
