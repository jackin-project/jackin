// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::oneshot;

use super::PendingExecReply;

struct CompletionDrop(Option<oneshot::Sender<()>>);

impl Drop for CompletionDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

async fn cancelled_before_poll(close_before_spawn: bool) {
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

#[tokio::test(flavor = "current_thread")]
async fn exec_closed_requester_prevents_command_first_poll() {
    cancelled_before_poll(true).await;
}

#[tokio::test(flavor = "current_thread")]
async fn exec_requester_closing_after_approval_before_scheduling_prevents_first_poll() {
    cancelled_before_poll(false).await;
}

#[tokio::test(flavor = "current_thread")]
async fn exec_requester_closing_drops_inflight_command_before_next_stage() {
    let (reply_tx, reply_rx) = oneshot::channel();
    let (started_tx, started_rx) = oneshot::channel();
    let (continue_tx, continue_rx) = oneshot::channel();
    let (dropped_tx, dropped_rx) = oneshot::channel();
    let dropped = CompletionDrop(Some(dropped_tx));
    let executed = Arc::new(AtomicBool::new(false));
    let future_executed = Arc::clone(&executed);
    PendingExecReply::new(reply_tx, None).spawn(async move {
        let _dropped = dropped;
        started_tx.send(()).expect("notify credential barrier");
        continue_rx.await.expect("release credential barrier");
        future_executed.store(true, Ordering::SeqCst);
        jackin_protocol::control::ServerMsg::ExecDenied {
            reason: "test".to_owned(),
        }
    });
    started_rx.await.expect("reach credential barrier");
    drop(reply_rx);
    // Both branches ready: cancellation must win over credential completion.
    continue_tx.send(()).expect("release credential barrier");
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped_rx)
        .await
        .expect("cancel task promptly")
        .expect("drop inflight command");
    assert!(!executed.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "current_thread")]
async fn exec_requester_departure_kills_running_command_and_descendant() {
    let directory = tempfile::tempdir().expect("command fixture directory");
    let marker = directory.path().join("started");
    let marker_arg = marker.to_str().expect("fixture path").to_owned();
    let (reply_tx, reply_rx) = oneshot::channel();
    let (dropped_tx, dropped_rx) = oneshot::channel();
    let dropped = CompletionDrop(Some(dropped_tx));
    PendingExecReply::new(reply_tx, None).spawn(async move {
        let _dropped = dropped;
        // The descendant inherits captured output, so the transport owns both
        // processes until their pipes close. No deadline kills this fixture.
        let request = jackin_process::ExecRequest::new(
            "sh",
            [
                "-c",
                "sleep 30 & printf '%s:%s' $$ $! > \"$1\"; wait",
                "fixture",
                &marker_arg,
            ],
        )
        .no_timeout();
        let result = jackin_process::exec_async(&request)
            .await
            .expect("run command fixture");
        jackin_protocol::control::ServerMsg::ExecResult {
            exit_code: result.code.unwrap_or(-1),
            stdout: String::new(),
            stderr: String::new(),
            redacted_count: 0,
        }
    });
    let pids = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Ok(pids) = std::fs::read_to_string(&marker)
                && pids.contains(':')
            {
                break pids;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("actual command and descendant started");
    let (parent, descendant) = pids.split_once(':').expect("fixture process identities");
    let pids = [parent, descendant].map(|pid| pid.parse::<i32>().expect("process id"));
    drop(reply_rx);
    tokio::time::timeout(std::time::Duration::from_secs(1), dropped_rx)
        .await
        .expect("drop production process future promptly")
        .expect("future dropped");
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if pids.iter().all(|pid| {
                matches!(
                    nix::sys::signal::kill(nix::unistd::Pid::from_raw(*pid), None),
                    Err(nix::errno::Errno::ESRCH)
                )
            }) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("command and descendant killed and reaped after requester cancellation");
}
