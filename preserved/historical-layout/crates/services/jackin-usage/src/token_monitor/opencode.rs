//! `SQLite` reader for `OpenCode` token usage, via the crate's store-backend
//! chokepoint (workspace-standard turso client, never `rusqlite`). Reads
//! `opencode.db`'s `message` table incrementally by `rowid`.

use super::{PollStatus, TokenSession};
use crate::store_backend::{self, DbOperation, connect_local, params};
use jackin_telemetry::ResultTelemetryExt as _;

const DB_PATH: &str = "/home/agent/.local/share/opencode/opencode.db";

pub(crate) async fn poll_session(session: &mut TokenSession) -> PollStatus {
    if !std::path::Path::new(DB_PATH).exists() {
        return PollStatus::Unchanged;
    }

    let Ok(conn) = connect_local(DB_PATH)
        .await
        .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError)
    else {
        return PollStatus::Degraded;
    };

    let query = "SELECT rowid, input, output, cost FROM message WHERE rowid > ? ORDER BY rowid ASC LIMIT 1000";
    let Ok(mut rows) = store_backend::operation(
        DbOperation::Select,
        conn.query(query, params![session.last_rowid]),
    )
    .await
    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError) else {
        // Pre-v1.2 OpenCode stored messages as JSON files, not SQLite; a missing
        // `message` table lands here. Reading that legacy format is not yet
        // implemented — treat as "no new data".
        return PollStatus::Degraded;
    };

    let mut changed = false;
    let mut degraded = false;
    let mut degradation_recorded = false;
    loop {
        let row = match rows
            .next()
            .await
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError)
        {
            Ok(Some(row)) => row,
            Ok(None) => break,
            Err(_) => {
                degraded = true;
                degradation_recorded = true;
                break;
            }
        };
        let (Ok(rowid), Ok(input), Ok(output)) =
            (row.get::<i64>(0), row.get::<i64>(1), row.get::<i64>(2))
        else {
            degraded = true;
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
    if degraded {
        if !degradation_recorded {
            let _error =
                jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::DbError);
        }
        PollStatus::Degraded
    } else {
        PollStatus::from_changed(changed)
    }
}

#[cfg(test)]
mod tests;
