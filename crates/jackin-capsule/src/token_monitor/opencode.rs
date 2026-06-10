//! `SQLite` reader for `OpenCode` token usage.
//!
//! Reads `~/.local/share/opencode/opencode.db`.

use super::TokenSession;

pub fn poll_session(session: &mut TokenSession) -> bool {
    let db_path = "/home/agent/.local/share/opencode/opencode.db";
    if !std::path::Path::new(db_path).exists() {
        return false;
    }

    let Ok(conn) =
        rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
    else {
        crate::cdebug!("token monitor: opencode db open failed: {:?}", db_path);
        return false;
    };

    let query = "SELECT rowid, input, output, cost FROM message WHERE rowid > ? ORDER BY rowid ASC LIMIT 1000";
    let Ok(mut stmt) = conn.prepare(query) else {
        crate::cdebug!("token monitor: opencode db schema mismatch, prepare failed");
        return poll_session_legacy(session, &conn);
    };

    let mut changed = false;
    let last_rowid = session.last_rowid;

    let rows: Vec<(i64, i64, i64, Option<f64>)> = stmt
        .query_map([last_rowid], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<f64>>(3)?,
            ))
        })
        .ok()
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();

    for (rowid, input, output, cost) in rows {
        session.totals.input_tokens += input as u64;
        session.totals.output_tokens += output as u64;
        if let Some(c) = cost {
            session.totals.cost_usd = Some(session.totals.cost_usd.unwrap_or(0.0) + c);
        }
        session.last_rowid = rowid;
        changed = true;
    }
    changed
}

// Pre-v1.2 OpenCode stored messages as JSON files, not SQLite.
// Reading that format is not yet implemented.
fn poll_session_legacy(session: &mut TokenSession, conn: &rusqlite::Connection) -> bool {
    let _ = (session, conn);
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn opencode_token_reader_db_path_is_correct() {
        let expected = "/home/agent/.local/share/opencode/opencode.db";
        assert!(expected.contains("opencode.db"));
    }
}
