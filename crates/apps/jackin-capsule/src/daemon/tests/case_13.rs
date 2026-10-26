// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use tokio::sync::mpsc;
use tokio::time::interval;

use crate::session::SessionEvent;
use crate::tui::subscriptions::STATE_TICK_INTERVAL;

#[tokio::test(start_paused = true)]
async fn ready_status_tick_wins_over_ready_pty_output() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut state_ticker = interval(STATE_TICK_INTERVAL);
    state_ticker.tick().await;
    tokio::time::advance(STATE_TICK_INTERVAL).await;
    tx.send(SessionEvent::Output {
        session_id: 1,
        data: b"busy".to_vec(),
    })
    .unwrap_or_else(|_| panic!("test receiver must be open"));

    let selected = tokio::select! {
        biased;
        _ = state_ticker.tick() => "tick",
        Some(_) = rx.recv() => "output",
    };

    assert_eq!(
        selected, "tick",
        "ready status tick must beat ready PTY output under biased select"
    );
}
