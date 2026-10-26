use std::path::PathBuf;

use anyhow::{Result, anyhow};

use super::*;
use crate::connect_local;
mod support;
pub(super) use support::command_output;

const SEED_PATH_ENV: &str = "JACKIN_BACKEND_BACKUP_SEED_PATH";

mod case_07;

#[tokio::test]
async fn seed_old_store_in_finished_child() -> Result<()> {
    let Some(path) = std::env::var_os(SEED_PATH_ENV) else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    let path = path
        .to_str()
        .ok_or_else(|| anyhow!("seed database path is not valid Unicode"))?;
    let connection = connect_local(path).await.map_err(anyhow::Error::msg)?;
    connection
        .execute(
            "CREATE TABLE old_rows (id INTEGER PRIMARY KEY AUTOINCREMENT, payload TEXT, score REAL, raw BLOB, nullable TEXT)",
            (),
        )
        .await?;
    connection
        .execute("CREATE INDEX old_rows_payload ON old_rows(payload)", ())
        .await?;
    connection
        .execute(
            "CREATE VIEW old_rows_view AS SELECT id, payload FROM old_rows",
            (),
        )
        .await?;
    connection
        .execute(
            "INSERT INTO old_rows(id, payload, score, raw, nullable) VALUES (1, 'first', 1.25, X'00FF', NULL), (3, 'third', -0.0, X'107F', 'kept')",
            (),
        )
        .await?;
    connection
        .execute(
            "INSERT INTO old_rows(id, payload) VALUES (20, 'sequence')",
            (),
        )
        .await?;
    connection
        .execute("DELETE FROM old_rows WHERE id = 20", ())
        .await?;
    connection.execute("PRAGMA user_version = 1", ()).await?;
    Ok(())
}
