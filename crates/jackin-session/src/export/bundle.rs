// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use flate2::{Decompress, FlushDecompress, Status};
use jackin_core::{CommitId, FileSystem, HostPath};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::ExportError;
use crate::project::ObjectFormat;

pub(super) const MAX_HEADER_BYTES: usize = 1024 * 1024;
pub(super) const MAX_HEADS: usize = 1024;
const CHUNK: usize = 64 * 1024;
const MAGIC: [u8; 4] = *b"PACK";
const VERSION: u32 = 2;
const OFS_DELTA: u8 = 6;
const REF_DELTA: u8 = 7;
const MORE: u8 = 0x80;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum BundleRefusal {
    #[error("the bundle header is malformed or over its limits")]
    Header,
    #[error("the bundle lists a prerequisite")]
    Prerequisite,
    #[error("the bundle declares a capability that jackin does not accept")]
    Capability,
    #[error("the bundle object format is {found} and the host repository uses {expected}")]
    ObjectFormat {
        found: ObjectFormat,
        expected: ObjectFormat,
    },
    #[error("the pack does not start with PACK")]
    PackMagic,
    #[error("the pack version is {0} and only version 2 is accepted")]
    PackVersion(u32),
    #[error("the pack ends before its declared content")]
    Truncated,
    #[error("a pack entry has the invalid type {0}")]
    EntryType(u8),
    #[error("a pack entry holds a corrupt zlib stream")]
    Inflate,
    #[error("a pack entry declares {declared} bytes and inflates to at least {inflated}")]
    SizeMismatch { declared: u64, inflated: u64 },
    #[error("the pack trailer is not the hash of the pack")]
    Trailer,
    #[error("bytes follow the pack trailer")]
    TrailingBytes,
}

#[derive(Debug)]
pub(super) struct Header {
    pub(super) heads: BTreeMap<String, CommitId>,
    pub(super) pack_offset: u64,
}

fn refused(refusal: BundleRefusal) -> ExportError {
    ExportError::BundleRefused(refusal)
}

enum Hasher {
    Sha1(Sha1),
    Sha256(Sha256),
}

impl Hasher {
    fn new(format: ObjectFormat) -> Hasher {
        match format {
            ObjectFormat::Sha1 => Hasher::Sha1(Sha1::new()),
            ObjectFormat::Sha256 => Hasher::Sha256(Sha256::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Hasher::Sha1(hasher) => hasher.update(bytes),
            Hasher::Sha256(hasher) => hasher.update(bytes),
        }
    }

    fn matches(self, trailer: &[u8]) -> bool {
        match self {
            Hasher::Sha1(hasher) => hasher.finalize().as_slice() == trailer,
            Hasher::Sha256(hasher) => hasher.finalize().as_slice() == trailer,
        }
    }
}

fn hash_len(format: ObjectFormat) -> usize {
    match format {
        ObjectFormat::Sha1 => 20,
        ObjectFormat::Sha256 => 32,
    }
}

async fn read_chunk(
    fs: &dyn FileSystem,
    path: &HostPath,
    offset: u64,
    len: usize,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, ExportError> {
    let bytes = fs.read_at(path, offset, len).await?;
    if cancel.is_cancelled() {
        return Err(ExportError::Cancelled);
    }
    Ok(bytes)
}

fn parse_format(capabilities: &[&[u8]], version: u8) -> Result<ObjectFormat, BundleRefusal> {
    match (version, capabilities) {
        (_, []) => Ok(ObjectFormat::Sha1),
        (3, [only]) => match *only {
            b"@object-format=sha1" => Ok(ObjectFormat::Sha1),
            b"@object-format=sha256" => Ok(ObjectFormat::Sha256),
            _ => Err(BundleRefusal::Capability),
        },
        _ => Err(BundleRefusal::Capability),
    }
}

fn parse_head(line: &[u8], format: ObjectFormat) -> Result<(String, CommitId), BundleRefusal> {
    let width = hash_len(format) * 2;
    let (id, name) = match line.split_at_checked(width) {
        Some((id, rest)) => match rest.split_first() {
            Some((b' ', name)) if !name.is_empty() => (id, name),
            _ => return Err(BundleRefusal::Header),
        },
        None => return Err(BundleRefusal::Header),
    };
    let id = std::str::from_utf8(id)
        .ok()
        .and_then(|text| CommitId::parse(text).ok())
        .ok_or(BundleRefusal::Header)?;
    Ok((String::from_utf8_lossy(name).into_owned(), id))
}

fn parse_lines(
    header: &[u8],
    expected: ObjectFormat,
) -> Result<BTreeMap<String, CommitId>, BundleRefusal> {
    let body = header.strip_suffix(b"\n\n").ok_or(BundleRefusal::Header)?;
    let mut lines = body.split(|byte| *byte == b'\n');
    let version = match lines.next() {
        Some(b"# v2 git bundle") => 2,
        Some(b"# v3 git bundle") => 3,
        _ => return Err(BundleRefusal::Header),
    };
    let rest: Vec<&[u8]> = lines.collect();
    let split = rest
        .iter()
        .position(|line| !line.starts_with(b"@"))
        .unwrap_or(rest.len());
    let (capabilities, lines) = rest.split_at(split);
    let found = parse_format(capabilities, version)?;
    if found != expected {
        return Err(BundleRefusal::ObjectFormat { found, expected });
    }
    let mut heads = BTreeMap::new();
    for line in lines {
        if line.starts_with(b"@") {
            return Err(BundleRefusal::Capability);
        }
        if line.starts_with(b"-") {
            return Err(BundleRefusal::Prerequisite);
        }
        if heads.len() >= MAX_HEADS {
            return Err(BundleRefusal::Header);
        }
        let (name, id) = parse_head(line, found)?;
        if heads.insert(name, id).is_some() {
            return Err(BundleRefusal::Header);
        }
    }
    Ok(heads)
}

pub(super) async fn header(
    fs: &dyn FileSystem,
    path: &HostPath,
    expected: ObjectFormat,
    cancel: &CancellationToken,
) -> Result<Header, ExportError> {
    let mut bytes: Vec<u8> = Vec::new();
    let end = loop {
        if let Some(at) = bytes.windows(2).position(|pair| pair == b"\n\n") {
            break at + 2;
        }
        if bytes.len() >= MAX_HEADER_BYTES {
            return Err(refused(BundleRefusal::Header));
        }
        let wanted = CHUNK.min(MAX_HEADER_BYTES - bytes.len());
        let offset = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let more = read_chunk(fs, path, offset, wanted, cancel).await?;
        if more.is_empty() {
            return Err(refused(BundleRefusal::Header));
        }
        bytes.extend_from_slice(&more);
    };
    let heads = parse_lines(&bytes[..end], expected).map_err(refused)?;
    Ok(Header {
        heads,
        pack_offset: u64::try_from(end).unwrap_or(u64::MAX),
    })
}

struct Pack<'a> {
    fs: &'a dyn FileSystem,
    path: &'a HostPath,
    cancel: &'a CancellationToken,
    format: ObjectFormat,
    buffer: Vec<u8>,
    at: usize,
    next: u64,
    hasher: Hasher,
    scratch: Vec<u8>,
}

impl Pack<'_> {
    async fn refill(&mut self) -> Result<bool, ExportError> {
        if self.at < self.buffer.len() {
            return Ok(true);
        }
        self.buffer = read_chunk(self.fs, self.path, self.next, CHUNK, self.cancel).await?;
        self.at = 0;
        self.next = self
            .next
            .saturating_add(u64::try_from(self.buffer.len()).unwrap_or(u64::MAX));
        Ok(!self.buffer.is_empty())
    }

    async fn byte(&mut self) -> Result<u8, ExportError> {
        if !self.refill().await? {
            return Err(refused(BundleRefusal::Truncated));
        }
        let byte = self.buffer[self.at];
        self.hasher.update(&[byte]);
        self.at += 1;
        Ok(byte)
    }

    async fn take(&mut self, count: usize) -> Result<(), ExportError> {
        for _ in 0..count {
            self.byte().await?;
        }
        Ok(())
    }

    async fn word(&mut self) -> Result<u32, ExportError> {
        let mut word = [0_u8; 4];
        for slot in &mut word {
            *slot = self.byte().await?;
        }
        Ok(u32::from_be_bytes(word))
    }

    async fn trailer(&mut self) -> Result<Vec<u8>, ExportError> {
        let mut taken = Vec::new();
        while taken.len() < hash_len(self.format) {
            if !self.refill().await? {
                return Err(refused(BundleRefusal::Truncated));
            }
            taken.push(self.buffer[self.at]);
            self.at += 1;
        }
        Ok(taken)
    }

    async fn entry_size(&mut self, first: u8) -> Result<u64, ExportError> {
        let mut size = u64::from(first & 0x0f);
        let mut shift = 4_u32;
        let mut byte = first;
        while byte & MORE != 0 {
            byte = self.byte().await?;
            if shift > 63 {
                return Err(refused(BundleRefusal::SizeMismatch {
                    declared: u64::MAX,
                    inflated: 0,
                }));
            }
            size |= u64::from(byte & 0x7f) << shift;
            shift += 7;
        }
        Ok(size)
    }

    async fn skip_offset(&mut self) -> Result<(), ExportError> {
        loop {
            if self.byte().await? & MORE == 0 {
                return Ok(());
            }
        }
    }

    async fn inflate(&mut self, declared: u64) -> Result<(), ExportError> {
        let mut stream = Decompress::new(true);
        let mut inflated: u64 = 0;
        loop {
            if !self.refill().await? {
                return Err(refused(BundleRefusal::Truncated));
            }
            let input = &self.buffer[self.at..];
            let (before_in, before_out) = (stream.total_in(), stream.total_out());
            let status = stream
                .decompress(input, &mut self.scratch, FlushDecompress::None)
                .map_err(|_| refused(BundleRefusal::Inflate))?;
            let used = usize::try_from(stream.total_in() - before_in).unwrap_or(usize::MAX);
            let produced = stream.total_out() - before_out;
            self.hasher.update(&input[..used]);
            self.at += used;
            inflated = inflated.saturating_add(produced);
            if inflated > declared {
                return Err(refused(BundleRefusal::SizeMismatch { declared, inflated }));
            }
            if status == Status::StreamEnd {
                break;
            }
            if used == 0 && produced == 0 {
                return Err(refused(BundleRefusal::Inflate));
            }
            if self.cancel.is_cancelled() {
                return Err(ExportError::Cancelled);
            }
            tokio::task::yield_now().await;
        }
        if inflated == declared {
            Ok(())
        } else {
            Err(refused(BundleRefusal::SizeMismatch { declared, inflated }))
        }
    }

    async fn entry(&mut self) -> Result<(), ExportError> {
        let first = self.byte().await?;
        let kind = (first >> 4) & 0x07;
        let declared = self.entry_size(first).await?;
        match kind {
            1..=4 => {}
            OFS_DELTA => self.skip_offset().await?,
            REF_DELTA => {
                let width = hash_len(self.format);
                self.take(width).await?;
            }
            other => return Err(refused(BundleRefusal::EntryType(other))),
        }
        self.inflate(declared).await
    }
}

pub(super) async fn walk(
    fs: &dyn FileSystem,
    path: &HostPath,
    pack_offset: u64,
    format: ObjectFormat,
    cancel: &CancellationToken,
) -> Result<(), ExportError> {
    let mut pack = Pack {
        fs,
        path,
        cancel,
        format,
        buffer: Vec::new(),
        at: 0,
        next: pack_offset,
        hasher: Hasher::new(format),
        scratch: vec![0; CHUNK],
    };
    let mut magic = [0_u8; 4];
    for slot in &mut magic {
        *slot = pack.byte().await?;
    }
    if magic != MAGIC {
        return Err(refused(BundleRefusal::PackMagic));
    }
    let version = pack.word().await?;
    if version != VERSION {
        return Err(refused(BundleRefusal::PackVersion(version)));
    }
    let count = pack.word().await?;
    for _ in 0..count {
        pack.entry().await?;
    }
    let trailer = pack.trailer().await?;
    let hasher = std::mem::replace(&mut pack.hasher, Hasher::new(format));
    if !hasher.matches(&trailer) {
        return Err(refused(BundleRefusal::Trailer));
    }
    if pack.refill().await? {
        return Err(refused(BundleRefusal::TrailingBytes));
    }
    Ok(())
}
