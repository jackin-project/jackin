// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use zeroize::Zeroizing;

use crate::boundary::{FileIdentity, FileSystem, FsError, RegularFileProblem};
use crate::ids::Sha256;
use crate::paths::{HostPath, RepoPath};

pub const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

const ATTEMPTS: usize = 3;
const WAL_HEADER: usize = 32;
const WAL_FRAME_HEADER: usize = 24;
const WAL_MAGIC: u32 = 0x377f_0682;
const WAL_MAGIC_SWAPPED: u32 = 0x377f_0683;
const WAL_FORMAT: u32 = 3_007_000;
const MIN_PAGE: usize = 512;
const MAX_PAGE: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureLimits {
    pub database_bytes: usize,
    pub log_bytes: usize,
    pub image_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaptureProblem {
    Missing,
    NotARegularFile,
    NotSqlite,
    DatabaseTooLarge,
    LogTooLarge,
    ImageTooLarge,
    LogUnreadable,
    Unreadable,
    Changed,
}

pub struct StoreImage(Zeroizing<Vec<u8>>);

impl std::fmt::Debug for StoreImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StoreImage({} bytes)", self.0.len())
    }
}

impl StoreImage {
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug)]
pub struct Capture {
    pub image: StoreImage,
    pub identity: FileIdentity,
}

enum Read {
    Bytes(Vec<u8>),
    Absent,
}

async fn read_file(
    fs: &dyn FileSystem,
    dir: &HostPath,
    name: &str,
    limit: usize,
    too_large: CaptureProblem,
    unusable: CaptureProblem,
) -> Result<Read, CaptureProblem> {
    let file = RepoPath::parse(name).map_err(|_| CaptureProblem::Unreadable)?;
    match fs.read_regular(dir, &file, limit).await {
        Ok(Ok(found)) if found.bytes.len() > limit => Err(too_large),
        Ok(Ok(found)) => Ok(Read::Bytes(found.bytes)),
        Ok(Err(RegularFileProblem::Missing))
        | Err(FsError::Io {
            kind: std::io::ErrorKind::NotFound,
            ..
        }) => Ok(Read::Absent),
        Ok(Err(RegularFileProblem::Symlink | RegularFileProblem::NotARegularFile)) => Err(unusable),
        Err(_) => Err(unusable),
    }
}

pub async fn capture(
    fs: &dyn FileSystem,
    database: &HostPath,
    limits: CaptureLimits,
) -> Result<Capture, CaptureProblem> {
    let target = fs
        .canonicalize(database)
        .await
        .map_err(|_| CaptureProblem::Unreadable)?
        .ok_or(CaptureProblem::Missing)?;
    let (Some(dir), Some(name)) = (
        target.parent(),
        target.file_name().and_then(|name| name.to_str()),
    ) else {
        return Err(CaptureProblem::NotARegularFile);
    };
    let log_name = format!("{name}-wal");
    let probe = RepoPath::parse(name).map_err(|_| CaptureProblem::Unreadable)?;
    match fs.read_regular(&dir, &probe, SQLITE_MAGIC.len()).await {
        Ok(Ok(head)) if head.bytes.starts_with(SQLITE_MAGIC) => {}
        Ok(Ok(_)) => return Err(CaptureProblem::NotSqlite),
        Ok(Err(RegularFileProblem::Missing)) => return Err(CaptureProblem::Missing),
        Ok(Err(_)) => return Err(CaptureProblem::NotARegularFile),
        Err(_) => return Err(CaptureProblem::Unreadable),
    }
    for _ in 0..ATTEMPTS {
        let before = identity_of(fs, &target).await?;
        let Read::Bytes(first) = read_file(
            fs,
            &dir,
            name,
            limits.database_bytes,
            CaptureProblem::DatabaseTooLarge,
            CaptureProblem::NotARegularFile,
        )
        .await?
        else {
            return Err(CaptureProblem::Missing);
        };
        let (first, digest) = offload(move || {
            let first = Zeroizing::new(first);
            let digest = Sha256::of(&first);
            (first, digest)
        })
        .await?;
        if !first.starts_with(SQLITE_MAGIC) {
            return Err(CaptureProblem::NotSqlite);
        }
        let log = match read_file(
            fs,
            &dir,
            &log_name,
            limits.log_bytes,
            CaptureProblem::LogTooLarge,
            CaptureProblem::LogUnreadable,
        )
        .await?
        {
            Read::Bytes(bytes) => Some(Zeroizing::new(bytes)),
            Read::Absent => None,
        };
        let Read::Bytes(again) = read_file(
            fs,
            &dir,
            name,
            limits.database_bytes,
            CaptureProblem::DatabaseTooLarge,
            CaptureProblem::NotARegularFile,
        )
        .await?
        else {
            continue;
        };
        let after = identity_of(fs, &target).await?;
        let (same, _wiped) = offload(move || {
            let again = Zeroizing::new(again);
            (Sha256::of(&again) == digest, again)
        })
        .await?;
        if before != after || !same {
            continue;
        }
        let image_bytes = limits.image_bytes;
        let image =
            offload(move || store_image(first, log.as_deref().map(Vec::as_slice), image_bytes))
                .await??;
        return Ok(Capture {
            image: StoreImage(image),
            identity: before,
        });
    }
    Err(CaptureProblem::Changed)
}

async fn offload<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, CaptureProblem> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| CaptureProblem::Unreadable)
}

async fn identity_of(
    fs: &dyn FileSystem,
    target: &HostPath,
) -> Result<FileIdentity, CaptureProblem> {
    fs.identity(target)
        .await
        .map_err(|_| CaptureProblem::Unreadable)?
        .ok_or(CaptureProblem::Missing)
}

fn store_image(
    database: Zeroizing<Vec<u8>>,
    log: Option<&[u8]>,
    image_bytes: usize,
) -> Result<Zeroizing<Vec<u8>>, CaptureProblem> {
    let mut image = database;
    if image.len() > image_bytes {
        return Err(CaptureProblem::ImageTooLarge);
    }
    if let Some(log) = log {
        apply_log(&mut image, log, image_bytes)?;
    }
    if let Some(header) = image.get_mut(18..20) {
        header.copy_from_slice(&[1, 1]);
    }
    Ok(image)
}

fn be32(bytes: &[u8]) -> Option<u32> {
    let array: [u8; 4] = bytes.try_into().ok()?;
    Some(u32::from_be_bytes(array))
}

fn be16(bytes: &[u8]) -> Option<u16> {
    let array: [u8; 2] = bytes.try_into().ok()?;
    Some(u16::from_be_bytes(array))
}

fn page_size_of(raw: u32) -> Option<usize> {
    let size = if raw == 1 {
        MAX_PAGE
    } else {
        usize::try_from(raw).ok()?
    };
    (MIN_PAGE..=MAX_PAGE)
        .contains(&size)
        .then_some(size)
        .filter(|size| size.is_power_of_two())
}

fn checksum(data: &[u8], mut one: u32, mut two: u32, big_endian: bool) -> (u32, u32) {
    let (pairs, _) = data.as_chunks::<8>();
    for pair in pairs {
        let (Some(first), Some(second)) = (pair.get(0..4), pair.get(4..8)) else {
            break;
        };
        let (Some(first), Some(second)) = (be32(first), be32(second)) else {
            break;
        };
        let (first, second) = if big_endian {
            (first, second)
        } else {
            (first.swap_bytes(), second.swap_bytes())
        };
        one = one.wrapping_add(first).wrapping_add(two);
        two = two.wrapping_add(second).wrapping_add(one);
    }
    (one, two)
}

fn apply_log(
    image: &mut Zeroizing<Vec<u8>>,
    log: &[u8],
    image_bytes: usize,
) -> Result<(), CaptureProblem> {
    let bad = CaptureProblem::LogUnreadable;
    if log.len() < WAL_HEADER {
        return Ok(());
    }
    let magic = log.get(0..4).and_then(be32).ok_or(bad)?;
    if magic != WAL_MAGIC && magic != WAL_MAGIC_SWAPPED {
        return Err(bad);
    }
    let big_endian = magic & 1 == 1;
    if log.get(4..8).and_then(be32) != Some(WAL_FORMAT) {
        return Err(bad);
    }
    let page_size = log
        .get(8..12)
        .and_then(be32)
        .and_then(page_size_of)
        .ok_or(bad)?;
    if image.len() < page_size || !image.len().is_multiple_of(page_size) {
        return Err(bad);
    }
    let stored_page_size = image
        .get(16..18)
        .and_then(be16)
        .and_then(|raw| page_size_of(u32::from(raw)))
        .ok_or(bad)?;
    if stored_page_size != page_size {
        return Err(bad);
    }
    let salt = log.get(16..24).ok_or(bad)?;
    let (mut one, mut two) = checksum(log.get(0..24).ok_or(bad)?, 0, 0, big_endian);
    if log.get(24..28).and_then(be32) != Some(one) || log.get(28..32).and_then(be32) != Some(two) {
        return Err(bad);
    }
    let frame_len = WAL_FRAME_HEADER + page_size;
    let mut frames: Vec<(u32, usize)> = Vec::new();
    let mut committed = 0_usize;
    let mut pages = 0_usize;
    let mut offset = WAL_HEADER;
    while let Some(end) = offset
        .checked_add(frame_len)
        .filter(|end| *end <= log.len())
    {
        let header = &log[offset..offset + WAL_FRAME_HEADER];
        let (Some(page), Some(after)) = (
            header.get(0..4).and_then(be32),
            header.get(4..8).and_then(be32),
        ) else {
            break;
        };
        if page == 0 || header.get(8..16) != Some(salt) {
            break;
        }
        let data = &log[offset + WAL_FRAME_HEADER..end];
        let (next_one, next_two) = checksum(&header[0..8], one, two, big_endian);
        let (next_one, next_two) = checksum(data, next_one, next_two, big_endian);
        if header.get(16..20).and_then(be32) != Some(next_one)
            || header.get(20..24).and_then(be32) != Some(next_two)
        {
            break;
        }
        (one, two) = (next_one, next_two);
        frames.push((page, offset + WAL_FRAME_HEADER));
        if after != 0 {
            committed = frames.len();
            pages = usize::try_from(after).map_err(|_| bad)?;
        }
        offset = end;
    }
    if committed == 0 {
        return Ok(());
    }
    let total = pages
        .checked_mul(page_size)
        .filter(|total| *total <= image_bytes)
        .ok_or(CaptureProblem::ImageTooLarge)?;
    image.resize(total, 0);
    for (page, at) in &frames[..committed] {
        let index = usize::try_from(*page).map_err(|_| bad)?;
        if index > pages {
            continue;
        }
        let start = (index - 1) * page_size;
        let source = log.get(*at..*at + page_size).ok_or(bad)?;
        image
            .get_mut(start..start + page_size)
            .ok_or(bad)?
            .copy_from_slice(source);
    }
    if let Some(size) = image.get_mut(28..32) {
        size.copy_from_slice(&u32::try_from(pages).map_err(|_| bad)?.to_be_bytes());
    }
    Ok(())
}
