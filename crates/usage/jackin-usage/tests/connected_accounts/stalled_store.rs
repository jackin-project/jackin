// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use jackin_core::{Provider, Secret};
use jackin_testkit::{FakeClock, Method, Route, UsageServer};
use jackin_usage::accounts::{AccountRegistry, CandidateIdentity, ConnectionError, UsageProvider};
use jackin_usage::connected::KeyConnection;
use jackin_usage::onboarding::{LoginError, LoginPrompt, OwnedLogin};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Result, Vault, bytes, db, scoped};

const PATIENCE: Duration = Duration::from_secs(5);

fn lock_file(directory: &std::path::Path) -> Result<std::fs::File> {
    let file = std::fs::File::create(directory.join("credentials.lock"))?;
    file.lock()?;
    Ok(file)
}

fn read_on_thread(
    path: &std::path::Path,
    vault: &Arc<Vault>,
    id: Uuid,
) -> mpsc::Receiver<std::result::Result<Vec<u8>, String>> {
    let (tx, rx) = mpsc::channel();
    let (path, vault) = (path.to_owned(), Arc::clone(vault));
    std::thread::spawn(move || {
        let read = AccountRegistry::with_vault(&path, vault)
            .and_then(|registry| registry.credential(id))
            .map(|(_, secret)| secret.expose().to_vec())
            .map_err(|e| e.to_string());
        let _sent = tx.send(read);
    });
    rx
}

fn slot_holding(vault: &Vault, value: &[u8]) -> Result<Uuid> {
    vault
        .values
        .lock()
        .map_err(|_| "vault poisoned")?
        .iter()
        .find(|(_, held)| held.as_slice() == value)
        .map(|(slot, _)| *slot)
        .ok_or_else(|| "no slot holds that value".into())
}

fn wait_for_stalled_read(vault: &Vault) -> Result<()> {
    let started = Instant::now();
    while vault.stalled_reads.load(Ordering::Acquire) == 0 {
        if started.elapsed() > PATIENCE {
            return Err("the stalled read never started".into());
        }
        std::thread::park_timeout(Duration::from_millis(10));
    }
    Ok(())
}

fn two_accounts(
    path: &std::path::Path,
    vault: &Arc<Vault>,
) -> Result<(AccountRegistry, Uuid, Uuid)> {
    let mut registry = db(path, vault)?;
    let first = registry.add(
        &jackin_core::CriticalWrite::begin(),
        UsageProvider::Claude,
        "first",
        &scoped("user/first"),
        &bytes("first-secret"),
    )?;
    let second = registry.add(
        &jackin_core::CriticalWrite::begin(),
        UsageProvider::Claude,
        "second",
        &scoped("user/second"),
        &bytes("second-secret"),
    )?;
    Ok((registry, first, second))
}

#[test]
fn a_stalled_keychain_read_leaves_other_credential_reads_unblocked() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let (_registry, first, second) = two_accounts(&path, &vault)?;
    vault.stall_reads_of(slot_holding(&vault, b"first-secret")?);
    let stalled = read_on_thread(&path, &vault, first);
    wait_for_stalled_read(&vault)?;
    let other = read_on_thread(&path, &vault, second).recv_timeout(PATIENCE);
    vault.release_stalled_reads();
    assert_eq!(
        other.map_err(|_| "a stalled read blocked an unrelated account")??,
        b"second-secret".to_vec()
    );
    assert_eq!(stalled.recv_timeout(PATIENCE)??, b"first-secret".to_vec());
    Ok(())
}

#[test]
fn credential_reads_do_not_wait_for_a_writer_holding_the_store_lock() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let (registry, first, _second) = two_accounts(&path, &vault)?;
    let held = lock_file(&temp.path().join("usage"))?;
    let read = read_on_thread(&path, &vault, first).recv_timeout(PATIENCE);
    drop(held);
    assert_eq!(
        read.map_err(|_| "a read waited for the store lock")??,
        b"first-secret".to_vec()
    );
    assert_eq!(registry.list()?.len(), 2);
    Ok(())
}

#[test]
fn a_read_that_races_a_rotation_returns_the_new_credential() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let (mut registry, first, _second) = two_accounts(&path, &vault)?;
    let old = registry.version(first)?;
    vault.stall_reads_of(slot_holding(&vault, b"first-secret")?);
    let (tx, rx) = mpsc::channel();
    let (reader_path, reader_vault) = (path.clone(), Arc::clone(&vault));
    std::thread::spawn(move || {
        let read = AccountRegistry::with_vault(&reader_path, reader_vault)
            .and_then(|registry| registry.credential(first))
            .map(|(version, secret)| (version, secret.expose().to_vec()))
            .map_err(|e| e.to_string());
        let _sent = tx.send(read);
    });
    wait_for_stalled_read(&vault)?;
    let rotated = registry.rotate(
        &jackin_core::CriticalWrite::begin(),
        first,
        old,
        &bytes("rotated"),
    )?;
    vault.release_stalled_reads();
    let (version, secret) = rx.recv_timeout(PATIENCE)??;
    assert_eq!(version, rotated);
    assert_eq!(secret, b"rotated".to_vec());
    Ok(())
}

#[test]
fn a_writer_waiting_on_a_busy_store_fails_with_a_message_naming_the_keychain() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let (registry, _first, _second) = two_accounts(&path, &vault)?;
    let mut registry = registry.with_lock_wait(Duration::from_millis(150));
    let held = lock_file(&temp.path().join("usage"))?;
    let started = Instant::now();
    let error = registry
        .add(
            &jackin_core::CriticalWrite::begin(),
            UsageProvider::Claude,
            "third",
            &scoped("user/third"),
            &bytes("third-secret"),
        )
        .err()
        .ok_or("add succeeded while the store lock was held")?;
    assert!(started.elapsed() < PATIENCE);
    assert!(matches!(error, ConnectionError::StoreBusy));
    assert!(error.to_string().contains("Keychain"));
    drop(held);
    assert_eq!(registry.list()?.len(), 2);
    registry.add(
        &jackin_core::CriticalWrite::begin(),
        UsageProvider::Claude,
        "third",
        &scoped("user/third"),
        &bytes("third-secret"),
    )?;
    assert_eq!(registry.list()?.len(), 3);
    Ok(())
}

#[test]
fn a_rotation_carrying_new_material_waits_for_the_store_lock_instead_of_dropping_it() -> Result<()>
{
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let (registry, first, _second) = two_accounts(&path, &vault)?;
    let old = registry.version(first)?;
    let held = lock_file(&temp.path().join("usage"))?;
    let (tx, rx) = mpsc::channel();
    let (writer_path, writer_vault) = (path.clone(), Arc::clone(&vault));
    std::thread::spawn(move || {
        let rotated = AccountRegistry::with_vault(&writer_path, writer_vault)
            .map(|registry| registry.with_lock_wait(Duration::from_millis(100)))
            .and_then(|mut registry| {
                registry.rotate(
                    &jackin_core::CriticalWrite::begin(),
                    first,
                    old,
                    &bytes("rotated"),
                )
            })
            .map_err(|e| e.to_string());
        let _sent = tx.send(rotated);
    });
    assert!(
        rx.recv_timeout(Duration::from_millis(600)).is_err(),
        "the rotation gave up while the store lock was held"
    );
    drop(held);
    let rotated = rx.recv_timeout(PATIENCE)??;
    let (version, secret) = registry.credential(first)?;
    assert_eq!(version, rotated);
    assert_eq!(secret.expose(), b"rotated");
    Ok(())
}

#[tokio::test]
async fn refresh_names_the_keychain_when_one_credential_read_stalls_and_the_rest_finish()
-> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let key = |name: &str| Secret::new(name.into());
    let (stuck_key, fine_key) = (key("stuck"), key("fine"));
    let stuck = registry.add(
        &jackin_core::CriticalWrite::begin(),
        UsageProvider::Zai,
        "stuck",
        &CandidateIdentity::api_key(&stuck_key),
        &KeyConnection::Zai.encode(&stuck_key)?,
    )?;
    let fine = registry.add(
        &jackin_core::CriticalWrite::begin(),
        UsageProvider::Zai,
        "fine",
        &CandidateIdentity::api_key(&fine_key),
        &KeyConnection::Zai.encode(&fine_key)?,
    )?;
    vault.stall_reads_of(slot_holding(
        &vault,
        KeyConnection::Zai.encode(&stuck_key)?.expose(),
    )?);
    let server = UsageServer::start().await?;
    server.serve(
        Provider::Zai,
        Method::Get,
        "/api/monitor/usage/quota/limit",
        Route::json(200,&json!({"success":true,"code":200,"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":10}]}}).to_string()).requires_header("authorization","Bearer fine"),
    );
    for (route, body) in [
        ("/api/biz/subscription/list", r#"{"data":[]}"#),
        (
            "/api/biz/customer-package-reset/list?targetType=PERSONAL",
            r#"{"data":{}}"#,
        ),
    ] {
        server.serve(Provider::Zai, Method::Get, route, Route::json(200, body));
    }
    let service = OwnedLogin::new(path, server.endpoints(), Arc::new(FakeClock::new()))
        .with_vault(Arc::<Vault>::clone(&vault))
        .with_store_wait(Duration::from_millis(400));
    let accounts = registry.list()?;
    let account = |id: Uuid| {
        accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or("missing account")
    };
    let cancel = CancellationToken::new();
    let started = Instant::now();
    let (stuck_account, fine_account) = (account(stuck)?, account(fine)?);
    let both = tokio::time::timeout(PATIENCE, async {
        tokio::join!(
            service.refresh(stuck_account, &cancel),
            service.refresh(fine_account, &cancel),
        )
    })
    .await;
    vault.release_stalled_reads();
    let (stuck_result, fine_result) = both.map_err(|_| "a stalled credential read hung refresh")?;
    assert!(started.elapsed() < PATIENCE);
    fine_result?;
    let error = stuck_result.err().ok_or("stalled refresh succeeded")?;
    assert!(matches!(error, LoginError::CredentialStoreTimeout));
    assert!(error.to_string().contains("Keychain"));
    let recorded = registry.failure(stuck)?.ok_or("failure not recorded")?;
    assert!(recorded.message.contains("Keychain"));
    assert!(registry.failure(fine)?.is_none());
    Ok(())
}

#[tokio::test]
async fn a_login_stores_its_issued_credentials_even_when_cancelled_while_the_keychain_write_is_pending()
-> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let registry = db(&path, &vault)?;
    let server = UsageServer::start().await?;
    let quota = "/api/monitor/usage/quota/limit";
    server.serve(
        Provider::Zai,
        Method::Get,
        quota,
        Route::json(200,&json!({"success":true,"code":200,"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":10}]}}).to_string())
            .requires_header("authorization", "Bearer issued"),
    );
    for (path, body) in [
        ("/api/biz/subscription/list", r#"{"data":[]}"#),
        (
            "/api/biz/customer-package-reset/list?targetType=PERSONAL",
            r#"{"data":{}}"#,
        ),
    ] {
        server.serve(Provider::Zai, Method::Get, path, Route::json(200, body));
    }
    let service = OwnedLogin::new(path, server.endpoints(), Arc::new(FakeClock::new()))
        .with_vault(Arc::<Vault>::clone(&vault));
    vault.stall_stores.store(true, Ordering::Release);
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let cancel = CancellationToken::new();
    let login = service.connect(UsageProvider::Zai, None, None, tx, &cancel);
    tokio::pin!(login);
    let prompt = tokio::select! {
        prompt = rx.recv() => prompt.ok_or("missing key prompt")?,
        result = &mut login => return Err(format!("login ended before the key: {result:?}").into()),
    };
    let LoginPrompt::Key { reply, .. } = prompt else {
        return Err("unexpected prompt".into());
    };
    reply
        .send(Secret::new("issued".into()))
        .map_err(|_| "key prompt closed")?;
    let started = Instant::now();
    while vault.stores_entered.load(Ordering::Acquire) == 0 {
        tokio::select! {
            result = &mut login => return Err(format!("login ended before the write: {result:?}").into()),
            () = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
        if started.elapsed() > PATIENCE {
            return Err("the credential write never began".into());
        }
    }
    cancel.cancel();
    vault.release_stalled_reads();
    let id = login.await?;
    assert_eq!(registry.list()?.len(), 1);
    assert_eq!(registry.list()?[0].id, id);
    assert_eq!(
        registry.credential(id)?.1.expose(),
        KeyConnection::Zai
            .encode(&Secret::new("issued".into()))?
            .expose()
    );
    Ok(())
}
