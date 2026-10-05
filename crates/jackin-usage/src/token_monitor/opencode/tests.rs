// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{DB_PATH, PollStatus, TokenSession, poll_session_at};
use crate::store_backend::connect_local;
use jackin_core::Agent;

#[test]
fn opencode_token_reader_db_path_is_correct() {
    assert!(DB_PATH.contains("opencode.db"));
}

#[test]
fn absent_database_stays_absent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opencode.db");
    let mut session = TokenSession::new(Agent::Opencode);

    assert_eq!(
        poll_session_at(&mut session, path.to_str().unwrap()),
        PollStatus::Unchanged
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    assert_eq!(session.last_rowid, 0);
}

#[tokio::test]
async fn committed_wal_is_read_without_rewriting_provider_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opencode.db");
    let path = path.to_str().unwrap();
    let wal_path = format!("{path}-wal");
    let writer = connect_local(path).await.unwrap();
    writer
        .execute("CREATE TABLE message (input INTEGER, output INTEGER, cost)", ())
        .await
        .unwrap();
    writer
        .execute(
            "INSERT INTO message VALUES (12, 4, 0.25), (-3, 6, 1), (7, 2, NULL)",
            (),
        )
        .await
        .unwrap();
    // Save a quiescent, committed DB/WAL pair before writer shutdown can
    // checkpoint it. Restore only these test fixture files after shutdown.
    let (database, wal) = crate::store_backend::source_bytes_for_test(path);
    let wal = wal.unwrap();
    assert!(!wal.is_empty());
    let mut session = TokenSession::new(Agent::Opencode);
    assert_eq!(poll_session_at(&mut session, path), PollStatus::Degraded);
    assert_eq!(session.last_rowid, 0);
    assert_eq!(
        crate::store_backend::source_bytes_for_test(path),
        (database.clone(), Some(wal.clone()))
    );
    drop(writer);
    std::fs::write(path, &database).unwrap();
    std::fs::write(&wal_path, &wal).unwrap();

    assert_eq!(poll_session_at(&mut session, path), PollStatus::Changed);
    assert_eq!(session.totals.input_tokens, 19);
    assert_eq!(session.totals.output_tokens, 12);
    assert_eq!(session.totals.cost_usd, Some(1.25));
    assert_eq!(session.last_rowid, 3);
    assert_eq!(poll_session_at(&mut session, path), PollStatus::Unchanged);
    assert_eq!(session.totals.input_tokens, 19);
    assert_eq!(session.totals.output_tokens, 12);
    assert_eq!(session.totals.cost_usd, Some(1.25));
    assert_eq!(std::fs::read(path).unwrap(), database);
    assert_eq!(std::fs::read(&wal_path).unwrap(), wal);
    let mut files = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files, ["opencode.db", "opencode.db-wal"]);
}

#[tokio::test]
async fn invalid_token_rows_degrade_without_changing_valid_row_ingestion() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opencode.db");
    let path = path.to_str().unwrap();
    let writer = connect_local(path).await.unwrap();
    writer
        .execute("CREATE TABLE message (input, output, cost)", ())
        .await
        .unwrap();
    writer
        .execute("INSERT INTO message VALUES ('bad', 5, 2), (9, 3, 'unknown')", ())
        .await
        .unwrap();
    drop(writer);

    let mut session = TokenSession::new(Agent::Opencode);
    assert_eq!(poll_session_at(&mut session, path), PollStatus::Degraded);
    assert_eq!(session.totals.input_tokens, 9);
    assert_eq!(session.totals.output_tokens, 3);
    assert_eq!(session.totals.cost_usd, None);
    assert_eq!(session.last_rowid, 2);
    assert_eq!(poll_session_at(&mut session, path), PollStatus::Unchanged);
}
