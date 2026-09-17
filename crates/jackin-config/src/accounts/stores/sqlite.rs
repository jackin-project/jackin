// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Minimal read-only `SQLite` parser shared by the store enumerators.
//!
//! `rusqlite` is not a workspace dependency and Turso is sole-owned by
//! `jackin-usage`, so the `.omp` and `opencode` database readers parse the
//! file format directly with no third-party database code. The reader
//! understands rowid table b-trees (interior pages, leaf pages, and overflow
//! chains) plus enough of the `sqlite_schema` table and `CREATE TABLE`
//! definitions to locate one table and map its columns.
//!
//! WAL handling: when the database header selects WAL mode and a sibling
//! `-wal` image is supplied, committed frames are overlaid onto an in-memory
//! page image. Nothing is ever written back: no checkpoint, no `-shm`
//! creation, no migration. Frame checksums are not validated; salt or shape
//! mismatches fail closed with [`StoreError::Malformed`].

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::StoreError;

/// Maximum b-tree depth followed before treating the image as corrupt.
const MAX_BTREE_DEPTH: u32 = 32;

/// Sibling `-wal` path for a database file. Appends the suffix; the
/// extension is preserved (`agent.db` pairs with `agent.db-wal`).
pub(crate) fn wal_sibling_path(db_path: &Path) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push("-wal");
    PathBuf::from(name)
}

/// In-memory database page image with committed WAL frames overlaid.
#[derive(Debug)]
pub(crate) struct SqliteImage {
    usable: usize,
    pages: Vec<Vec<u8>>,
}

impl SqliteImage {
    /// Load a database image, overlaying `wal` when the header selects WAL.
    pub(crate) fn load(db: &[u8], wal: Option<&[u8]>) -> Result<Self, StoreError> {
        let magic: &[u8] = b"SQLite format 3\0";
        if db.get(..16) != Some(magic) {
            return Err(StoreError::Malformed);
        }
        let raw_size = read_u16(db, 16)?;
        let page_size = if raw_size == 1 {
            65_536_usize
        } else {
            usize::from(raw_size)
        };
        if !page_size.is_power_of_two() || !(512..=65_536).contains(&page_size) {
            return Err(StoreError::Malformed);
        }
        let reserved = usize::from(db.get(20).copied().ok_or(StoreError::Malformed)?);
        let usable = page_size
            .checked_sub(reserved)
            .ok_or(StoreError::Malformed)?;
        if usable < 256 || !db.len().is_multiple_of(page_size) || db.is_empty() {
            return Err(StoreError::Malformed);
        }
        let mut pages: Vec<Vec<u8>> = db.chunks(page_size).map(<[u8]>::to_vec).collect();
        let read_version = db.get(18).copied().ok_or(StoreError::Malformed)?;
        let write_version = db.get(19).copied().ok_or(StoreError::Malformed)?;
        if read_version == 2
            && write_version == 2
            && let Some(frames) = wal
        {
            apply_wal(&mut pages, page_size, frames)?;
        }
        Ok(Self { usable, pages })
    }

    /// Read every row of the rowid table rooted at `root`, in rowid order.
    pub(crate) fn walk_table(&self, root: u32) -> Result<Vec<TableRow>, StoreError> {
        let mut rows = Vec::new();
        let mut visited = BTreeSet::new();
        self.walk_page(root, 0, &mut visited, &mut rows)?;
        Ok(rows)
    }

    fn page_bytes(&self, page_no: u32) -> Result<&[u8], StoreError> {
        let index = usize::try_from(page_no.checked_sub(1).ok_or(StoreError::Malformed)?)
            .map_err(|_| StoreError::Malformed)?;
        self.pages
            .get(index)
            .map(Vec::as_slice)
            .ok_or(StoreError::Malformed)
    }

    fn walk_page(
        &self,
        page_no: u32,
        depth: u32,
        visited: &mut BTreeSet<u32>,
        rows: &mut Vec<TableRow>,
    ) -> Result<(), StoreError> {
        if depth > MAX_BTREE_DEPTH || page_no == 0 || !visited.insert(page_no) {
            return Err(StoreError::Malformed);
        }
        let raw = self.page_bytes(page_no)?;
        let base = if page_no == 1 { 100 } else { 0 };
        let area = raw.get(base..).ok_or(StoreError::Malformed)?;
        match area.first().copied().ok_or(StoreError::Malformed)? {
            0x05 => {
                let cells = read_u16(area, 3)?;
                for index in 0..cells {
                    let offset = 12_usize
                        .checked_add(
                            usize::from(index)
                                .checked_mul(2)
                                .ok_or(StoreError::Malformed)?,
                        )
                        .ok_or(StoreError::Malformed)?;
                    let cell = Self::cell_at(area, base, offset)?;
                    let child = read_u32(cell, 0)?;
                    let (_key, _key_len) = read_varint(cell, 4)?;
                    self.walk_page(child, depth + 1, visited, rows)?;
                }
                let right = read_u32(area, 8)?;
                self.walk_page(right, depth + 1, visited, rows)?;
            }
            0x0D => {
                let cells = read_u16(area, 3)?;
                for index in 0..cells {
                    let offset = 8_usize
                        .checked_add(
                            usize::from(index)
                                .checked_mul(2)
                                .ok_or(StoreError::Malformed)?,
                        )
                        .ok_or(StoreError::Malformed)?;
                    let cell = Self::cell_at(area, base, offset)?;
                    rows.push(self.parse_leaf_cell(cell)?);
                }
            }
            _ => return Err(StoreError::Malformed),
        }
        Ok(())
    }

    /// Resolve one cell pointer. Pointers are page-relative, so page 1
    /// pointers clear the 100-byte database header before indexing the area.
    fn cell_at(area: &[u8], base: usize, offset: usize) -> Result<&[u8], StoreError> {
        let pointer = usize::from(read_u16(area, offset)?);
        let start = pointer.checked_sub(base).ok_or(StoreError::Malformed)?;
        area.get(start..).ok_or(StoreError::Malformed)
    }

    fn parse_leaf_cell(&self, cell: &[u8]) -> Result<TableRow, StoreError> {
        let (payload_len, header_len) = read_varint(cell, 0)?;
        let (rowid, rowid_len) = read_varint(cell, header_len)?;
        let total = usize::try_from(payload_len).map_err(|_| StoreError::Malformed)?;
        let header = header_len
            .checked_add(rowid_len)
            .ok_or(StoreError::Malformed)?;
        let usable = u64::try_from(self.usable).map_err(|_| StoreError::Malformed)?;
        let max_local = usable.checked_sub(35).ok_or(StoreError::Malformed)?;
        let payload = if payload_len <= max_local {
            let end = header.checked_add(total).ok_or(StoreError::Malformed)?;
            cell.get(header..end).ok_or(StoreError::Malformed)?.to_vec()
        } else {
            let minimum = usable
                .checked_sub(12)
                .and_then(|value| value.checked_mul(32))
                .map(|value| value / 255)
                .and_then(|value| value.checked_sub(23))
                .ok_or(StoreError::Malformed)?;
            let step = usable.checked_sub(4).ok_or(StoreError::Malformed)?;
            let spread = payload_len
                .checked_sub(minimum)
                .ok_or(StoreError::Malformed)?;
            let local = minimum
                .checked_add(spread % step)
                .ok_or(StoreError::Malformed)?;
            let local = usize::try_from(local).map_err(|_| StoreError::Malformed)?;
            let local_end = header.checked_add(local).ok_or(StoreError::Malformed)?;
            let mut payload = cell
                .get(header..local_end)
                .ok_or(StoreError::Malformed)?
                .to_vec();
            let first = read_u32(cell, local_end)?;
            let mut remaining = total.checked_sub(local).ok_or(StoreError::Malformed)?;
            self.read_overflow_chain(first, &mut remaining, &mut payload)?;
            if remaining != 0 {
                return Err(StoreError::Malformed);
            }
            payload
        };
        Ok(TableRow {
            rowid: i64::from_be_bytes(rowid.to_be_bytes()),
            values: parse_record(&payload)?,
        })
    }

    fn read_overflow_chain(
        &self,
        first: u32,
        remaining: &mut usize,
        payload: &mut Vec<u8>,
    ) -> Result<(), StoreError> {
        let mut next = first;
        let mut hops = 0_usize;
        let step = self.usable.checked_sub(4).ok_or(StoreError::Malformed)?;
        while *remaining > 0 {
            if next == 0 || hops > self.pages.len() {
                return Err(StoreError::Malformed);
            }
            hops = hops.checked_add(1).ok_or(StoreError::Malformed)?;
            let page = self.page_bytes(next)?;
            next = read_u32(page, 0)?;
            let take = step.min(*remaining);
            let end = take.checked_add(4).ok_or(StoreError::Malformed)?;
            payload.extend_from_slice(page.get(4..end).ok_or(StoreError::Malformed)?);
            *remaining = remaining.checked_sub(take).ok_or(StoreError::Malformed)?;
        }
        Ok(())
    }
}

/// Overlay committed WAL frames onto the page image.
///
/// Only frames at or before the last commit marker are applied; a torn
/// trailing frame is ignored. The page vector may grow: WAL frames can extend
/// the database, but never beyond the frames actually present.
fn apply_wal(pages: &mut Vec<Vec<u8>>, page_size: usize, wal: &[u8]) -> Result<(), StoreError> {
    if wal.is_empty() {
        return Ok(());
    }
    if wal.len() < 32 {
        return Err(StoreError::Malformed);
    }
    if !matches!(read_u32(wal, 0)?, 0x377F_0682 | 0x377F_0683)
        || read_u32(wal, 4)? != 3_007_000
        || usize::try_from(read_u32(wal, 8)?).map_err(|_| StoreError::Malformed)? != page_size
    {
        return Err(StoreError::Malformed);
    }
    let salt_one = read_u32(wal, 16)?;
    let salt_two = read_u32(wal, 20)?;
    let frame_len = page_size.checked_add(24).ok_or(StoreError::Malformed)?;
    let mut frames: Vec<(u32, u32, usize)> = Vec::new();
    let mut at = 32_usize;
    while at
        .checked_add(frame_len)
        .is_some_and(|end| end <= wal.len())
    {
        if read_u32(wal, at + 8)? != salt_one || read_u32(wal, at + 12)? != salt_two {
            return Err(StoreError::Malformed);
        }
        let page_no = read_u32(wal, at)?;
        if page_no == 0 {
            return Err(StoreError::Malformed);
        }
        frames.push((page_no, read_u32(wal, at + 4)?, at + 24));
        at = at.checked_add(frame_len).ok_or(StoreError::Malformed)?;
    }
    let mut committed = 0_usize;
    for (index, frame) in frames.iter().enumerate() {
        if frame.1 != 0 {
            committed = index.checked_add(1).ok_or(StoreError::Malformed)?;
        }
    }
    let max_pages = pages
        .len()
        .checked_add(frames.len())
        .ok_or(StoreError::Malformed)?;
    for (page_no, _, offset) in frames.iter().take(committed) {
        let index = usize::try_from(page_no.checked_sub(1).ok_or(StoreError::Malformed)?)
            .map_err(|_| StoreError::Malformed)?;
        if index >= max_pages {
            return Err(StoreError::Malformed);
        }
        while pages.len() <= index {
            pages.push(vec![0_u8; page_size]);
        }
        let end = offset.checked_add(page_size).ok_or(StoreError::Malformed)?;
        let image = wal.get(*offset..end).ok_or(StoreError::Malformed)?;
        let slot = pages.get_mut(index).ok_or(StoreError::Malformed)?;
        slot.copy_from_slice(image);
    }
    Ok(())
}

/// One decoded rowid-table row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TableRow {
    /// Row identifier; walks yield rows in ascending order.
    pub(crate) rowid: i64,
    /// Column values in table-definition order.
    pub(crate) values: Vec<SqlValue>,
}

/// Minimal `SQLite` value surface: enough for text credentials.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SqlValue {
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

impl SqlValue {
    /// Borrow the value when it is text.
    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    /// Copy the value when it is an integer.
    pub(crate) fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            _ => None,
        }
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, StoreError> {
    let end = offset.checked_add(2).ok_or(StoreError::Malformed)?;
    let pair: [u8; 2] = bytes
        .get(offset..end)
        .ok_or(StoreError::Malformed)?
        .try_into()
        .map_err(|_| StoreError::Malformed)?;
    Ok(u16::from_be_bytes(pair))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, StoreError> {
    let end = offset.checked_add(4).ok_or(StoreError::Malformed)?;
    let quad: [u8; 4] = bytes
        .get(offset..end)
        .ok_or(StoreError::Malformed)?
        .try_into()
        .map_err(|_| StoreError::Malformed)?;
    Ok(u32::from_be_bytes(quad))
}

/// Decode a `SQLite` varint, returning the value and bytes consumed.
fn read_varint(bytes: &[u8], offset: usize) -> Result<(u64, usize), StoreError> {
    let mut value: u64 = 0;
    for index in 0..8_usize {
        let at = offset.checked_add(index).ok_or(StoreError::Malformed)?;
        let byte = bytes.get(at).copied().ok_or(StoreError::Malformed)?;
        value = (value << 7) | u64::from(byte & 0x7F);
        if byte & 0x80 == 0 {
            return Ok((value, index.checked_add(1).ok_or(StoreError::Malformed)?));
        }
    }
    let at = offset.checked_add(8).ok_or(StoreError::Malformed)?;
    let byte = bytes.get(at).copied().ok_or(StoreError::Malformed)?;
    Ok(((value << 8) | u64::from(byte), 9))
}

/// Decode one record payload into column values.
fn parse_record(payload: &[u8]) -> Result<Vec<SqlValue>, StoreError> {
    let (header_len, mut at) = read_varint(payload, 0)?;
    let header_len = usize::try_from(header_len).map_err(|_| StoreError::Malformed)?;
    if header_len < at || header_len > payload.len() {
        return Err(StoreError::Malformed);
    }
    let mut serials = Vec::new();
    while at < header_len {
        let (serial, used) = read_varint(payload, at)?;
        serials.push(serial);
        at = at.checked_add(used).ok_or(StoreError::Malformed)?;
    }
    let mut values = Vec::with_capacity(serials.len());
    let mut body = header_len;
    for serial in serials {
        let (value, used) = read_serial_value(payload, body, serial)?;
        values.push(value);
        body = body.checked_add(used).ok_or(StoreError::Malformed)?;
    }
    if body != payload.len() {
        return Err(StoreError::Malformed);
    }
    Ok(values)
}

fn read_serial_value(
    payload: &[u8],
    at: usize,
    serial: u64,
) -> Result<(SqlValue, usize), StoreError> {
    match serial {
        0 => Ok((SqlValue::Null, 0)),
        1..=4 => {
            let len = usize::try_from(serial).map_err(|_| StoreError::Malformed)?;
            Ok((SqlValue::Int(read_int(payload, at, len)?), len))
        }
        5 => Ok((SqlValue::Int(read_int(payload, at, 6)?), 6)),
        6 => Ok((SqlValue::Int(read_int(payload, at, 8)?), 8)),
        7 => {
            let end = at.checked_add(8).ok_or(StoreError::Malformed)?;
            let octet: [u8; 8] = payload
                .get(at..end)
                .ok_or(StoreError::Malformed)?
                .try_into()
                .map_err(|_| StoreError::Malformed)?;
            Ok((
                SqlValue::Float(f64::from_bits(u64::from_be_bytes(octet))),
                8,
            ))
        }
        8 => Ok((SqlValue::Int(0), 0)),
        9 => Ok((SqlValue::Int(1), 0)),
        10 | 11 => Err(StoreError::Malformed),
        even if even >= 12 && even % 2 == 0 => {
            let len = usize::try_from(even.checked_sub(12).ok_or(StoreError::Malformed)? / 2)
                .map_err(|_| StoreError::Malformed)?;
            let end = at.checked_add(len).ok_or(StoreError::Malformed)?;
            let blob = payload.get(at..end).ok_or(StoreError::Malformed)?.to_vec();
            Ok((SqlValue::Blob(blob), len))
        }
        rest => {
            if rest < 13 || rest % 2 == 0 {
                return Err(StoreError::Malformed);
            }
            let len = usize::try_from(rest.checked_sub(13).ok_or(StoreError::Malformed)? / 2)
                .map_err(|_| StoreError::Malformed)?;
            let end = at.checked_add(len).ok_or(StoreError::Malformed)?;
            let text = payload.get(at..end).ok_or(StoreError::Malformed)?;
            let text = str::from_utf8(text)
                .map_err(|_| StoreError::Malformed)?
                .to_owned();
            Ok((SqlValue::Text(text), len))
        }
    }
}

/// Read a big-endian signed integer of `len` bytes with sign extension.
fn read_int(payload: &[u8], at: usize, len: usize) -> Result<i64, StoreError> {
    if !(1..=8).contains(&len) {
        return Err(StoreError::Malformed);
    }
    let end = at.checked_add(len).ok_or(StoreError::Malformed)?;
    let bytes = payload.get(at..end).ok_or(StoreError::Malformed)?;
    let negative = bytes.first().is_some_and(|byte| byte & 0x80 != 0);
    let mut value: i64 = if negative { -1 } else { 0 };
    for byte in bytes {
        value = (value << 8) | i64::from(*byte);
    }
    Ok(value)
}

/// Locate table `name` in decoded `sqlite_schema` rows.
///
/// Returns the root page and the `CREATE TABLE` text.
pub(crate) fn find_table_schema(schema: &[TableRow], name: &str) -> Option<(u32, String)> {
    schema.iter().find_map(|row| {
        let kind = row.values.first()?.as_text()?;
        let found = row.values.get(1)?.as_text()?;
        let root = row.values.get(3)?.as_int()?;
        let sql = row.values.get(4)?.as_text()?;
        (kind == "table" && found == name)
            .then(|| u32::try_from(root).ok().filter(|page| *page > 0))
            .flatten()
            .map(|page| (page, sql.to_owned()))
    })
}

/// Split a `CREATE TABLE` column-definition list at top-level commas.
pub(crate) fn parse_column_names(sql: &str) -> Result<Vec<String>, StoreError> {
    let start = sql.find('(').ok_or(StoreError::Malformed)?;
    let end = sql.rfind(')').ok_or(StoreError::Malformed)?;
    let inner = sql
        .get(start.checked_add(1).ok_or(StoreError::Malformed)?..end)
        .ok_or(StoreError::Malformed)?;
    let mut columns = Vec::new();
    for item in split_top_level(inner) {
        let token = item
            .split_whitespace()
            .next()
            .ok_or(StoreError::Malformed)?;
        let name = token.trim_matches(|char| matches!(char, '"' | '\'' | '`' | '[' | ']'));
        if name.is_empty() {
            return Err(StoreError::Malformed);
        }
        if matches!(
            name.to_uppercase().as_str(),
            "CONSTRAINT" | "PRIMARY" | "FOREIGN" | "UNIQUE" | "CHECK"
        ) {
            continue;
        }
        columns.push(name.to_owned());
    }
    if columns.is_empty() {
        return Err(StoreError::Malformed);
    }
    Ok(columns)
}

/// Split `inner` on commas, ignoring commas inside quotes and parentheses.
fn split_top_level(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0_usize;
    let mut quote: Option<char> = None;
    let mut start = 0_usize;
    let mut bracket = false;
    for (index, char) in inner.char_indices() {
        if let Some(open) = quote {
            if char == open {
                quote = None;
            }
            continue;
        }
        if bracket {
            if char == ']' {
                bracket = false;
            }
            continue;
        }
        match char {
            '\'' | '"' | '`' => quote = Some(char),
            '[' => bracket = true,
            '(' => depth = depth.saturating_add(1),
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if let Some(part) = inner.get(start..index) {
                    parts.push(part);
                }
                start = index.saturating_add(1);
            }
            _ => {}
        }
    }
    if let Some(part) = inner.get(start..) {
        parts.push(part);
    }
    parts
}

/// First column index whose name matches `wanted` (case-insensitive).
pub(crate) fn find_column(columns: &[String], wanted: &[&str]) -> Option<usize> {
    wanted.iter().find_map(|name| {
        columns
            .iter()
            .position(|column| column.eq_ignore_ascii_case(name))
    })
}

/// Usable text of a value: trimmed, non-blank text or UTF-8 blob.
pub(crate) fn text_value(value: &SqlValue) -> Option<String> {
    match value {
        SqlValue::Text(text) => (!text.trim().is_empty()).then(|| text.trim().to_owned()),
        SqlValue::Blob(bytes) => str::from_utf8(bytes)
            .ok()
            .and_then(|text| (!text.trim().is_empty()).then(|| text.trim().to_owned())),
        SqlValue::Null | SqlValue::Int(_) | SqlValue::Float(_) => None,
    }
}

/// Synthetic database fixtures for the store enumerator tests.
///
/// Builds minimal but structurally valid `SQLite` images: a schema page, one
/// table b-tree, optional overflow chains, and optional WAL frames. Leaf
/// cells are laid sorted by rowid, matching the invariant real writers keep.
#[cfg(test)]
pub(crate) mod fixture {
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
}

#[cfg(test)]
mod tests {
    use super::fixture::{
        Cell, PAGE_SIZE, Value, database, db_header, encode_varint, interior_page, leaf_page,
        wal_image,
    };
    use super::{
        SqlValue, SqliteImage, find_column, find_table_schema, parse_column_names, text_value,
    };
    use crate::accounts::stores::StoreError;

    #[test]
    fn reads_leaf_table_in_rowid_order() {
        let db = database(
            "CREATE TABLE t (a TEXT, b INTEGER, c BLOB, d REAL, e TEXT)",
            2,
            &[
                Cell::row(
                    9,
                    vec![
                        Value::Text("nine".into()),
                        Value::Int(-42),
                        Value::Blob(vec![1, 2, 3]),
                        Value::Float(1.5),
                        Value::Null,
                    ],
                ),
                Cell::row(
                    3,
                    vec![
                        Value::Text("three".into()),
                        Value::Int(0),
                        Value::Blob(vec![]),
                        Value::Float(0.0),
                        Value::Text("x".into()),
                    ],
                ),
            ],
            false,
        );
        let image = SqliteImage::load(&db, None).unwrap();
        let schema = image.walk_table(1).unwrap();
        assert_eq!(schema.len(), 1);
        let (root, sql) = find_table_schema(&schema, "t").unwrap();
        assert_eq!(root, 2);
        assert_eq!(
            parse_column_names(&sql).unwrap(),
            vec!["a", "b", "c", "d", "e"]
        );
        let rows = image.walk_table(root).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].rowid, 3);
        assert_eq!(rows[0].values[0], SqlValue::Text("three".into()));
        assert_eq!(rows[0].values[1], SqlValue::Int(0));
        assert_eq!(rows[0].values[2], SqlValue::Blob(vec![]));
        assert_eq!(rows[1].rowid, 9);
        assert_eq!(rows[1].values[1], SqlValue::Int(-42));
        assert_eq!(rows[1].values[2], SqlValue::Blob(vec![1, 2, 3]));
        assert_eq!(rows[1].values[3], SqlValue::Float(1.5));
        assert_eq!(rows[1].values[4], SqlValue::Null);
    }

    #[test]
    fn varint_codec_round_trips_boundaries() {
        for value in [
            0,
            1,
            127,
            128,
            240,
            16_383,
            16_384,
            u64::from(u32::MAX),
            1 << 56,
            u64::MAX,
        ] {
            let encoded = encode_varint(value);
            let (decoded, used) = super::read_varint(&encoded, 0).unwrap();
            assert_eq!(decoded, value, "value: {value}");
            assert_eq!(used, encoded.len(), "value: {value}");
            let padded = [vec![0xAA_u8; 3], encoded.clone()].concat();
            let (shifted, _) = super::read_varint(&padded, 3).unwrap();
            assert_eq!(shifted, value, "value: {value}");
        }
        assert_eq!(encode_varint(u64::MAX).len(), 9);
        assert_eq!(
            super::read_varint(&[0x80_u8; 8], 0).unwrap_err(),
            StoreError::Malformed
        );
    }

    #[test]
    fn walks_interior_root() {
        let schema = Cell::row(
            1,
            vec![
                Value::Text("table".into()),
                Value::Text("t".into()),
                Value::Text("t".into()),
                Value::Int(2),
                Value::Text("CREATE TABLE t (a TEXT)".into()),
            ],
        );
        let mut page_one = leaf_page(&[schema], 100);
        page_one[..100].copy_from_slice(&db_header(false));
        let left = leaf_page(&[Cell::row(1, vec![Value::Text("a".into())])], 0);
        let right = leaf_page(&[Cell::row(5, vec![Value::Text("b".into())])], 0);
        let root = interior_page(&[(3, 1)], 4);
        let mut db = page_one;
        db.extend(root);
        db.extend(left);
        db.extend(right);
        let image = SqliteImage::load(&db, None).unwrap();
        let rows = image.walk_table(2).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].values[0], SqlValue::Text("a".into()));
        assert_eq!(rows[1].values[0], SqlValue::Text("b".into()));
    }

    #[test]
    fn reads_overflow_payload() {
        let big = "v".repeat(2000);
        let db = database(
            "CREATE TABLE t (a TEXT)",
            2,
            &[Cell::row(1, vec![Value::Text(big.clone())])],
            false,
        );
        assert!(db.len() > 2 * PAGE_SIZE, "payload must spill to overflow");
        let image = SqliteImage::load(&db, None).unwrap();
        let rows = image.walk_table(2).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values[0], SqlValue::Text(big));
    }

    #[test]
    fn wal_committed_frames_overlay_and_tail_is_ignored() {
        let base = database(
            "CREATE TABLE t (a TEXT)",
            2,
            &[Cell::row(1, vec![Value::Text("stale".into())])],
            true,
        );
        let fresh = leaf_page(&[Cell::row(1, vec![Value::Text("fresh".into())])], 0);
        let uncommitted = leaf_page(&[Cell::row(1, vec![Value::Text("nope".into())])], 0);
        let mut wal = wal_image(&[(2, 1, fresh), (2, 0, uncommitted)]);
        wal.extend([0xFF_u8; 7]);
        let image = SqliteImage::load(&base, Some(&wal)).unwrap();
        let rows = image.walk_table(2).unwrap();
        assert_eq!(rows[0].values[0], SqlValue::Text("fresh".into()));
    }

    #[test]
    fn wal_ignored_without_wal_header() {
        let base = database(
            "CREATE TABLE t (a TEXT)",
            2,
            &[Cell::row(1, vec![Value::Text("base".into())])],
            false,
        );
        let other = leaf_page(&[Cell::row(1, vec![Value::Text("other".into())])], 0);
        let wal = wal_image(&[(2, 1, other)]);
        let image = SqliteImage::load(&base, Some(&wal)).unwrap();
        let rows = image.walk_table(2).unwrap();
        assert_eq!(rows[0].values[0], SqlValue::Text("base".into()));
    }

    #[test]
    fn wal_may_extend_the_database() {
        let schema = Cell::row(
            1,
            vec![
                Value::Text("table".into()),
                Value::Text("t".into()),
                Value::Text("t".into()),
                Value::Int(3),
                Value::Text("CREATE TABLE t (a TEXT)".into()),
            ],
        );
        let mut page_one = leaf_page(&[schema], 100);
        page_one[..100].copy_from_slice(&db_header(true));
        let mut db = page_one;
        db.extend(leaf_page(&[], 0));
        let grown = leaf_page(&[Cell::row(1, vec![Value::Text("grown".into())])], 0);
        let wal = wal_image(&[(3, 1, grown)]);
        let image = SqliteImage::load(&db, Some(&wal)).unwrap();
        let rows = image.walk_table(3).unwrap();
        assert_eq!(rows[0].values[0], SqlValue::Text("grown".into()));
    }

    #[test]
    fn rejects_malformed_images() {
        let good = database("CREATE TABLE t (a TEXT)", 2, &[], false);
        let mut bad_magic = good.clone();
        bad_magic[0] = b'X';
        let mut bad_page_size = good.clone();
        bad_page_size[16..18].copy_from_slice(&513_u16.to_be_bytes());
        let mut bad_type = good.clone();
        bad_type[100] = 0x02;
        let mut dangling = good.clone();
        dangling[100] = 0x05;
        let truncated_cell = good[..good.len() - 3].to_vec();
        for image in [
            bad_magic,
            good[..99].to_vec(),
            bad_page_size,
            bad_type,
            dangling,
            truncated_cell,
            vec![0_u8; 512],
        ] {
            let walked =
                SqliteImage::load(&image, None).and_then(|loaded| loaded.walk_table(1).map(|_| ()));
            assert_eq!(walked.unwrap_err(), StoreError::Malformed);
        }
        let wal_base = database(
            "CREATE TABLE t (a TEXT)",
            2,
            &[Cell::row(1, vec![Value::Text("v".into())])],
            true,
        );
        let page = leaf_page(&[Cell::row(1, vec![Value::Text("w".into())])], 0);
        let good_wal = wal_image(&[(2, 1, page.clone())]);
        let mut bad_wal_magic = good_wal.clone();
        bad_wal_magic[0] = 0;
        let mut bad_wal_version = good_wal.clone();
        bad_wal_version[4..8].copy_from_slice(&1_u32.to_be_bytes());
        let mut bad_wal_salt = good_wal.clone();
        let salt_at = 32 + 8;
        bad_wal_salt[salt_at] ^= 0xFF;
        let zero_page = wal_image(&[(0, 1, page.clone())]);
        let far_page = wal_image(&[(99_999, 1, page)]);
        for wal in [
            bad_wal_magic,
            bad_wal_version,
            bad_wal_salt,
            zero_page,
            far_page,
        ] {
            assert_eq!(
                SqliteImage::load(&wal_base, Some(&wal)).unwrap_err(),
                StoreError::Malformed
            );
        }
        assert_eq!(
            SqliteImage::load(&wal_base, Some(&[1_u8; 31])).unwrap_err(),
            StoreError::Malformed
        );
    }

    #[test]
    fn column_definition_parsing() {
        let columns = parse_column_names(
            "CREATE TABLE t ([id] INTEGER PRIMARY KEY, \"Value\" TEXT NOT NULL, 'we,ird' BLOB, CHECK (id > (1)), CONSTRAINT x UNIQUE (id))",
        )
        .unwrap();
        assert_eq!(columns, vec!["id", "Value", "we,ird"]);
        assert_eq!(find_column(&columns, &["value"]), Some(1));
        assert_eq!(find_column(&columns, &["missing"]), None);
        for sql in [
            "CREATE TABLE t",
            "CREATE TABLE t ()",
            "CREATE TABLE t (,,,)",
        ] {
            assert_eq!(
                parse_column_names(sql).unwrap_err(),
                StoreError::Malformed,
                "{sql}"
            );
        }
    }

    #[test]
    fn text_value_trims_and_skips() {
        assert_eq!(
            text_value(&SqlValue::Text("  padded  ".into())),
            Some("padded".to_owned())
        );
        assert_eq!(text_value(&SqlValue::Text("   ".into())), None);
        assert_eq!(
            text_value(&SqlValue::Blob(b"blob".to_vec())),
            Some("blob".to_owned())
        );
        assert_eq!(text_value(&SqlValue::Blob(vec![0xFF])), None);
        for value in [SqlValue::Null, SqlValue::Int(7), SqlValue::Float(1.0)] {
            assert_eq!(text_value(&value), None);
        }
    }
}
