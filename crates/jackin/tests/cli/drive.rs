// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use jackin::WriteFate;
use jackin_core::{Clock, CriticalWrite, Provider, SecretBytes};
use jackin_testkit::{Method, Route, TestResult, UsageServer, World};
use jackin_usage::accounts::{AccountRegistry, CandidateIdentity, CredentialVault, UsageProvider};
use jackin_usage::onboarding::OwnedLogin;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::antigravity_refresh::Memory;
use super::support::Failure;

const BOUND: Duration = Duration::from_secs(4);
const CRITICAL_WORK: Duration = Duration::from_secs(3);
const STUCK_READ_ALLOWANCE: Duration = Duration::from_millis(3500);

#[test]
fn drive_returns_while_a_blocking_task_is_stuck() -> TestResult {
    let (release, parked) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<u8>();
    let driver = std::thread::spawn(move || {
        let output = jackin::drive(async move {
            let stuck = tokio::task::spawn_blocking(move || {
                started_tx.send(()).is_ok() && parked.recv().is_err()
            });
            let started = started_rx.recv().is_ok();
            let abandoned = tokio::time::timeout(Duration::from_millis(50), stuck)
                .await
                .is_err();
            u8::from(started && abandoned)
        });
        done_tx
            .send(output.map_or(u8::MAX, |driven| driven.output))
            .map_err(|_| "the test stopped listening")
    });
    let outcome = done_rx.recv_timeout(BOUND);
    drop(release);
    driver.join().map_err(|_| "driver thread panicked")??;
    assert_eq!(outcome, Ok(1), "drive waited for a stuck blocking task");
    Ok(())
}

#[test]
fn drive_lets_short_blocking_work_finish_before_it_returns() -> TestResult {
    let finished = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&finished);
    jackin::drive(async move {
        drop(tokio::task::spawn_blocking(move || {
            let (_hold, wait) = mpsc::channel::<()>();
            let waited = wait.recv_timeout(Duration::from_millis(300)).is_err();
            flag.store(waited, Ordering::SeqCst);
        }));
        tokio::task::yield_now().await;
    })?;
    assert!(
        finished.load(Ordering::SeqCst),
        "drive abandoned in-flight work"
    );
    Ok(())
}

#[test]
fn drive_waits_for_a_critical_write_but_abandons_a_stuck_read() -> TestResult {
    let (release, parked) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<Duration>();
    let driver = std::thread::spawn(move || {
        let started = Instant::now();
        let finished = jackin::drive(async move {
            drop(tokio::task::spawn_blocking(move || parked.recv().is_err()));
            let write = CriticalWrite::begin();
            drop(tokio::task::spawn_blocking(move || {
                pause(CRITICAL_WORK);
                drop(write);
            }));
            tokio::task::yield_now().await;
        });
        let elapsed = started.elapsed();
        done_tx
            .send(elapsed)
            .map_err(|_| "the test stopped listening")?;
        finished.map_err(|_| "drive failed")
    });
    let elapsed = done_rx.recv_timeout(Duration::from_secs(12));
    drop(release);
    driver.join().map_err(|_| "driver thread panicked")??;
    let elapsed = elapsed.map_err(|_| "drive waited for a stuck read")?;
    assert!(
        elapsed >= CRITICAL_WORK,
        "drive returned before the critical write finished: {elapsed:?}"
    );
    assert!(
        elapsed < CRITICAL_WORK + STUCK_READ_ALLOWANCE,
        "drive held the process for a stuck read: {elapsed:?}"
    );
    Ok(())
}

fn shared(vault: &Arc<Memory>) -> Arc<dyn CredentialVault> {
    Arc::<Memory>::clone(vault)
}

fn pause(length: Duration) {
    let (_hold, wait) = mpsc::channel::<()>();
    let _timed_out = wait.recv_timeout(length).is_err();
}

fn seed_expiring_claude_account(
    path: &std::path::Path,
    vault: &Arc<Memory>,
    now: i64,
) -> Result<(), Failure> {
    let mut registry = AccountRegistry::with_vault(path, shared(vault))?;
    registry.add_with_snapshot(
        &CriticalWrite::begin(),
        UsageProvider::Claude,
        "Work",
        &CandidateIdentity::OAuth(Some("member".into())),
        &SecretBytes::new(serde_json::to_vec(&json!([
            "old-access",
            "old-refresh",
            now - 10,
            null
        ]))?),
        (now - 60, &json!({"plan":"stale","resources":{}})),
    )?;
    Ok(())
}

#[test]
fn drive_lets_a_rotation_stuck_in_the_keychain_store_its_tokens() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.sqlite3");
    let vault = Arc::new(Memory::default());
    let world = World::builder().build()?;
    seed_expiring_claude_account(&path, &vault, world.clock.now().0)?;
    vault.stalling.store(true, Ordering::Release);
    let (service_path, service_vault) = (path.clone(), Arc::clone(&vault));
    let clock = Arc::clone(&world.clock);
    let started = Instant::now();
    jackin::drive(async move {
        let server = UsageServer::start().await?;
        server.serve(
            Provider::Claude,
            Method::Post,
            "/v1/oauth/token",
            Route::json(
                200,
                r#"{"access_token":"new-access","refresh_token":"new-refresh","expires_in":7200}"#,
            ),
        );
        let service = OwnedLogin::new(
            service_path.clone(),
            server.endpoints(),
            clock,
            shared(&service_vault),
        );
        let account = {
            AccountRegistry::with_vault(&service_path, shared(&service_vault))?
                .list()?
                .remove(0)
        };
        let cancel = CancellationToken::new();
        let write_began = async {
            while !service_vault.entered.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        };
        tokio::select! {
            biased;
            began = tokio::time::timeout(Duration::from_secs(3), write_began) => {
                began.map_err(|_| "the credential write never began")?;
                let releasing = Arc::clone(&service_vault);
                std::thread::spawn(move || {
                    pause(Duration::from_secs(4));
                    releasing.stalling.store(false, Ordering::Release);
                });
            }
            refreshed = service.refresh(&account, &cancel) => drop(refreshed),
        }
        Ok::<(), Failure>(())
    })?
    .output?;
    assert!(
        started.elapsed() > Duration::from_secs(3),
        "the rotation was not in flight when drive ended"
    );
    let registry = AccountRegistry::with_vault(&path, shared(&vault))?;
    let id = registry.list()?.remove(0).id;
    let (_, stored) = registry.credential(id)?;
    let text = String::from_utf8(stored.expose().to_vec())?;
    assert!(text.contains("new-access"), "the new tokens were lost");
    assert!(text.contains("new-refresh"), "the new tokens were lost");
    Ok(())
}

fn kill_self(signal: &str) -> bool {
    std::process::Command::new("kill")
        .args([signal, &std::process::id().to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn drive_abandons_a_stuck_write_when_interrupted_during_the_wait() -> TestResult {
    let (release, parked) = mpsc::channel::<()>();
    let started = Instant::now();
    let driven = jackin::drive(async move {
        let write = CriticalWrite::begin();
        drop(tokio::task::spawn_blocking(move || {
            let _held = parked.recv().is_err();
            drop(write);
        }));
        drop(std::thread::spawn(|| {
            pause(Duration::from_millis(1500));
            kill_self("-INT")
        }));
    })?;
    drop(release);
    assert_eq!(driven.writes, WriteFate::Abandoned);
    assert!(started.elapsed() < Duration::from_secs(10));
    Ok(())
}

#[test]
fn drive_does_not_replay_the_interrupt_that_ended_the_run() -> TestResult {
    let driven = jackin::drive(async move {
        let write = CriticalWrite::begin();
        drop(tokio::task::spawn_blocking(move || {
            pause(Duration::from_millis(1500));
            drop(write);
        }));
        let sent = tokio::task::spawn_blocking(|| kill_self("-INT")).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        sent.unwrap_or(false)
    })?;
    assert!(driven.output, "the interrupt was not sent");
    assert_eq!(driven.writes, WriteFate::Settled);
    Ok(())
}
