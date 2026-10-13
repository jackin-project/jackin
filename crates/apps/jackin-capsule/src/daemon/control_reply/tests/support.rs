// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) struct CompletionDrop(pub(super) Option<oneshot::Sender<()>>);

impl Drop for CompletionDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            // A dropped receiver means the waiter is gone; delivery is best-effort.
            if sender.send(()).is_err() {
                // Completion needs no delivery.
            }
        }
    }
}

pub(super) async fn cancelled_before_poll(close_before_spawn: bool) {
    let (reply_tx, reply_rx) = oneshot::channel();
    let mut reply_rx = Some(reply_rx);
    let polled = Arc::new(AtomicBool::new(false));
    let future_polled = Arc::clone(&polled);
    let (dropped_tx, dropped_rx) = oneshot::channel();
    let dropped = CompletionDrop(Some(dropped_tx));
    if close_before_spawn {
        drop(reply_rx.take());
    }
    PendingExecReply::new(reply_tx, None).spawn(async move {
        let _dropped = dropped;
        future_polled.store(true, Ordering::SeqCst);
        std::future::pending().await
    });
    // current_thread guarantees the spawned task cannot poll until we yield.
    drop(reply_rx);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped_rx)
        .await
        .expect("cancel task promptly")
        .expect("drop command future");
    assert!(
        !polled.load(Ordering::SeqCst),
        "cancelled command must never be polled"
    );
}
