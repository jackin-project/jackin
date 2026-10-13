// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const SCHEMA: &str = "CREATE TABLE credential (id INTEGER PRIMARY KEY, integration_id TEXT, label TEXT, value TEXT, connector_id TEXT, method_id TEXT, active INTEGER)";

pub(super) fn credential(cells: &[Cell], wal_mode: bool) -> Vec<u8> {
    database(SCHEMA, 2, cells, wal_mode)
}

pub(super) fn row(
    rowid: u64,
    integration: &str,
    label: &str,
    value: Value,
    method: &str,
    active: Value,
) -> Cell {
    Cell::row(
        rowid,
        vec![
            Value::Int(rowid as i64),
            Value::Text(integration.into()),
            Value::Text(label.into()),
            value,
            Value::Text(String::new()),
            Value::Text(method.into()),
            active,
        ],
    )
}
