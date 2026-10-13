// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::tests::{
    Cell, PAGE_SIZE, Value, database, db_header, encode_varint, interior_page, leaf_page, wal_image,
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
