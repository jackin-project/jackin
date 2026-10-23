// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::apple_container_client::{
    AppleContainerApi, AppleContainerCommandRunner, AppleContainerInfo, AppleContainerSpec,
    AppleContainerStatus,
};
use anyhow::Result;
use std::future::Future;
use std::sync::Mutex;
use std::task::Poll;

struct ListingRunner {
    stdout: Vec<u8>,
    success: bool,
    requests: Mutex<Vec<(std::path::PathBuf, Vec<String>)>>,
}

impl ListingRunner {
    fn new(stdout: impl AsRef<[u8]>, success: bool) -> Self {
        Self {
            stdout: stdout.as_ref().to_vec(),
            success,
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl AppleContainerCommandRunner for ListingRunner {
    async fn exec(
        &self,
        request: &jackin_process::ExecRequest,
    ) -> Result<jackin_process::ExecResult> {
        std::future::poll_fn(|_| {
            self.requests.lock().unwrap().push((
                request.program.clone(),
                request
                    .args
                    .iter()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect(),
            ));
            Poll::Ready(Ok(jackin_process::ExecResult {
                code: Some(i32::from(!self.success)),
                success: self.success,
                stdout: self.stdout.clone(),
                stderr: Vec::new(),
                duration: std::time::Duration::ZERO,
                timed_out: false,
            }))
        })
        .await
    }
}

struct RunnerBackedClient {
    runner: ListingRunner,
    start_calls: Mutex<Vec<String>>,
}

impl RunnerBackedClient {
    fn new(stdout: impl AsRef<[u8]>, success: bool) -> Self {
        Self {
            runner: ListingRunner::new(stdout, success),
            start_calls: Mutex::new(Vec::new()),
        }
    }
}

impl AppleContainerApi for RunnerBackedClient {
    async fn run_container(&self, _name: &str, _spec: &AppleContainerSpec) -> Result<()> {
        std::future::poll_fn(|_| Poll::Ready(Err(anyhow::anyhow!("unexpected run")))).await
    }

    async fn start_container(&self, name: &str) -> Result<()> {
        std::future::poll_fn(|_| {
            self.start_calls.lock().unwrap().push(name.to_owned());
            Poll::Ready(Ok(()))
        })
        .await
    }

    async fn stop_container(&self, _name: &str) -> Result<()> {
        std::future::poll_fn(|_| Poll::Ready(Err(anyhow::anyhow!("unexpected stop")))).await
    }

    async fn remove_container(&self, _name: &str) -> Result<()> {
        std::future::poll_fn(|_| Poll::Ready(Err(anyhow::anyhow!("unexpected remove")))).await
    }

    async fn inspect_container(&self, name: &str) -> Result<Option<AppleContainerInfo>> {
        crate::apple_container_client::AppleContainerClient::new()
            .inspect_container_with_command_runner(&self.runner, name)
            .await
    }

    async fn list_containers(&self, prefix: &str) -> Result<Vec<AppleContainerInfo>> {
        crate::apple_container_client::AppleContainerClient::new()
            .list_containers_with_command_runner(&self.runner, prefix)
            .await
    }
}

enum Inspection {
    Found(AppleContainerInfo),
    Absent,
    Failed,
}

enum Inventory {
    Rows(Vec<AppleContainerInfo>),
    Failed,
}

struct LifecycleClient {
    inspection: Inspection,
    inventory: Inventory,
    start_calls: Mutex<Vec<String>>,
    fail_start: bool,
}

impl LifecycleClient {
    fn with_status(status: AppleContainerStatus) -> Self {
        let info = AppleContainerInfo {
            id: "jk-a".to_owned(),
            status,
        };
        Self {
            inspection: Inspection::Found(info.clone()),
            inventory: Inventory::Rows(vec![info]),
            start_calls: Mutex::new(Vec::new()),
            fail_start: false,
        }
    }

    fn failed_inventory() -> Self {
        Self {
            inspection: Inspection::Failed,
            inventory: Inventory::Failed,
            start_calls: Mutex::new(Vec::new()),
            fail_start: false,
        }
    }

    fn calls(&self) -> Vec<String> {
        self.start_calls.lock().unwrap().clone()
    }
}

impl AppleContainerApi for LifecycleClient {
    async fn run_container(&self, _name: &str, _spec: &AppleContainerSpec) -> Result<()> {
        std::future::poll_fn(|_| Poll::Ready(Err(anyhow::anyhow!("unexpected run")))).await
    }

    async fn start_container(&self, name: &str) -> Result<()> {
        std::future::poll_fn(|_| {
            self.start_calls.lock().unwrap().push(name.to_owned());
            let result = if self.fail_start {
                Err(anyhow::anyhow!("fixture start failure"))
            } else {
                Ok(())
            };
            Poll::Ready(result)
        })
        .await
    }

    async fn stop_container(&self, _name: &str) -> Result<()> {
        std::future::poll_fn(|_| Poll::Ready(Err(anyhow::anyhow!("unexpected stop")))).await
    }

    async fn remove_container(&self, _name: &str) -> Result<()> {
        std::future::poll_fn(|_| Poll::Ready(Err(anyhow::anyhow!("unexpected remove")))).await
    }

    async fn inspect_container(&self, _name: &str) -> Result<Option<AppleContainerInfo>> {
        std::future::poll_fn(|_| {
            let result = match &self.inspection {
                Inspection::Found(info) => Ok(Some(info.clone())),
                Inspection::Absent => Ok(None),
                Inspection::Failed => Err(anyhow::anyhow!("fixture inventory parse error")),
            };
            Poll::Ready(result)
        })
        .await
    }

    async fn list_containers(&self, prefix: &str) -> Result<Vec<AppleContainerInfo>> {
        std::future::poll_fn(|_| {
            let result = match &self.inventory {
                Inventory::Rows(rows) => Ok(rows
                    .iter()
                    .filter(|row| row.id.starts_with(prefix))
                    .cloned()
                    .collect()),
                Inventory::Failed => Err(anyhow::anyhow!("fixture inventory parse error")),
            };
            Poll::Ready(result)
        })
        .await
    }
}

type EffectFuture = std::pin::Pin<Box<dyn Future<Output = Result<()>>>>;

fn effect_recorder() -> (
    std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>,
    impl FnOnce() -> EffectFuture,
) {
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let captured = std::rc::Rc::clone(&events);
    let effect = move || -> EffectFuture {
        Box::pin(async move {
            captured.borrow_mut().push("activate");
            captured.borrow_mut().push("wait");
            captured.borrow_mut().push("attach");
            captured.borrow_mut().push("record");
            Ok(())
        })
    };
    (events, effect)
}

#[tokio::test]
async fn reconnect_starts_exactly_stopped_once_and_runs_effects_in_order() {
    for (status, expected_starts) in [
        (AppleContainerStatus::Running, 0),
        (AppleContainerStatus::Stopped, 1),
    ] {
        let client = LifecycleClient::with_status(status);
        let (events, effects) = effect_recorder();
        reconnect_with_client_and(&client, "jk-a", effects)
            .await
            .unwrap();

        let starts = client.calls();
        assert_eq!(starts.len(), expected_starts);
        if expected_starts == 1 {
            assert_eq!(starts, vec!["jk-a".to_owned()]);
        }
        assert_eq!(
            *events.borrow(),
            vec!["activate", "wait", "attach", "record"]
        );
    }
}

#[tokio::test]
async fn uncertain_absent_or_failed_observations_do_not_start_or_run_later_effects() {
    let mut clients = vec![
        LifecycleClient::with_status(AppleContainerStatus::Unknown),
        LifecycleClient::with_status(AppleContainerStatus::Stopping),
        LifecycleClient::failed_inventory(),
    ];
    clients.push(LifecycleClient {
        inspection: Inspection::Absent,
        inventory: Inventory::Rows(Vec::new()),
        start_calls: Mutex::new(Vec::new()),
        fail_start: false,
    });

    for client in clients {
        let (events, effects) = effect_recorder();
        assert!(
            reconnect_with_client_and(&client, "jk-a", effects)
                .await
                .is_err()
        );
        assert!(client.calls().is_empty());
        assert!(events.borrow().is_empty());
    }

    for (stdout, success) in [(b"[not-json]".as_slice(), true), (b"[]".as_slice(), false)] {
        let client = RunnerBackedClient::new(stdout, success);
        let (events, effects) = effect_recorder();
        assert!(
            reconnect_with_client_and(&client, "jk-a", effects)
                .await
                .is_err()
        );
        assert!(client.start_calls.lock().unwrap().is_empty());
        assert!(events.borrow().is_empty());
        assert_eq!(client.runner.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn mismatched_exact_id_and_start_failure_abstain_before_later_effects() {
    let mismatch = LifecycleClient {
        inspection: Inspection::Found(AppleContainerInfo {
            id: "jk-a-extra".to_owned(),
            status: AppleContainerStatus::Stopped,
        }),
        inventory: Inventory::Rows(Vec::new()),
        start_calls: Mutex::new(Vec::new()),
        fail_start: false,
    };
    let (events, effects) = effect_recorder();
    assert!(
        reconnect_with_client_and(&mismatch, "jk-a", effects)
            .await
            .is_err()
    );
    assert!(mismatch.calls().is_empty());
    assert!(events.borrow().is_empty());

    let stopped = LifecycleClient {
        fail_start: true,
        ..LifecycleClient::with_status(AppleContainerStatus::Stopped)
    };
    let (events, effects) = effect_recorder();
    assert!(
        reconnect_with_client_and(&stopped, "jk-a", effects)
            .await
            .is_err()
    );
    assert_eq!(stopped.calls(), vec!["jk-a".to_owned()]);
    assert!(events.borrow().is_empty());
}

#[tokio::test]
async fn purge_requires_a_valid_inventory_and_blocks_every_exact_target_state() {
    for status in [
        AppleContainerStatus::Running,
        AppleContainerStatus::Stopped,
        AppleContainerStatus::Unknown,
        AppleContainerStatus::Stopping,
    ] {
        let client = LifecycleClient::with_status(status);
        assert!(ensure_absent_for_purge_with(&client, "jk-a").await.is_err());
    }

    let prefix_neighbor = LifecycleClient {
        inspection: Inspection::Absent,
        inventory: Inventory::Rows(vec![AppleContainerInfo {
            id: "jk-a-extra".to_owned(),
            status: AppleContainerStatus::Stopped,
        }]),
        start_calls: Mutex::new(Vec::new()),
        fail_start: false,
    };
    ensure_absent_for_purge_with(&prefix_neighbor, "jk-a")
        .await
        .expect("prefix neighbor does not establish exact ID presence");

    let empty = LifecycleClient {
        inspection: Inspection::Absent,
        inventory: Inventory::Rows(Vec::new()),
        start_calls: Mutex::new(Vec::new()),
        fail_start: false,
    };
    ensure_absent_for_purge_with(&empty, "jk-a")
        .await
        .expect("complete empty inventory proves absence");

    let failure = LifecycleClient::failed_inventory();
    assert!(
        ensure_absent_for_purge_with(&failure, "jk-a")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn purge_parser_errors_and_nonzero_output_never_prove_exact_absence() {
    for (stdout, success) in [(b"[not-json]".as_slice(), true), (b"[]".as_slice(), false)] {
        let client = RunnerBackedClient::new(stdout, success);
        assert!(ensure_absent_for_purge_with(&client, "jk-a").await.is_err());
        assert!(client.start_calls.lock().unwrap().is_empty());
        assert_eq!(client.runner.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn purge_refuses_a_stopped_exact_container_from_the_production_listing_parser() {
    let client = RunnerBackedClient::new(
        br#"[
            {"id":"jk-a","configuration":{"id":"jk-a"},"status":{"state":"stopped"}},
            {"id":"jk-a-extra","configuration":{"id":"jk-a-extra"},"status":{"state":"stopped"}}
        ]"#,
        true,
    );

    let error = ensure_absent_for_purge_with(&client, "jk-a")
        .await
        .unwrap_err();

    assert!(error.to_string().contains("still exists"));
    assert_eq!(client.runner.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn purge_accepts_exact_absence_with_a_stopped_prefix_neighbor_from_production_listing() {
    let client = RunnerBackedClient::new(
        br#"[{"id":"jk-a-extra","configuration":{"id":"jk-a-extra"},"status":{"state":"stopped"}}]"#,
        true,
    );

    ensure_absent_for_purge_with(&client, "jk-a")
        .await
        .expect("prefix neighbor does not establish exact ID presence");

    assert_eq!(client.runner.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn reconnect_and_purge_errors_do_not_echo_untrusted_inventory_strings() {
    const CANARY: &str = "inventory-secret-canary-7bba";
    let unknown_state = format!(
        r#"[{{"id":"jk-a","configuration":{{"id":"jk-a"}},"status":{{"state":"{CANARY}"}}}}]"#
    );
    let reconnect_client = RunnerBackedClient::new(unknown_state.as_bytes(), true);
    let (events, effects) = effect_recorder();
    let reconnect_error = reconnect_with_client_and(&reconnect_client, "jk-a", effects)
        .await
        .unwrap_err();
    assert_redacted_inventory_error(&reconnect_error, CANARY);
    assert!(reconnect_client.start_calls.lock().unwrap().is_empty());
    assert!(events.borrow().is_empty());

    let malformed_row = format!(r#"["{CANARY}"]"#);
    let purge_client = RunnerBackedClient::new(malformed_row.as_bytes(), true);
    let purge_error = ensure_absent_for_purge_with(&purge_client, "jk-a")
        .await
        .unwrap_err();
    assert_redacted_inventory_error(&purge_error, CANARY);
}

fn assert_redacted_inventory_error(error: &anyhow::Error, canary: &str) {
    let display = format!("{error}");
    let alternate = format!("{error:#}");
    let debug = format!("{error:?}");
    assert!(!display.contains(canary), "Display leaked input: {display}");
    assert!(
        !alternate.contains(canary),
        "alternate Display leaked input: {alternate}"
    );
    assert!(!debug.contains(canary), "Debug leaked input: {debug}");
}
