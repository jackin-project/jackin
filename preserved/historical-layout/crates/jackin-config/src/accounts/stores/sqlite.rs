// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Minimal read-only `SQLite` parser shared by the store enumerators.
//!
//! Scoped `ENGINEERING.md` waiver of the turso-only rule: turso 0.7.2 has
//! no read-only/immutable open (`Builder::build` takes no flags), so opening
//! the live third-party databases (`.omp`, `opencode`) through turso would
//! open them read-write, risking `-shm`/`-journal` side effects and locks
//! against another app's live state. The readers therefore parse the file
//! format directly with no third-party database code. Re-check on turso
//! upgrade. The reader understands rowid table b-trees (interior pages,
//! leaf pages, and overflow chains) plus enough of the `sqlite_schema`
//! table and `CREATE TABLE` definitions to locate one table and map its
//! columns.
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

#[cfg(test)]
mod tests;
