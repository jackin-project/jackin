// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_core::ManualClock;
use jackin_protocol::attach::{ClipboardImageChunk, ClipboardImageFormat, ClipboardImageStart};

use super::super::{CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT, ClipboardImageTransfers};

fn transfers(clock: &Arc<ManualClock>) -> ClipboardImageTransfers {
    let owned: Arc<ManualClock> = Arc::<ManualClock>::clone(clock);
    let clock: Arc<dyn jackin_core::Clock> = owned;
    ClipboardImageTransfers::with_clock(clock)
}

fn start(transfer_id: u64) -> ClipboardImageStart {
    ClipboardImageStart {
        transfer_id,
        format: ClipboardImageFormat::Png,
        size: 4,
    }
}

fn chunk(transfer_id: u64, offset: u64) -> ClipboardImageChunk {
    ClipboardImageChunk {
        transfer_id,
        offset,
        bytes: vec![0x89, 0x50],
    }
}

#[test]
fn abort_idle_older_than_drops_transfer_past_timeout() {
    let clock = Arc::new(ManualClock::new());
    let mut active = transfers(&clock);
    active.start(start(7)).unwrap();
    active.chunk(chunk(7, 0)).unwrap();
    clock
        .advance(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT + Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        active.abort_idle_ids_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT),
        vec![7]
    );
}

#[test]
fn abort_idle_only_drops_idle_of_two_transfers() {
    let clock = Arc::new(ManualClock::new());
    let mut active = transfers(&clock);
    active.start(start(1)).unwrap();
    clock.advance(Duration::from_secs(240)).unwrap();
    active.start(start(2)).unwrap();
    clock.advance(Duration::from_secs(120)).unwrap();
    assert_eq!(
        active.abort_idle_ids_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT),
        vec![1]
    );
    active.chunk(chunk(2, 0)).unwrap();
}

#[test]
fn abort_idle_boundary_idle_exactly_timeout_is_aborted() {
    let clock = Arc::new(ManualClock::new());
    let mut active = transfers(&clock);
    active.start(start(9)).unwrap();
    clock
        .advance(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT)
        .unwrap();
    assert_eq!(
        active.abort_idle_ids_older_than(CLIPBOARD_IMAGE_TRANSFER_IDLE_TIMEOUT),
        vec![9]
    );
}
