use std::sync::Arc;
use std::time::Duration;

use jackin_core::{Clock, ManualClock};
use jackin_protocol::attach::{
    ClipboardImageChunk, ClipboardImageFormat, ClipboardImageStart,
};

use super::{CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT, ClipboardImageTransfers};

fn start(transfer_id: u64, size: u64) -> ClipboardImageStart {
    ClipboardImageStart {
        transfer_id,
        format: ClipboardImageFormat::Png,
        size,
    }
}

fn chunk(transfer_id: u64, offset: u64, bytes: Vec<u8>) -> ClipboardImageChunk {
    ClipboardImageChunk {
        transfer_id,
        offset,
        bytes,
    }
}

fn transfers_with(manual: &Arc<ManualClock>) -> ClipboardImageTransfers {
    // Intermediate lets CoerceUnsized turn Arc<ManualClock> into Arc<dyn Clock>
    // without a trivial cast (and keeps `manual` for `advance`).
    let concrete = Arc::clone(manual);
    let clock: Arc<dyn Clock> = concrete;
    ClipboardImageTransfers::with_clock(clock)
}

#[test]
fn abort_idle_older_than_drops_transfer_past_timeout() {
    let clock = Arc::new(ManualClock::new());
    let mut transfers = transfers_with(&clock);
    transfers.start(start(1, 8)).expect("start");

    clock.advance(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT + Duration::from_secs(1));
    let aborted = transfers.abort_idle_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT);
    assert_eq!(aborted, 1);
    assert!(transfers.active.is_empty());
}

#[test]
fn chunk_activity_resets_idle_window() {
    let clock = Arc::new(ManualClock::new());
    let mut transfers = transfers_with(&clock);
    // 16-byte payload so a mid-transfer chunk is valid before end.
    transfers.start(start(1, 16)).expect("start");

    clock.advance(Duration::from_mins(4));
    transfers
        .chunk(chunk(1, 0, b"\x89PNG\r\n\x1a\n".to_vec()))
        .expect("chunk at T+4min");

    // Idle window restarted at T+4min; check at T+5min+1s should not abort.
    clock.advance(Duration::from_mins(1) + Duration::from_secs(1));
    let aborted = transfers.abort_idle_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT);
    assert_eq!(aborted, 0);
    assert_eq!(transfers.active.len(), 1);

    // From last activity (T+4min) + 5min + 1s = T+9min+1s.
    clock.advance(Duration::from_mins(4));
    let aborted = transfers.abort_idle_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT);
    assert_eq!(aborted, 1);
    assert!(transfers.active.is_empty());
}

#[test]
fn abort_idle_only_drops_idle_of_two_transfers() {
    let clock = Arc::new(ManualClock::new());
    let mut transfers = transfers_with(&clock);
    transfers.start(start(1, 8)).expect("start idle");
    transfers.start(start(2, 8)).expect("start active");

    clock.advance(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT + Duration::from_secs(1));
    // Keep transfer 2 alive with a late chunk (offset 0, still under size 8).
    transfers
        .chunk(chunk(2, 0, b"\x89PNG\r\n\x1a\n".to_vec()))
        .expect("refresh active transfer");

    let aborted = transfers.abort_idle_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT);
    assert_eq!(aborted, 1);
    assert!(transfers.active.contains_key(&2));
    assert!(!transfers.active.contains_key(&1));
}

#[test]
fn abort_idle_boundary_idle_exactly_timeout_is_aborted() {
    // `abort_idle_before` uses `last_activity <= cutoff`, so idle == timeout aborts.
    let clock = Arc::new(ManualClock::new());
    let mut transfers = transfers_with(&clock);
    transfers.start(start(1, 8)).expect("start");

    clock.advance(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT);
    let aborted = transfers.abort_idle_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT);
    assert_eq!(aborted, 1);
    assert!(transfers.active.is_empty());
}
