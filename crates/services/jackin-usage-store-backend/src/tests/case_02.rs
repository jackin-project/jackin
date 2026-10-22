// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context as _;

#[tokio::test]
async fn pinned_turso_vacuum_into_works_while_same_database_writer_lock_is_held()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let source_path = directory.path().join("source.db");
    let backup_path = directory.path().join("logical-backup.db");
    let restored_path = directory.path().join("restored.db");
    let source = source_path.to_str().context("source path is UTF-8")?;
    let backup = backup_path.to_str().context("backup path is UTF-8")?;
    anyhow::ensure!(
        source.is_ascii()
            && backup.is_ascii()
            && !source.contains('\'')
            && !source.contains('"')
            && !backup.contains('\'')
            && !backup.contains('"'),
        "test paths must be safe SQL literals"
    );
    anyhow::ensure!(!backup_path.exists(), "VACUUM INTO destination must be new");

    let database = turso::Builder::new_local(source).build().await?;
    let writer = database.connect()?;
    let mut journal_mode = writer.query("PRAGMA journal_mode = 'wal'", ()).await?;
    let row = journal_mode
        .next()
        .await?
        .context("journal mode query returns a row")?;
    anyhow::ensure!(
        row.get::<String>(0)?.eq_ignore_ascii_case("wal"),
        "source store must use WAL mode"
    );
    writer
        .execute(
            "CREATE TABLE quota_backup_probe(id INTEGER PRIMARY KEY, value TEXT NOT NULL)",
            (),
        )
        .await?;
    writer
        .execute(
            "INSERT INTO quota_backup_probe(id, value) VALUES (17, 'committed wal row')",
            (),
        )
        .await?;
    writer.cacheflush()?;
    let source_wal = sidecar_path(&source_path, "-wal");
    anyhow::ensure!(
        source_wal.exists(),
        "committed data must reside in source WAL"
    );
    anyhow::ensure!(
        std::fs::metadata(&source_wal)?.len() > 0,
        "source WAL must contain frames before backup"
    );

    writer.execute("BEGIN IMMEDIATE", ()).await?;
    anyhow::ensure!(
        !writer.is_autocommit()?,
        "writer reservation must remain active"
    );

    let backup_connection = database.connect()?;
    backup_connection
        .execute(&format!("VACUUM main INTO '{backup}'"), ())
        .await?;
    anyhow::ensure!(
        !writer.is_autocommit()?,
        "VACUUM INTO must leave the source writer transaction active"
    );

    let third_writer = database.connect()?;
    third_writer.busy_timeout(Duration::ZERO)?;
    let blocked = third_writer.execute("BEGIN IMMEDIATE", ()).await;
    let blocked_error = blocked.expect_err("third writer must be excluded by reservation");
    let blocked_message = blocked_error.to_string().to_ascii_lowercase();
    anyhow::ensure!(
        blocked_message.contains("busy") || blocked_message.contains("locked"),
        "expected a lock conflict, received: {blocked_error}"
    );
    anyhow::ensure!(
        third_writer.is_autocommit()?,
        "blocked begin must not open a transaction"
    );

    drop(backup_connection);
    anyhow::ensure!(
        backup_path.is_file(),
        "VACUUM INTO must publish a backup file"
    );
    let backup_wal = sidecar_path(&backup_path, "-wal");
    if backup_wal.exists() {
        anyhow::ensure!(
            std::fs::metadata(&backup_wal)?.len() == 0,
            "VACUUM INTO main output must not depend on WAL frames"
        );
    }
    std::fs::copy(&backup_path, &restored_path)?;

    let restored =
        turso::Builder::new_local(restored_path.to_str().context("restore path is UTF-8")?)
            .build()
            .await?;
    let restored_reader = restored.connect()?;
    assert_probe_row(&restored_reader).await?;

    writer.execute("ROLLBACK", ()).await?;
    anyhow::ensure!(
        writer.is_autocommit()?,
        "rollback must release writer reservation"
    );
    third_writer.execute("BEGIN IMMEDIATE", ()).await?;
    third_writer.execute("ROLLBACK", ()).await?;
    Ok(())
}

fn sidecar_path(database_path: &std::path::Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", database_path.display()))
}

async fn assert_probe_row(connection: &turso::Connection) -> anyhow::Result<()> {
    let mut rows = connection
        .query("SELECT id, value FROM quota_backup_probe", ())
        .await?;
    let row = rows.next().await?.context("backup row is present")?;
    anyhow::ensure!(row.get::<i64>(0)? == 17, "backup row id must be exact");
    anyhow::ensure!(
        row.get::<String>(1)? == "committed wal row",
        "backup must include the committed WAL row"
    );
    anyhow::ensure!(
        rows.next().await?.is_none(),
        "backup has one exact probe row"
    );
    Ok(())
}
