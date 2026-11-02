// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::{Compression, Decompress, FlushDecompress, Status};
use sha1::{Digest, Sha1};
use sha2::Sha256;

pub(crate) const OFS_DELTA: u8 = 6;
pub(crate) const REF_DELTA: u8 = 7;

type Found<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Hash {
    Sha1,
    Sha256,
}

impl Hash {
    pub(crate) fn len(self) -> usize {
        match self {
            Hash::Sha1 => 20,
            Hash::Sha256 => 32,
        }
    }

    fn digest(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            Hash::Sha1 => Sha1::digest(bytes).to_vec(),
            Hash::Sha256 => Sha256::digest(bytes).to_vec(),
        }
    }
}

pub(crate) fn header_len(bundle: &[u8]) -> usize {
    bundle
        .windows(2)
        .position(|pair| pair == b"\n\n")
        .map_or(bundle.len(), |at| at + 2)
}

pub(crate) struct Entry {
    pub(crate) start: usize,
    pub(crate) kind: u8,
}

fn stream_len(input: &[u8]) -> Found<usize> {
    let mut stream = Decompress::new(true);
    let mut sink = vec![0_u8; 64 * 1024];
    loop {
        let used = usize::try_from(stream.total_in())?;
        let rest = input.get(used..).ok_or("the zlib stream is cut")?;
        if stream.decompress(rest, &mut sink, FlushDecompress::None)? == Status::StreamEnd {
            return Ok(usize::try_from(stream.total_in())?);
        }
    }
}

fn at(bundle: &[u8], index: usize) -> Found<u8> {
    bundle
        .get(index)
        .copied()
        .ok_or_else(|| "the pack is cut".into())
}

pub(crate) fn entries(bundle: &[u8], hash: Hash) -> Found<Vec<Entry>> {
    let mut cursor = header_len(bundle);
    let count_at = cursor + 8;
    let count = u32::from_be_bytes(
        bundle
            .get(count_at..count_at + 4)
            .ok_or("no pack header")?
            .try_into()?,
    );
    cursor += 12;
    let mut found = Vec::new();
    for _ in 0..count {
        let start = cursor;
        let first = at(bundle, cursor)?;
        let kind = (first >> 4) & 7;
        cursor += 1;
        let mut byte = first;
        while byte & 0x80 != 0 {
            byte = at(bundle, cursor)?;
            cursor += 1;
        }
        if kind == OFS_DELTA {
            while at(bundle, cursor)? & 0x80 != 0 {
                cursor += 1;
            }
            cursor += 1;
        } else if kind == REF_DELTA {
            cursor += hash.len();
        }
        cursor += stream_len(bundle.get(cursor..).ok_or("the pack is cut")?)?;
        found.push(Entry { start, kind });
    }
    Ok(found)
}

pub(crate) fn kinds(bundle: &[u8], hash: Hash) -> Found<BTreeMap<u8, usize>> {
    let mut counts = BTreeMap::new();
    for entry in entries(bundle, hash)? {
        *counts.entry(entry.kind).or_insert(0) += 1;
    }
    Ok(counts)
}

pub(crate) fn resign(bundle: &mut [u8], hash: Hash) {
    let from = header_len(bundle);
    let trailer = bundle.len() - hash.len();
    let digest = hash.digest(&bundle[from..trailer]);
    bundle[trailer..].copy_from_slice(&digest);
}

pub(crate) fn with_header(bundle: &[u8], header: &[u8]) -> Vec<u8> {
    let mut bytes = header.to_vec();
    bytes.extend_from_slice(&bundle[header_len(bundle)..]);
    bytes
}

pub(crate) const BLOB: u8 = 3;
pub(crate) const TREE: u8 = 2;
pub(crate) const COMMIT: u8 = 1;
pub(crate) const TAG: u8 = 4;

pub(crate) fn zlib(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    drop(encoder.write_all(bytes));
    encoder.finish().unwrap_or_default()
}

pub(crate) fn zeros(count: usize) -> Vec<u8> {
    vec![0; count]
}

pub(crate) fn varint(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let low = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            bytes.push(low);
            return bytes;
        }
        bytes.push(low | 0x80);
    }
}

pub(crate) fn typed_size(kind: u8, size: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut rest = size >> 4;
    let first = (kind << 4) | (size & 0x0f) as u8;
    if rest == 0 {
        bytes.push(first);
        return bytes;
    }
    bytes.push(first | 0x80);
    loop {
        let low = (rest & 0x7f) as u8;
        rest >>= 7;
        if rest == 0 {
            bytes.push(low);
            return bytes;
        }
        bytes.push(low | 0x80);
    }
}

pub(crate) fn entry(kind: u8, size_field: u64, payload: &[u8]) -> Vec<u8> {
    let mut bytes = typed_size(kind, size_field);
    bytes.extend(zlib(payload));
    bytes
}

pub(crate) fn blob(payload: &[u8]) -> Vec<u8> {
    entry(BLOB, payload.len() as u64, payload)
}

pub(crate) fn ref_delta(hash: Hash, size_field: u64, stream: &[u8]) -> Vec<u8> {
    ref_delta_on(&zeros(hash.len()), size_field, stream)
}

pub(crate) fn ref_delta_on(base: &[u8], size_field: u64, stream: &[u8]) -> Vec<u8> {
    let mut bytes = typed_size(REF_DELTA, size_field);
    bytes.extend_from_slice(base);
    bytes.extend(zlib(stream));
    bytes
}

pub(crate) fn delta_stream(base: u64, result: u64, ops: &[u8]) -> Vec<u8> {
    let mut stream = varint(base);
    stream.extend(varint(result));
    stream.extend_from_slice(ops);
    stream
}

pub(crate) fn copies(count: usize) -> Vec<u8> {
    vec![0x80; count]
}

pub(crate) fn assemble_open(header: &[u8], entries: &[Vec<u8>], count: u32) -> Vec<u8> {
    let mut bytes = header.to_vec();
    bytes.extend_from_slice(b"PACK");
    bytes.extend_from_slice(&2_u32.to_be_bytes());
    bytes.extend_from_slice(&count.to_be_bytes());
    for item in entries {
        bytes.extend_from_slice(item);
    }
    bytes
}

pub(crate) fn assemble(header: &[u8], entries: &[Vec<u8>], count: u32, hash: Hash) -> Vec<u8> {
    let mut bytes = assemble_open(header, entries, count);
    let digest = hash.digest(&bytes[header.len()..]);
    bytes.extend_from_slice(&digest);
    bytes
}
