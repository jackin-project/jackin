use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context as _, Result, anyhow};

use super::*;
use crate::connect_local;

const TYPED_SEED_ENV: &str = "JACKIN_BACKEND_TYPED_SEED_PATH";

#[tokio::test]
async fn complete_typed_logical_image_restores_from_a_separate_basename() -> Result<()> {
    if let Some(path) = std::env::var_os(TYPED_SEED_ENV) {
        return seed_typed_fixture(&path).await;
    }

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("typed-old.db");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg("local_store::tests::case_06::complete_typed_logical_image_restores_from_a_separate_basename")
        .arg("--nocapture")
        .env(TYPED_SEED_ENV, &path);
    let output = command_output(&mut command).await?;
    anyhow::ensure!(
        output.status.success(),
        "finished-child typed seed failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::CliUsage, &guard).await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let receipt = store
        .backup_before_migration(ManagedStoreKind::CliUsage, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("old typed fixture needs a logical backup"))?;
    let backup_path = receipt.directory.join("rollback.sqlite");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&receipt.directory)?.permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&backup_path)?.permissions().mode() & 0o777,
            0o600
        );
    }
    let restore_directory = directory.path().join("restored-store");
    std::fs::create_dir(&restore_directory)?;
    let restored_path = restore_directory.join("restored-under-new-name.db");
    std::fs::copy(&backup_path, &restored_path)?;

    let restored_database =
        turso::Builder::new_local(restored_path.to_str().context("restore path is UTF-8")?)
            .build()
            .await?;
    let restored = restored_database.connect()?;
    let before = logical::logical_digest(store.connection()).await?;
    let after = logical::logical_digest(&restored).await?;
    assert_eq!(before, after);
    assert_eq!(&before, receipt.logical_digest());

    let mut rows = restored
        .query(
            "SELECT signed_min, signed_max, subnormal, negative_zero, max_finite, nullable_text, text_payload, blob_payload FROM typed_rows WHERE id = 41",
            (),
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| anyhow!("typed row is missing"))?;
    assert!(matches!(row.get_value(0)?, turso::Value::Integer(i64::MIN)));
    assert!(matches!(row.get_value(1)?, turso::Value::Integer(i64::MAX)));
    assert!(matches!(row.get_value(2)?, turso::Value::Real(value) if value.to_bits() == 1));
    assert!(
        matches!(row.get_value(3)?, turso::Value::Real(value) if value.to_bits() == (-0.0_f64).to_bits())
    );
    assert!(
        matches!(row.get_value(4)?, turso::Value::Real(value) if value.to_bits() == f64::MAX.to_bits())
    );
    assert!(matches!(row.get_value(5)?, turso::Value::Null));
    assert!(
        matches!(row.get_value(6)?, turso::Value::Text(value) if value == "quota 🐾 exact text")
    );
    assert!(
        matches!(row.get_value(7)?, turso::Value::Blob(value) if value.as_slice() == [0, 255, 128, 10])
    );
    drop(row);
    assert!(rows.next().await?.is_none());
    drop(rows);

    let mut sequence = restored
        .query(
            "SELECT seq FROM sqlite_sequence WHERE name = 'old_rows'",
            (),
        )
        .await?;
    let sequence_row = sequence
        .next()
        .await?
        .ok_or_else(|| anyhow!("AUTOINCREMENT sequence is missing"))?;
    assert_eq!(sequence_row.get::<i64>(0)?, 20);
    drop(sequence_row);
    assert!(sequence.next().await?.is_none());
    drop(sequence);
    let mut audit = restored
        .query(
            "SELECT row_key FROM typed_rows_audit WHERE row_key = 41",
            (),
        )
        .await?;
    let audit_row = audit.next().await?.context("trigger audit row")?;
    assert_eq!(audit_row.get::<i64>(0)?, 41);
    drop(audit_row);
    assert!(audit.next().await?.is_none());
    drop(audit);
    let mut extension = restored
        .query(
            "SELECT payload FROM extension_rows WHERE store = 'snapshot' AND slot = 7",
            (),
        )
        .await?;
    let extension_row = extension.next().await?.context("WITHOUT ROWID row")?;
    assert_eq!(extension_row.get::<String>(0)?, "retained extension");
    drop(extension_row);
    assert!(extension.next().await?.is_none());
    drop(extension);
    store.connection().execute("ROLLBACK", ()).await?;
    Ok(())
}

async fn seed_typed_fixture(path: &std::ffi::OsStr) -> Result<()> {
    let path = PathBuf::from(path);
    let path = path.to_str().context("typed fixture path is UTF-8")?;
    let connection = connect_local(path).await.map_err(anyhow::Error::msg)?;
    connection
        .execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta (key, value) VALUES ('schema_version', '1');
             CREATE TABLE old_rows (id INTEGER PRIMARY KEY AUTOINCREMENT, payload TEXT);
             INSERT INTO old_rows (id, payload) VALUES (1, 'first'), (3, 'gap'), (20, 'sequence');
             DELETE FROM old_rows WHERE id = 20;
             CREATE TABLE typed_rows (
                id INTEGER PRIMARY KEY,
                signed_min INTEGER NOT NULL,
                signed_max INTEGER NOT NULL,
                subnormal REAL NOT NULL,
                negative_zero REAL NOT NULL,
                max_finite REAL NOT NULL,
                nullable_text TEXT,
                text_payload TEXT NOT NULL,
                blob_payload BLOB NOT NULL
             );
             CREATE INDEX typed_rows_text ON typed_rows(text_payload);
             CREATE TABLE typed_rows_audit (row_key INTEGER NOT NULL);
             CREATE TRIGGER typed_rows_insert_audit AFTER INSERT ON typed_rows
                BEGIN INSERT INTO typed_rows_audit(row_key) VALUES (new.id); END;
             CREATE VIEW typed_rows_view AS SELECT id, text_payload FROM typed_rows;
             CREATE TABLE extension_rows (
                store TEXT NOT NULL,
                slot INTEGER NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (store, slot)
             );
             INSERT INTO extension_rows VALUES ('snapshot', 7, 'retained extension');
             PRAGMA application_id = 305419896;
             PRAGMA user_version = 1;",
        )
        .await?;
    connection
        .execute(
            "INSERT INTO typed_rows VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            turso::params![
                41_i64,
                i64::MIN,
                i64::MAX,
                f64::from_bits(1),
                -0.0_f64,
                f64::MAX,
                Option::<String>::None,
                "quota 🐾 exact text",
                vec![0_u8, 255, 128, 10],
            ],
        )
        .await?;
    Ok(())
}
