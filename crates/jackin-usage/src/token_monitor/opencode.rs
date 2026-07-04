// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `SQLite` reader for `OpenCode` token usage, via the workspace-standard
//! `turso` client (never `rusqlite`). Reads `opencode.db`'s `message` table
//! incrementally by `rowid`.

use super::TokenSession;

const DB_PATH: &str = "/home/agent/.local/share/opencode/opencode.db";

pub(crate) async fn poll_session(session: &mut TokenSession) -> bool {
    if !std::path::Path::new(DB_PATH).exists() {
        return false;
    }

    let Ok(db) = turso::Builder::new_local(DB_PATH).build().await else {
        crate::cdebug!("token monitor: opencode db open failed: {DB_PATH:?}");
        return false;
    };
    let Ok(conn) = db.connect() else {
        crate::cdebug!("token monitor: opencode db connect failed");
        return false;
    };

    let query = "SELECT rowid, input, output, cost FROM message WHERE rowid > ? ORDER BY rowid ASC LIMIT 1000";
    let Ok(mut rows) = conn.query(query, turso::params![session.last_rowid]).await else {
        // Pre-v1.2 OpenCode stored messages as JSON files, not SQLite; a missing
        // `message` table lands here. Reading that legacy format is not yet
        // implemented — treat as "no new data".
        crate::cdebug!("token monitor: opencode db schema mismatch, query failed");
        return false;
    };

    let mut changed = false;
    loop {
        let row = match rows.next().await {
            Ok(Some(row)) => row,
            Ok(None) => break,
            Err(e) => {
                crate::cdebug!("token monitor: opencode row read failed: {e}");
                break;
            }
        };
        let (Ok(rowid), Ok(input), Ok(output)) =
            (row.get::<i64>(0), row.get::<i64>(1), row.get::<i64>(2))
        else {
            continue;
        };
        let cost = row.get::<f64>(3).ok();

        session.totals.input_tokens += u64::try_from(input).unwrap_or(0);
        session.totals.output_tokens += u64::try_from(output).unwrap_or(0);
        if let Some(c) = cost {
            session.totals.cost_usd = Some(session.totals.cost_usd.unwrap_or(0.0) + c);
        }
        session.last_rowid = rowid;
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests;
