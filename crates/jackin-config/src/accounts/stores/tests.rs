// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{CredentialKind, StoreCandidate, StoreKind, select_entry_secret};
use std::path::PathBuf;

#[test]
fn debug_redacts_secret_value() {
    let candidate = StoreCandidate::new(
        StoreKind::Opencode,
        "anthropic".to_owned(),
        None,
        PathBuf::from("/tmp/auth.json"),
        CredentialKind::ApiKey,
        "key".to_owned(),
        "fixture-credential-001".to_owned(),
    );
    let rendered = format!("{candidate:?}");
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("fixture-credential-001"));
    assert!(rendered.contains("anthropic"));
    // The secret is stored (equality distinguishes it) but never shown.
    let other = StoreCandidate::new(
        StoreKind::Opencode,
        "anthropic".to_owned(),
        None,
        PathBuf::from("/tmp/auth.json"),
        CredentialKind::ApiKey,
        "key".to_owned(),
        "fixture-credential-002".to_owned(),
    );
    assert_ne!(candidate, other);
    assert_eq!(candidate, candidate.clone());
}

#[test]
fn select_entry_secret_covers_shared_shapes() {
    let api: serde_json::Value = serde_json::from_str(r#"{"type":"api","key":"k"}"#).unwrap();
    assert_eq!(
        select_entry_secret(&api),
        Some((CredentialKind::ApiKey, "key", "k"))
    );
    let oauth: serde_json::Value =
        serde_json::from_str(r#"{"type":"oauth","access":"a","refresh":"r"}"#).unwrap();
    assert_eq!(
        select_entry_secret(&oauth),
        Some((CredentialKind::OAuth, "access", "a"))
    );
    let refresh_only: serde_json::Value =
        serde_json::from_str(r#"{"type":"oauth","refresh":"r"}"#).unwrap();
    assert_eq!(
        select_entry_secret(&refresh_only),
        Some((CredentialKind::OAuth, "refresh", "r"))
    );
    for raw in [
        r#"{"type":"api","key":"  "}"#,
        r#"{"type":"oauth"}"#,
        r#"{"type":"unknown","key":"k"}"#,
        r#"{"key":"k"}"#,
        r"[]",
    ] {
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(select_entry_secret(&value), None, "raw: {raw}");
    }
}

// ---------------------------------------------------------------------------
// Shared SQLite page builders for the store suites (`sqlite`, `omp`,
// `opencode`). Free items (not a child module: the test-layout gate forbids
// child modules in `tests.rs`). They live here rather than in the
// feature-gated cross-crate `test_support`, where test-only clippy
// exemptions do not apply.
// ---------------------------------------------------------------------------

/// Fixture page size: the smallest valid `SQLite` page.
pub(crate) const PAGE_SIZE: usize = 512;

/// Fixture column value.
#[derive(Clone, Debug)]
pub(crate) enum Value {
    /// SQL NULL.
    Null,
    /// Signed integer.
    Int(i64),
    /// IEEE-754 float.
    Float(f64),
    /// UTF-8 text.
    Text(String),
    /// Raw blob.
    Blob(Vec<u8>),
}

/// Fixture rowid-table cell.
#[derive(Clone, Debug)]
pub(crate) struct Cell {
    rowid: u64,
    values: Vec<Value>,
}

impl Cell {
    /// One leaf cell with the given rowid and values.
    pub(crate) fn row(rowid: u64, values: Vec<Value>) -> Self {
        Self { rowid, values }
    }
}

/// Encode one `SQLite` varint.
pub(crate) fn encode_varint(value: u64) -> Vec<u8> {
    if value < 128 {
        return vec![value as u8];
    }
    if value >> 56 != 0 {
        let mut out: Vec<u8> = Vec::with_capacity(9);
        let mut rest = value >> 8;
        let mut groups = Vec::with_capacity(8);
        for _ in 0..8 {
            groups.push((rest & 0x7F) as u8);
            rest >>= 7;
        }
        groups.reverse();
        for group in groups {
            out.push(group | 0x80);
        }
        out.push((value & 0xFF) as u8);
        return out;
    }
    let mut rest = value;
    let mut groups = Vec::new();
    while rest >= 128 {
        groups.push((rest & 0x7F) as u8);
        rest >>= 7;
    }
    groups.push(rest as u8);
    groups.reverse();
    let last = groups.len() - 1;
    for (index, group) in groups.iter_mut().enumerate() {
        if index != last {
            *group |= 0x80;
        }
    }
    groups
}

fn int_serial(value: i64) -> (u64, Vec<u8>) {
    if value == 0 {
        return (8, Vec::new());
    }
    if value == 1 {
        return (9, Vec::new());
    }
    for len in [1_usize, 2, 3, 4, 6, 8] {
        let (min, max) = if len == 8 {
            (i64::MIN, i64::MAX)
        } else {
            let half = 1_i64 << (len * 8 - 1);
            (-half, half - 1)
        };
        if value >= min && value <= max {
            let bytes = value.to_be_bytes();
            let serial = match len {
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 4,
                6 => 5,
                _ => 6,
            };
            return (serial, bytes[8 - len..].to_vec());
        }
    }
    unreachable!("i64 always fits in 8 bytes")
}

/// Encode one record payload from column values.
pub(crate) fn encode_record(values: &[Value]) -> Vec<u8> {
    let mut serials = Vec::new();
    let mut body = Vec::new();
    for value in values {
        match value {
            Value::Null => serials.push(encode_varint(0)),
            Value::Int(number) => {
                let (serial, bytes) = int_serial(*number);
                serials.push(encode_varint(serial));
                body.extend(bytes);
            }
            Value::Float(number) => {
                serials.push(encode_varint(7));
                body.extend(number.to_bits().to_be_bytes());
            }
            Value::Text(text) => {
                serials.push(encode_varint(13 + 2 * text.len() as u64));
                body.extend(text.as_bytes());
            }
            Value::Blob(bytes) => {
                serials.push(encode_varint(12 + 2 * bytes.len() as u64));
                body.extend(bytes.iter());
            }
        }
    }
    let header: Vec<u8> = serials.concat();
    let mut len = header.len() + 1;
    while encode_varint(len as u64).len() + header.len() != len {
        len += 1;
    }
    let mut out = encode_varint(len as u64);
    out.extend(header);
    out.extend(body);
    out
}

struct OverflowAlloc {
    next_page: u32,
    pages: Vec<Vec<u8>>,
}

fn leaf_cell_bytes(cell: &Cell, alloc: Option<&mut OverflowAlloc>) -> Vec<u8> {
    let payload = encode_record(&cell.values);
    let mut out = encode_varint(payload.len() as u64);
    out.extend(encode_varint(cell.rowid));
    const MAX_LOCAL: usize = PAGE_SIZE - 35;
    if payload.len() <= MAX_LOCAL {
        out.extend(payload);
        return out;
    }
    let alloc = alloc.expect("oversized fixture cell needs an overflow allocator");
    const MIN_LOCAL: usize = (PAGE_SIZE - 12) * 32 / 255 - 23;
    const STEP: usize = PAGE_SIZE - 4;
    let local = MIN_LOCAL + (payload.len() - MIN_LOCAL) % STEP;
    out.extend(&payload[..local]);
    let first = alloc.next_page;
    let mut rest = &payload[local..];
    while !rest.is_empty() {
        alloc.next_page += 1;
        let take = rest.len().min(STEP);
        let mut page = vec![0_u8; PAGE_SIZE];
        let next = if rest.len() > STEP {
            alloc.next_page
        } else {
            0
        };
        page[..4].copy_from_slice(&next.to_be_bytes());
        page[4..4 + take].copy_from_slice(&rest[..take]);
        alloc.pages.push(page);
        rest = &rest[take..];
    }
    out.extend(first.to_be_bytes());
    out
}

fn write_btree_header(
    page: &mut [u8],
    prefix: usize,
    kind: u8,
    cells: usize,
    content: u16,
    pointers: &[u16],
    right: Option<u32>,
) {
    page[prefix] = kind;
    page[prefix + 3..prefix + 5].copy_from_slice(&(cells as u16).to_be_bytes());
    page[prefix + 5..prefix + 7].copy_from_slice(&content.to_be_bytes());
    let mut at = prefix + if right.is_some() { 12 } else { 8 };
    if let Some(child) = right {
        page[prefix + 8..prefix + 12].copy_from_slice(&child.to_be_bytes());
    }
    for pointer in pointers {
        page[at..at + 2].copy_from_slice(&pointer.to_be_bytes());
        at += 2;
    }
}

fn leaf_page_inner(
    cells: &[Cell],
    prefix: usize,
    mut alloc: Option<&mut OverflowAlloc>,
) -> Vec<u8> {
    let mut sorted = cells.to_vec();
    sorted.sort_by_key(|cell| cell.rowid);
    let mut page = vec![0_u8; PAGE_SIZE];
    let mut free = PAGE_SIZE;
    let mut pointers = Vec::new();
    for cell in &sorted {
        let bytes = leaf_cell_bytes(cell, alloc.as_deref_mut());
        free -= bytes.len();
        page[free..free + bytes.len()].copy_from_slice(&bytes);
        pointers.push(free as u16);
    }
    write_btree_header(
        &mut page,
        prefix,
        0x0D,
        pointers.len(),
        free as u16,
        &pointers,
        None,
    );
    page
}

/// One leaf table page; cells are laid sorted by rowid without overflow.
pub(crate) fn leaf_page(cells: &[Cell], prefix: usize) -> Vec<u8> {
    leaf_page_inner(cells, prefix, None)
}

/// One interior table page with `(child, max_rowid)` entries.
pub(crate) fn interior_page(entries: &[(u32, u64)], right: u32) -> Vec<u8> {
    let mut page = vec![0_u8; PAGE_SIZE];
    let mut free = PAGE_SIZE;
    let mut pointers = Vec::new();
    for (child, key) in entries {
        let mut cell = child.to_be_bytes().to_vec();
        cell.extend(encode_varint(*key));
        free -= cell.len();
        page[free..free + cell.len()].copy_from_slice(&cell);
        pointers.push(free as u16);
    }
    write_btree_header(
        &mut page,
        0,
        0x05,
        pointers.len(),
        free as u16,
        &pointers,
        Some(right),
    );
    page
}

/// 100-byte database header, optionally selecting WAL mode.
pub(crate) fn db_header(wal_mode: bool) -> Vec<u8> {
    let mut header = vec![0_u8; 100];
    header[..16].copy_from_slice(b"SQLite format 3\0");
    header[16..18].copy_from_slice(&(PAGE_SIZE as u16).to_be_bytes());
    header[18] = if wal_mode { 2 } else { 1 };
    header[19] = if wal_mode { 2 } else { 1 };
    header
}

/// Whole database: schema page plus the table leaf at `root` (always 2).
pub(crate) fn database(sql: &str, root: u32, cells: &[Cell], wal_mode: bool) -> Vec<u8> {
    assert_eq!(root, 2, "fixture lays the table leaf at page 2");
    let name = table_name(sql);
    let schema = Cell::row(
        1,
        vec![
            Value::Text("table".to_owned()),
            Value::Text(name.clone()),
            Value::Text(name),
            Value::Int(i64::from(root)),
            Value::Text(sql.to_owned()),
        ],
    );
    let mut page_one = leaf_page(&[schema], 100);
    page_one[..100].copy_from_slice(&db_header(wal_mode));
    let mut alloc = OverflowAlloc {
        next_page: 3,
        pages: Vec::new(),
    };
    let mut sorted = cells.to_vec();
    sorted.sort_by_key(|cell| cell.rowid);
    let leaf = leaf_page_inner(&sorted, 0, Some(&mut alloc));
    let mut db = page_one;
    db.extend(leaf);
    for page in alloc.pages {
        db.extend(page);
    }
    db
}

fn table_name(sql: &str) -> String {
    let upper = sql.to_uppercase();
    let at = upper.find("TABLE").unwrap() + 5;
    let mut tokens = sql.get(at..).unwrap().split_whitespace();
    let mut name = tokens.next().unwrap().to_owned();
    if name.eq_ignore_ascii_case("IF") {
        tokens.next();
        tokens.next();
        name = tokens.next().unwrap().to_owned();
    }
    name.trim_matches(|char| matches!(char, '"' | '\'' | '`' | '[' | ']'))
        .to_owned()
}

/// One WAL image with `(page_no, commit_size, page_image)` frames.
pub(crate) fn wal_image(frames: &[(u32, u32, Vec<u8>)]) -> Vec<u8> {
    let mut wal = Vec::new();
    wal.extend(0x377F_0682_u32.to_be_bytes());
    wal.extend(3_007_000_u32.to_be_bytes());
    wal.extend((PAGE_SIZE as u32).to_be_bytes());
    wal.extend(1_u32.to_be_bytes());
    wal.extend(0x1111_2222_u32.to_be_bytes());
    wal.extend(0x3333_4444_u32.to_be_bytes());
    wal.extend(0_u32.to_be_bytes());
    wal.extend(0_u32.to_be_bytes());
    for (page, commit, image) in frames {
        assert_eq!(image.len(), PAGE_SIZE);
        wal.extend(page.to_be_bytes());
        wal.extend(commit.to_be_bytes());
        wal.extend(0x1111_2222_u32.to_be_bytes());
        wal.extend(0x3333_4444_u32.to_be_bytes());
        wal.extend(0_u32.to_be_bytes());
        wal.extend(0_u32.to_be_bytes());
        wal.extend(image);
    }
    wal
}
