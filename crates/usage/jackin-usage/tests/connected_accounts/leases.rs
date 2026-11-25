// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_core::{Provider, Secret};
use jackin_testkit::{FakeClock, Method, Route, UsageServer};
use jackin_usage::accounts::{CandidateIdentity, ConnectionError, UsageProvider};
use jackin_usage::connected::KeyConnection;
use jackin_usage::onboarding::{LoginError, LoginPrompt, OwnedLogin};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::{Result, Vault, db};

#[tokio::test]
async fn cancelling_refresh_during_key_prompt_preserves_last_snapshot_and_new_key_wins()
-> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let old = Secret::new("first".into());
    let initial = json!({"resources":{"session":{"used":2}}});
    let id = registry.add_with_snapshot(
        &jackin_core::CriticalWrite::begin(),
        UsageProvider::Zai,
        "work",
        &CandidateIdentity::api_key(&old),
        &KeyConnection::Zai.encode(&old)?,
        (10, &initial),
    )?;
    let server = UsageServer::start().await?;
    let service = OwnedLogin::new(path, server.endpoints(), Arc::new(FakeClock::new()), vault);
    let quota = "/api/monitor/usage/quota/limit";
    let reading = |used: u32, key: &str| {
        Route::json(200,&json!({"success":true,"code":200,"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":used}]}}).to_string()).requires_header("authorization",key)
    };
    server.serve(
        Provider::Zai,
        Method::Get,
        quota,
        reading(10, "Bearer first"),
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
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let replacement_cancel = CancellationToken::new();
    let replacement = service.connect(UsageProvider::Zai, None, Some(id), tx, &replacement_cancel);
    tokio::pin!(replacement);
    let prompt = tokio::select! {
        prompt=rx.recv()=>prompt.ok_or("missing key prompt")?,
        result=&mut replacement=>return Err(format!("replacement completed before input: {result:?}").into()),
    };
    let LoginPrompt::Key { reply, .. } = prompt else {
        return Err("unexpected replacement prompt".into());
    };
    let account = registry.list()?.pop().ok_or("missing account")?;
    let cancel = CancellationToken::new();
    let refresh = service.refresh(&account, &cancel);
    tokio::pin!(refresh);
    assert!(
        tokio::time::timeout(Duration::from_millis(300), &mut refresh)
            .await
            .is_err(),
        "refresh completed while the replacement key prompt was open"
    );
    cancel.cancel();
    assert!(matches!(refresh.await, Err(LoginError::Cancelled)));
    assert_eq!(registry.snapshots()?[0].2.as_ref(), Some(&initial));
    assert_eq!(
        registry.credential(id)?.1.expose(),
        KeyConnection::Zai.encode(&old)?.expose()
    );
    server.replace(
        Provider::Zai,
        Method::Get,
        quota,
        reading(20, "Bearer second"),
    );
    reply
        .send(Secret::new("second".into()))
        .map_err(|_| "key prompt closed")?;
    assert_eq!(replacement.await?, id);
    server.replace(
        Provider::Zai,
        Method::Get,
        quota,
        reading(35, "Bearer second"),
    );
    service.refresh(&account, &CancellationToken::new()).await?;
    let (_, _, result) = registry
        .snapshots()?
        .pop()
        .ok_or("missing updated account")?;
    assert_eq!(
        result.ok_or("missing refreshed quota")?["resources"]["session"]["used"],
        35.0
    );
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn hub_sync_requires_a_vault_key_bound_to_the_saved_authority() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let source = UsageServer::start().await?;
    let other = UsageServer::start().await?;
    let service = OwnedLogin::new(
        path.clone(),
        source.endpoints(),
        Arc::new(FakeClock::new()),
        vault,
    );
    let listing = "/v0/management/auth-files";
    source.serve(
        Provider::Claude,
        Method::Get,
        listing,
        Route::json(200, r#"{"files":[]}"#)
            .requires_header("authorization", "Bearer synthetic-hub-key"),
    );
    let origin = source.provider_base(Provider::Claude);
    let cancel = CancellationToken::new();
    let id = service
        .connect_hub(
            &origin,
            &Secret::new("synthetic-hub-key".into()),
            None,
            &cancel,
        )
        .await?;
    let metadata = rusqlite::Connection::open(path)?;
    let address = |v: &str| {
        metadata.execute(
            "UPDATE hubs SET address=?1 WHERE id=?2",
            rusqlite::params![v, id.to_string()],
        )
    };
    address(&other.provider_base(Provider::Claude))?;
    assert!(matches!(
        service.sync_hub(id, &cancel).await,
        Err(LoginError::Store(ConnectionError::Invalid))
    ));
    assert!(
        other.mismatches().is_empty(),
        "unexpected request to the second origin"
    );
    address(&origin)?;
    service.sync_hub(id, &cancel).await?;
    assert!(source.mismatches().is_empty());
    Ok(())
}
