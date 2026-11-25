// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use zeroize::Zeroizing;

use crate::boundary::{FileIdentity, FileKind, FileSystem, FsError, Hold, RegularFileProblem};
use crate::paths::{HostPath, RepoPath};

pub const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
const JOURNAL_MAGIC: &[u8] = &[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7];

const ATTEMPTS: usize = 5;
const RETRY_PAUSE: Duration = Duration::from_millis(40);
const CAPTURE_SLOTS: usize = 2;
const PREFLIGHT_SLOTS: usize = 4;
const WAL_HEADER: usize = 32;
const WAL_FRAME_HEADER: usize = 24;
const WAL_MAGIC: u32 = 0x377f_0682;
const WAL_MAGIC_SWAPPED: u32 = 0x377f_0683;
const WAL_FORMAT: u32 = 3_007_000;
const WAL_FORMAT_VERSION: u8 = 2;
const MIN_PAGE: usize = 512;
const MAX_PAGE: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureLimits {
    pub database_bytes: usize,
    pub log_bytes: usize,
    pub image_bytes: usize,
    pub deadline: Duration,
}

pub const OMP_CAPTURE_LIMITS: CaptureLimits = CaptureLimits {
    database_bytes: 64 * 1024 * 1024,
    log_bytes: 16 * 1024 * 1024,
    image_bytes: 80 * 1024 * 1024,
    deadline: Duration::from_secs(30),
};
pub const STORE_VALUE_MAX_BYTES: usize = 64 * 1024;
pub const STORE_QUERY_DEADLINE: Duration = Duration::from_secs(5);
pub const STORE_PRAGMAS: &str = "PRAGMA temp_store = MEMORY; PRAGMA trusted_schema = OFF;";
pub const STORE_QUERY_STEP: i32 = 1000;
const STORE_QUERY_GRACE: Duration = Duration::from_secs(1);
pub const STORE_QUERY_MAX_STEPS: usize = 200_000;
pub const STORE_LENGTH_LIMIT: i32 = 128 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaptureProblem {
    Missing,
    NotARegularFile,
    Symlink,
    NotSqlite,
    DatabaseTooLarge,
    LogTooLarge,
    ImageTooLarge,
    LogUnreadable,
    Unreadable,
    Changed,
    TimedOut,
}

pub struct StoreImage {
    bytes: Zeroizing<Vec<u8>>,
    _slot: Slot,
}

impl std::fmt::Debug for StoreImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StoreImage({} bytes)", self.bytes.len())
    }
}

impl StoreImage {
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn wipe(&mut self) {
        self.bytes = Zeroizing::new(Vec::new());
    }
}

#[derive(Debug)]
pub struct Capture {
    pub image: StoreImage,
    pub identity: FileIdentity,
}

enum Read {
    Bytes(Zeroizing<Vec<u8>>),
    Absent,
}

type Slot = Arc<OwnedSemaphorePermit>;

fn capture_slots() -> &'static Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(CAPTURE_SLOTS)))
}

fn preflight_slots() -> &'static Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(PREFLIGHT_SLOTS)))
}

async fn preflight_slot() -> Result<Slot, CaptureProblem> {
    Arc::clone(preflight_slots())
        .acquire_owned()
        .await
        .map(Arc::new)
        .map_err(|_| CaptureProblem::Unreadable)
}

pub(crate) async fn within_preflight<T, Work, Running>(
    deadline: Duration,
    work: Work,
) -> Result<T, CaptureProblem>
where
    Work: FnOnce(Hold) -> Running,
    Running: Future<Output = T>,
{
    tokio::time::timeout(deadline, async {
        let slot = preflight_slot().await?;
        let hold: Hold = slot;
        Ok(work(hold).await)
    })
    .await
    .map_err(|_| CaptureProblem::TimedOut)?
}

fn is_wal_mode(header: &[u8]) -> bool {
    header.get(19) == Some(&WAL_FORMAT_VERSION)
}

async fn read_file(
    fs: &dyn FileSystem,
    dir: &HostPath,
    name: &str,
    limit: usize,
    kinds: (CaptureProblem, CaptureProblem),
    slot: &Slot,
) -> Result<Read, CaptureProblem> {
    let (too_large, unusable) = kinds;
    let file = RepoPath::parse(name).map_err(|_| CaptureProblem::Unreadable)?;
    let hold: Hold = Arc::<OwnedSemaphorePermit>::clone(slot);
    match fs.read_regular_holding(dir, &file, limit, hold).await {
        Ok(Ok(found)) => {
            let bytes = Zeroizing::new(found.bytes);
            if bytes.len() > limit {
                Err(too_large)
            } else {
                Ok(Read::Bytes(bytes))
            }
        }
        Ok(Err(RegularFileProblem::Missing))
        | Err(FsError::Io {
            kind: std::io::ErrorKind::NotFound,
            ..
        }) => Ok(Read::Absent),
        Ok(Err(RegularFileProblem::Symlink | RegularFileProblem::NotARegularFile)) => Err(unusable),
        Err(_) => Err(if matches!(unusable, CaptureProblem::LogUnreadable) {
            unusable
        } else {
            CaptureProblem::Unreadable
        }),
    }
}

pub async fn capture(
    fs: &dyn FileSystem,
    database: &HostPath,
    limits: CaptureLimits,
) -> Result<Capture, CaptureProblem> {
    tokio::time::timeout(limits.deadline, capture_pinned(fs, database, limits))
        .await
        .map_err(|_| CaptureProblem::TimedOut)?
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fence {
    dir: FileIdentity,
    database: FileIdentity,
    log: Option<FileIdentity>,
}

async fn fence_of(
    fs: &dyn FileSystem,
    dir: &HostPath,
    database: &HostPath,
    log: &HostPath,
    slot: &Slot,
) -> Result<Fence, CaptureProblem> {
    Ok(Fence {
        dir: identity_of(fs, dir, slot).await?,
        database: identity_of(fs, database, slot).await?,
        log: fs
            .identity_holding(log, Arc::<OwnedSemaphorePermit>::clone(slot))
            .await
            .map_err(|_| CaptureProblem::Unreadable)?,
    })
}

async fn capture_pinned(
    fs: &dyn FileSystem,
    database: &HostPath,
    limits: CaptureLimits,
) -> Result<Capture, CaptureProblem> {
    let preflight = preflight_slot().await?;
    match fs
        .stat_holding(database, Arc::<OwnedSemaphorePermit>::clone(&preflight))
        .await
    {
        Ok(Some(FileKind::Symlink)) => return Err(classify_link(fs, database, &preflight).await),
        Ok(_) => {}
        Err(_) => return Err(CaptureProblem::Unreadable),
    }
    let (Some(parent), Some(name)) = (
        database.parent(),
        database.file_name().and_then(|name| name.to_str()),
    ) else {
        return Err(CaptureProblem::NotARegularFile);
    };
    let dir = fs
        .canonicalize_holding(&parent, Arc::<OwnedSemaphorePermit>::clone(&preflight))
        .await
        .map_err(|_| CaptureProblem::Unreadable)?
        .ok_or(CaptureProblem::Missing)?;
    let target = dir.join_name(name);
    let log_name = format!("{name}-wal");
    let log_target = dir.join_name(&log_name);
    let journal_name = format!("{name}-journal");
    let probe = RepoPath::parse(name).map_err(|_| CaptureProblem::Unreadable)?;
    let hold: Hold = Arc::<OwnedSemaphorePermit>::clone(&preflight);
    match fs
        .read_regular_holding(&dir, &probe, SQLITE_MAGIC.len(), hold)
        .await
    {
        Ok(Ok(head)) if head.bytes.starts_with(SQLITE_MAGIC) => {}
        Ok(Ok(_)) => return Err(CaptureProblem::NotSqlite),
        Ok(Err(RegularFileProblem::Missing)) => return Err(CaptureProblem::Missing),
        Ok(Err(_)) => return Err(CaptureProblem::NotARegularFile),
        Err(_) => return Err(CaptureProblem::Unreadable),
    }
    drop(preflight);
    let slot: Slot = Arc::new(
        Arc::clone(capture_slots())
            .acquire_owned()
            .await
            .map_err(|_| CaptureProblem::Unreadable)?,
    );
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(RETRY_PAUSE * u32::try_from(attempt).unwrap_or(1)).await;
        }
        if journal_is_hot(fs, &dir, &journal_name, &slot).await? {
            continue;
        }
        let before = fence_of(fs, &dir, &target, &log_target, &slot).await?;
        let Some((first, log)) = read_pair(fs, &dir, name, &log_name, limits, &slot).await? else {
            continue;
        };
        if !first.starts_with(SQLITE_MAGIC) {
            return Err(CaptureProblem::NotSqlite);
        }
        if journal_is_hot(fs, &dir, &journal_name, &slot).await? {
            continue;
        }
        let first = if is_wal_mode(&first) {
            first
        } else {
            let again = read_database(fs, &dir, name, limits, &slot).await?;
            let held = Arc::clone(&slot);
            let (first, same) = offload(move || {
                let _held = held;
                let same =
                    matches!(&again, Read::Bytes(bytes) if bytes.as_slice() == first.as_slice());
                (first, same)
            })
            .await?;
            if !same {
                continue;
            }
            first
        };
        if before != fence_of(fs, &dir, &target, &log_target, &slot).await? {
            continue;
        }
        let image_bytes = limits.image_bytes;
        let held = Arc::clone(&slot);
        let image = offload(move || {
            let _held = held;
            store_image(first, log.as_deref().map(Vec::as_slice), image_bytes)
        })
        .await??;
        return Ok(Capture {
            image: StoreImage {
                bytes: image,
                _slot: slot,
            },
            identity: before.database,
        });
    }
    Err(CaptureProblem::Changed)
}

async fn classify_link(fs: &dyn FileSystem, link: &HostPath, slot: &Slot) -> CaptureProblem {
    let Ok(Some(target)) = fs
        .canonicalize_holding(link, Arc::<OwnedSemaphorePermit>::clone(slot))
        .await
    else {
        return CaptureProblem::NotSqlite;
    };
    let (Some(dir), Some(name)) = (
        target.parent(),
        target
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| RepoPath::parse(name).ok()),
    ) else {
        return CaptureProblem::NotSqlite;
    };
    let hold: Hold = Arc::<OwnedSemaphorePermit>::clone(slot);
    match fs
        .read_regular_holding(&dir, &name, SQLITE_MAGIC.len(), hold)
        .await
    {
        Ok(Ok(head)) if head.bytes.starts_with(SQLITE_MAGIC) => CaptureProblem::Symlink,
        _ => CaptureProblem::NotSqlite,
    }
}

type Pair = (Zeroizing<Vec<u8>>, Option<Zeroizing<Vec<u8>>>);

async fn read_database(
    fs: &dyn FileSystem,
    dir: &HostPath,
    name: &str,
    limits: CaptureLimits,
    slot: &Slot,
) -> Result<Read, CaptureProblem> {
    read_file(
        fs,
        dir,
        name,
        limits.database_bytes,
        (
            CaptureProblem::DatabaseTooLarge,
            CaptureProblem::NotARegularFile,
        ),
        slot,
    )
    .await
}

async fn read_log(
    fs: &dyn FileSystem,
    dir: &HostPath,
    name: &str,
    limits: CaptureLimits,
    slot: &Slot,
) -> Result<Read, CaptureProblem> {
    read_file(
        fs,
        dir,
        name,
        limits.log_bytes,
        (CaptureProblem::LogTooLarge, CaptureProblem::LogUnreadable),
        slot,
    )
    .await
}

fn same_bytes(one: &Read, two: &Read) -> bool {
    match (one, two) {
        (Read::Bytes(one), Read::Bytes(two)) => one.as_slice() == two.as_slice(),
        (Read::Absent, Read::Absent) => true,
        _ => false,
    }
}

fn log_only_grew(one: &Read, two: &Read) -> bool {
    match (one, two) {
        (Read::Bytes(one), Read::Bytes(two)) => {
            two.len() >= one.len()
                && two.starts_with(one)
                && (one.len() >= WAL_HEADER || one.len() == two.len())
        }
        (Read::Absent, Read::Absent) => true,
        _ => false,
    }
}

async fn read_pair(
    fs: &dyn FileSystem,
    dir: &HostPath,
    name: &str,
    log_name: &str,
    limits: CaptureLimits,
    slot: &Slot,
) -> Result<Option<Pair>, CaptureProblem> {
    let first = read_database(fs, dir, name, limits, slot).await?;
    if matches!(first, Read::Absent) {
        return Err(CaptureProblem::Missing);
    }
    let wal_mode = matches!(&first, Read::Bytes(bytes) if is_wal_mode(bytes));
    let log_first = if wal_mode {
        read_log(fs, dir, log_name, limits, slot).await?
    } else {
        Read::Absent
    };
    let again = read_database(fs, dir, name, limits, slot).await?;
    let held = Arc::clone(slot);
    let (first, same) = offload(move || {
        let _held = held;
        let same = same_bytes(&first, &again);
        (first, same)
    })
    .await?;
    if !same {
        return Ok(None);
    }
    let log_again = if wal_mode {
        read_log(fs, dir, log_name, limits, slot).await?
    } else {
        Read::Absent
    };
    let held = Arc::clone(slot);
    let (log_first, same) = offload(move || {
        let _held = held;
        let same = log_only_grew(&log_first, &log_again);
        (log_first, same)
    })
    .await?;
    let (Read::Bytes(first), true) = (first, same) else {
        return Ok(None);
    };
    let log = match log_first {
        Read::Bytes(bytes) => Some(bytes),
        Read::Absent => None,
    };
    Ok(Some((first, log)))
}

async fn journal_is_hot(
    fs: &dyn FileSystem,
    dir: &HostPath,
    name: &str,
    slot: &Slot,
) -> Result<bool, CaptureProblem> {
    let file = RepoPath::parse(name).map_err(|_| CaptureProblem::Unreadable)?;
    let hold: Hold = Arc::<OwnedSemaphorePermit>::clone(slot);
    match fs
        .read_regular_holding(dir, &file, JOURNAL_MAGIC.len(), hold)
        .await
    {
        Ok(Ok(head)) => Ok(head.bytes.starts_with(JOURNAL_MAGIC)),
        Ok(Err(RegularFileProblem::Missing))
        | Err(FsError::Io {
            kind: std::io::ErrorKind::NotFound,
            ..
        }) => Ok(false),
        Ok(Err(_)) | Err(_) => Err(CaptureProblem::Unreadable),
    }
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
    slot: &Slot,
) -> Result<FileIdentity, CaptureProblem> {
    fs.identity_holding(target, Arc::<OwnedSemaphorePermit>::clone(slot))
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
    if let Some(log) = log.filter(|_| is_wal_mode(&image)) {
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
    if total > image.len() {
        let mut grown = Zeroizing::new(Vec::new());
        grown
            .try_reserve_exact(total)
            .map_err(|_| CaptureProblem::ImageTooLarge)?;
        grown.extend_from_slice(image);
        grown.resize(total, 0);
        *image = grown;
    } else {
        image.truncate(total);
    }
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

#[derive(Debug)]
pub struct QueryBudget {
    deadline: Instant,
    stopped: Arc<AtomicBool>,
    steps: usize,
}

impl QueryBudget {
    pub fn spent(&mut self) -> bool {
        self.steps += 1;
        self.steps > STORE_QUERY_MAX_STEPS
            || self.stopped.load(Ordering::SeqCst)
            || Instant::now() >= self.deadline
    }
}

struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

pub async fn run_bounded<T: Send + 'static>(
    deadline: Duration,
    work: impl FnOnce(QueryBudget) -> T + Send + 'static,
) -> Result<T, CaptureProblem> {
    let stopped = Arc::new(AtomicBool::new(false));
    let _stop = StopOnDrop(Arc::clone(&stopped));
    let (Some(ends), Some(patience)) = (
        Instant::now().checked_add(deadline),
        deadline.checked_add(STORE_QUERY_GRACE),
    ) else {
        return Err(CaptureProblem::TimedOut);
    };
    let budget = QueryBudget {
        deadline: ends,
        stopped,
        steps: 0,
    };
    let task = tokio::task::spawn_blocking(move || work(budget));
    match tokio::time::timeout(patience, task).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(_)) => Err(CaptureProblem::Unreadable),
        Err(_) => Err(CaptureProblem::TimedOut),
    }
}
