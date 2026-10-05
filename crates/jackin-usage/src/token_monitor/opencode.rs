//! `SQLite` reader for `OpenCode` token usage, via the crate's store-backend
//! chokepoint (workspace-standard turso client, never `rusqlite`). Reads
//! `opencode.db`'s `message` table incrementally by `rowid`.

use super::{PollStatus, TokenSession};
use crate::store_backend;
use jackin_telemetry::ResultTelemetryExt as _;

const DB_PATH: &str = "/home/agent/.local/share/opencode/opencode.db";

pub(crate) async fn poll_session(session: &mut TokenSession) -> PollStatus {
    poll_session_at(session, DB_PATH)
}

fn poll_session_at(session: &mut TokenSession, path: &str) -> PollStatus {
    let result = store_backend::read_local(path, |reader| {
        let query = "SELECT rowid, input, output, cost FROM message WHERE rowid > ? ORDER BY rowid ASC LIMIT 1000";
        let rows = reader.query_i64(query, session.last_rowid)?;
        let mut changed = false;
        let mut degraded = false;
        for row in rows {
            let (Ok(rowid), Ok(input), Ok(output)) = (
                row.i64(0, "rowid"),
                row.i64(1, "input"),
                row.i64(2, "output"),
            ) else {
                degraded = true;
                continue;
            };
            let cost = row.numeric_f64(3, "cost").ok();

            session.totals.input_tokens += u64::try_from(input).unwrap_or(0);
            session.totals.output_tokens += u64::try_from(output).unwrap_or(0);
            if let Some(c) = cost {
                session.totals.cost_usd = Some(session.totals.cost_usd.unwrap_or(0.0) + c);
            }
            session.last_rowid = rowid;
            changed = true;
        }
        if degraded {
            let _error =
                jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::DbError);
            Ok(PollStatus::Degraded)
        } else {
            Ok(PollStatus::from_changed(changed))
        }
    });
    match result.record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError) {
        Ok(Some(status)) => status,
        Ok(None) => PollStatus::Unchanged,
        Err(_) => PollStatus::Degraded,
    }
}

#[cfg(test)]
mod tests;
