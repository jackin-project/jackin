// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use tokio::sync::mpsc;
use tokio::time::interval;

use crate::session::SessionEvent;
use crate::tui::subscriptions::STATE_TICK_INTERVAL;

#[test]
fn status_tick_select_arm_stays_above_pty_output() {
    let source = include_str!("../run_loop/events.rs");
    let tick_arm = source
        .find("_ = self.state_ticker.tick()")
        .unwrap_or_else(|| panic!("state ticker select arm missing"));
    let output_arm = source
        .find("Some(event) = self.mux.control.event_rx.recv()")
        .unwrap_or_else(|| panic!("PTY event select arm missing"));
    assert!(
        tick_arm < output_arm,
        "state ticker must stay above PTY output in the biased select"
    );
    let priority = [
        "_ = self.sigterm.recv()",
        "_ = self.sigint.recv()",
        "Some((stream, client_permit)) = self.new_clients.recv()",
        "Some(request) = self.control_rx.recv()",
        "Some(ready) = self.handshake_rx.recv()",
        "Some(frame) = self.cmd_rx.recv()",
        "_ = self.state_ticker.tick()",
        "Some(event) = self.mux.control.event_rx.recv()",
        "}, if self.esc_deadline.is_some()",
        "}, if render_deadline.is_some()",
        "_ = self.branch_context_ticker.tick()",
        "_ = self.usage_account_ticker.tick()",
    ];
    let mut previous = source
        .find("biased;")
        .unwrap_or_else(|| panic!("daemon select must retain biased priority"));
    for arm in priority {
        let position = source
            .find(arm)
            .unwrap_or_else(|| panic!("daemon select arm missing: {arm}"));
        assert!(
            position > previous,
            "daemon select priority changed at {arm}"
        );
        previous = position;
    }
}

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
