use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};

use super::*;

#[tokio::test]
async fn preopen_raw_file_descriptors_close_before_builder_even_on_invalid_database() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invalid-source.db");
    let main = b"not a sqlite database";
    let wal = b"uncheckpointed wal bytes";
    let journal = b"rollback journal bytes";
    std::fs::write(&path, main)?;
    std::fs::write(format!("{}-wal", path.display()), wal)?;
    std::fs::write(format!("{}-journal", path.display()), journal)?;
    let guard = acquire_store_operation(&path).await?;

    if let Ok(store) =
        LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await
    {
        drop(store);
    }
    let events = forensics::take_events(&path);
    assert_eq!(
        events,
        vec![
            forensics::CaptureEvent::SourceOpened,
            forensics::CaptureEvent::SourceClosed,
            forensics::CaptureEvent::SourceOpened,
            forensics::CaptureEvent::SourceClosed,
            forensics::CaptureEvent::SourceOpened,
            forensics::CaptureEvent::SourceClosed,
            forensics::CaptureEvent::BuilderStarted,
        ]
    );
    let evidence_dir = std::fs::read_dir(directory.path())?
        .filter_map(std::result::Result::ok)
        .find(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".jackin-quota-backup-")
        })
        .ok_or_else(|| anyhow!("forensic evidence directory was not created"))?
        .path();
    assert_eq!(std::fs::read(evidence_dir.join("source.main"))?, main);
    assert_eq!(std::fs::read(evidence_dir.join("source.wal"))?, wal);
    assert_eq!(std::fs::read(evidence_dir.join("source.journal"))?, journal);
    assert!(registry::is_opened_ever(&path));
    Ok(())
}

#[tokio::test]
async fn ready_manifest_publish_failure_leaves_old_schema_and_no_ready_claim() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("manifest-failure.db");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg("local_store::tests::seed_old_store_in_finished_child")
        .arg("--nocapture")
        .env(SEED_PATH_ENV, &path);
    let output = command_output(&mut command).await?;
    anyhow::ensure!(
        output.status.success(),
        "finished-child old-schema seed failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    manifest::fail_next_immutable_publish(directory.path(), "backup_ready.manifest");
    let result = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await;
    assert!(
        matches!(&result, Err(StoreError::Io(_, _))),
        "ready publish should return an injected I/O error, got {result:?}"
    );
    assert_no_ready(directory.path())?;
    assert_old_source(store.connection()).await?;
    store.connection().execute("ROLLBACK", ()).await?;
    Ok(())
}

#[tokio::test]
async fn forensic_io_failures_close_source_files_before_any_builder() -> Result<()> {
    for (index, phase) in [
        forensics::CaptureFaultPhase::SourceOpen,
        forensics::CaptureFaultPhase::HashSeek,
        forensics::CaptureFaultPhase::HashRead,
        forensics::CaptureFaultPhase::CopySeek,
        forensics::CaptureFaultPhase::CopyRead,
        forensics::CaptureFaultPhase::ArtifactWrite,
        forensics::CaptureFaultPhase::ArtifactPermission,
        forensics::CaptureFaultPhase::ArtifactSync,
    ]
    .into_iter()
    .enumerate()
    {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join(format!("capture-fault-{index}.db"));
        let source = b"unchanged source bytes for injected capture failure";
        std::fs::write(&path, source)?;
        forensics::fail_next_capture_at(&path, phase);
        let guard = acquire_store_operation(&path).await?;
        let opened = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await;
        assert!(matches!(opened, Err(StoreError::Io(_, _))), "{phase:?}");

        let events = forensics::take_events(&path);
        let expected = if phase == forensics::CaptureFaultPhase::SourceOpen {
            Vec::new()
        } else {
            vec![
                forensics::CaptureEvent::SourceOpened,
                forensics::CaptureEvent::SourceClosed,
            ]
        };
        assert_eq!(events, expected, "{phase:?}");
        assert!(!events.contains(&forensics::CaptureEvent::BuilderStarted));
        assert!(!registry::is_opened_ever(&path));
        assert_eq!(std::fs::read(&path)?, source);
        drop(guard);
    }
    Ok(())
}

#[tokio::test]
async fn absent_capture_does_not_skip_backup_for_a_later_unversioned_image() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("later-unversioned.db");
    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::CliUsage, &guard).await?;
    assert_eq!(store.preopen_status(), PreopenStatus::CapturedAbsent);
    store
        .connection()
        .execute_batch(
            "CREATE TABLE unversioned_rows (
                id INTEGER PRIMARY KEY,
                amount REAL NOT NULL,
                payload BLOB NOT NULL
             );
             INSERT INTO unversioned_rows VALUES (9, -0.0, X'00FF');",
        )
        .await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let receipt = store
        .backup_before_migration(ManagedStoreKind::CliUsage, None, 2)
        .await?
        .ok_or_else(|| anyhow!("nonempty unversioned source needs a rollback image"))?;
    assert_eq!(receipt.old_version(), None);
    let backup_path = receipt.directory.join("rollback.sqlite");
    let backup_database =
        turso::Builder::new_local(backup_path.to_str().context("backup path is UTF-8")?)
            .build()
            .await?;
    let backup_connection = backup_database.connect()?;
    assert_eq!(
        logical::logical_digest(store.connection()).await?,
        logical::logical_digest(&backup_connection).await?
    );
    drop(backup_connection);
    drop(backup_database);
    store.connection().execute("ROLLBACK", ()).await?;
    Ok(())
}

#[test]
fn immutable_manifest_collision_never_overwrites_existing_bytes() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let generation = directory.path().join("generation");
    std::fs::create_dir(&generation)?;
    let ready = generation.join("backup_ready.manifest");
    let original = b"preserve the prior immutable receipt";
    std::fs::write(&ready, original)?;

    let result = manifest::write_immutable(&generation, "backup_ready.manifest", b"replacement");
    assert!(matches!(result, Err(StoreError::BackupCollision)));
    assert_eq!(std::fs::read(ready)?, original);
    Ok(())
}

#[tokio::test]
async fn cancellation_while_source_fd_is_open_keeps_capture_order_and_registry_safe() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("cancel-during-capture.db");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg("local_store::tests::seed_old_store_in_finished_child")
        .arg("--nocapture")
        .env(SEED_PATH_ENV, &path);
    let output = command_output(&mut command).await?;
    anyhow::ensure!(
        output.status.success(),
        "finished-child old-schema seed failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let (ready_receiver, release_sender) = forensics::set_capture_pause(&path);
    let release = TestPauseRelease::new(release_sender);
    let first_guard = acquire_store_operation(&path).await?;
    let first_path = path.clone();
    let opening = tokio::spawn(async move {
        LocalStore::open_managed(&first_path, ManagedStoreKind::QuotaSnapshot, &first_guard).await
    });
    tokio::task::spawn_blocking(move || ready_receiver.recv_timeout(Duration::from_secs(5)))
        .await??;
    opening.abort();
    let opening_result = tokio::time::timeout(Duration::from_secs(5), opening)
        .await
        .map_err(|_| anyhow!("canceled open task did not finish"))?;
    match opening_result {
        Err(error) if error.is_cancelled() => {}
        _ => return Err(anyhow!("managed open was not canceled during raw capture")),
    }
    drop(release);

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if registry::is_opened_ever(&path) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    let events = forensics::take_events(&path);
    assert!(!events.is_empty());
    let mut open_count = 0;
    for event in events {
        match event {
            forensics::CaptureEvent::SourceOpened => open_count += 1,
            forensics::CaptureEvent::SourceClosed => {
                assert!(open_count > 0, "source close must follow an open");
                open_count -= 1;
            }
            forensics::CaptureEvent::BuilderStarted => {
                return Err(anyhow!(
                    "builder started before captured source files closed"
                ));
            }
            forensics::CaptureEvent::ArtifactOpened(_) => {}
            forensics::CaptureEvent::ReservationPaused
            | forensics::CaptureEvent::ReservationReleased
            | forensics::CaptureEvent::ReservationWaitTimedOut => {}
        }
    }
    assert_eq!(open_count, 0);

    let guard = acquire_store_operation(&path).await?;
    let store = tokio::time::timeout(
        Duration::from_secs(5),
        LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard),
    )
    .await??;
    assert_eq!(store.preopen_status(), PreopenStatus::CapturedExisting);
    assert_eq!(
        forensics::take_events(&path),
        vec![forensics::CaptureEvent::BuilderStarted]
    );
    Ok(())
}

fn assert_no_ready(parent: &Path) -> Result<()> {
    for entry in std::fs::read_dir(parent)? {
        anyhow::ensure!(
            !entry?.path().join("backup_ready.manifest").exists(),
            "failed ready publication must not leave a ready manifest"
        );
    }
    Ok(())
}

async fn assert_old_source(connection: &Connection) -> Result<()> {
    let mut version = connection.query("PRAGMA user_version", ()).await?;
    assert_eq!(
        version
            .next()
            .await?
            .context("old version row")?
            .get::<i64>(0)?,
        1
    );
    drop(version);
    let mut rows = connection
        .query("SELECT COUNT(*) FROM old_rows", ())
        .await?;
    assert_eq!(
        rows.next()
            .await?
            .context("old rows count")?
            .get::<i64>(0)?,
        2
    );
    drop(rows);
    Ok(())
}
