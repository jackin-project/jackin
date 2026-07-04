// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `runtime`.
use super::{
    Subscription, SubscriptionPoll, spawn_blocking_subscription, spawn_named_blocking_subscription,
};

fn wait_for_worker_poll() {
    #[expect(
        clippy::disallowed_methods,
        reason = "test polls an owned OS worker thread without a tokio runtime"
    )]
    std::thread::sleep(std::time::Duration::from_millis(1));
}

#[test]
fn oneshot_subscription_reports_ready_value() {
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    tx.send(7).expect("receiver should be live");

    assert_eq!(rx.poll_next(), SubscriptionPoll::Ready(7));
}

#[test]
fn oneshot_subscription_reports_pending_then_closed() {
    let (tx, mut rx) = tokio::sync::oneshot::channel::<u8>();

    assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);

    drop(tx);

    assert_eq!(rx.poll_next(), SubscriptionPoll::Closed);
}

#[test]
fn mpsc_subscription_reports_ready_values() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    tx.send(7).expect("receiver should be live");
    tx.send(8).expect("receiver should be live");

    assert_eq!(rx.poll_next(), SubscriptionPoll::Ready(7));
    assert_eq!(rx.poll_next(), SubscriptionPoll::Ready(8));
    assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);
}

#[test]
fn mpsc_subscription_reports_closed() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<u8>();

    assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);

    drop(tx);

    assert_eq!(rx.poll_next(), SubscriptionPoll::Closed);
}

#[test]
fn std_mpsc_subscription_reports_ready_value() {
    let (tx, mut rx) = std::sync::mpsc::channel();
    tx.send(7).expect("receiver should be live");

    assert_eq!(rx.poll_next(), SubscriptionPoll::Ready(7));
    assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);
}

#[test]
fn std_mpsc_subscription_reports_closed() {
    let (tx, mut rx) = std::sync::mpsc::channel::<u8>();

    assert_eq!(rx.poll_next(), SubscriptionPoll::Pending);

    drop(tx);

    assert_eq!(rx.poll_next(), SubscriptionPoll::Closed);
}

#[test]
fn spawn_blocking_subscription_reports_worker_result() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime should build");

    runtime.block_on(async {
        let rx = spawn_blocking_subscription(|| 7);

        assert_eq!(rx.await.expect("worker should send result"), 7);
    });
}

#[test]
fn named_blocking_subscription_reports_worker_result_without_runtime() {
    let mut rx = spawn_named_blocking_subscription("jackin-tui-test-worker", || 7);

    for _ in 0..100 {
        match rx.poll_next() {
            SubscriptionPoll::Ready(value) => {
                assert_eq!(value, 7);
                return;
            }
            SubscriptionPoll::Pending => wait_for_worker_poll(),
            SubscriptionPoll::Closed => panic!("worker closed before sending result"),
        }
    }

    panic!("worker did not finish");
}
