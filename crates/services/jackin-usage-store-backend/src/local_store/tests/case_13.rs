use std::time::Duration;

use anyhow::Result;

use super::*;

#[tokio::test]
async fn legacy_builder_cannot_acquire_reserved_sidecar_inode() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("reserved-source.db");
    case_10::seed_old_store(&path).await?;
    let sidecar = path::sidecar_path(&path, "-wal");
    // Keep the sidecar valid for Turso after the reservation race is checked.
    std::fs::write(&sidecar, b"")?;

    let (ready_receiver, release_sender) = forensics::set_capture_reservation_pause(&path);
    let release = TestPauseRelease::new(release_sender);
    let guard = acquire_store_operation(&path).await?;
    let opening_path = path.clone();
    let opening = tokio::spawn(async move {
        LocalStore::open_managed(&opening_path, ManagedStoreKind::QuotaSnapshot, &guard).await
    });

    tokio::task::spawn_blocking(move || ready_receiver.recv_timeout(Duration::from_secs(5)))
        .await??;
    let sidecar_text = sidecar
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("sidecar path is not UTF-8"))?;
    let legacy = tokio::time::timeout(Duration::from_secs(5), connect_local(sidecar_text)).await;
    drop(release);
    let managed = tokio::time::timeout(Duration::from_secs(10), opening)
        .await
        .map_err(|_| {
            anyhow::anyhow!("managed open did not finish after reservation release")
        })???;
    drop(managed);

    let legacy_error = match legacy {
        Ok(Err(error)) => error,
        Ok(Ok(connection)) => {
            drop(connection);
            "legacy builder unexpectedly succeeded".to_owned()
        }
        Err(_) => "legacy builder exceeded bounded wait".to_owned(),
    };
    assert_eq!(
        legacy_error,
        "track legacy local store open failed: managed store source identity was already opened through an alias"
    );
    assert!(!forensics::take_events(&sidecar).contains(&forensics::CaptureEvent::BuilderStarted));
    let capture_events = forensics::take_events(&path);
    assert!(capture_events.contains(&forensics::CaptureEvent::ReservationPaused));
    assert!(capture_events.contains(&forensics::CaptureEvent::ReservationReleased));
    assert!(!capture_events.contains(&forensics::CaptureEvent::ReservationWaitTimedOut));
    Ok(())
}
