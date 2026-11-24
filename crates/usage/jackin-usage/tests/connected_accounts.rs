// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::error::Error;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use jackin_core::{Secret, SecretBytes};
use jackin_usage::accounts::{
    AccountPreferences, AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault,
    UsageProvider,
};
use serde_json::json;
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Default, Debug)]
struct Vault {
    values: Mutex<HashMap<Uuid, Vec<u8>>>,
    lock_write: AtomicBool,
    lock_delete: AtomicBool,
    cancel_store: Mutex<Option<tokio_util::sync::CancellationToken>>,
}

impl CredentialVault for Vault {
    fn store(&self, slot: Uuid, value: &SecretBytes) -> std::result::Result<(), ConnectionError> {
        if self.lock_write.load(Ordering::Acquire) {
            return Err(ConnectionError::Vault);
        }
        self.values
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .insert(slot, value.expose().to_vec());
        if let Some(token) = self
            .cancel_store
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .take()
        {
            token.cancel();
        }
        Ok(())
    }
    fn load(&self, slot: Uuid) -> std::result::Result<SecretBytes, ConnectionError> {
        self.values
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .get(&slot)
            .cloned()
            .map(SecretBytes::new)
            .ok_or(ConnectionError::MissingCredential)
    }
    fn delete(&self, slot: Uuid) -> std::result::Result<(), ConnectionError> {
        if self.lock_delete.load(Ordering::Acquire) {
            return Err(ConnectionError::Vault);
        }
        self.values
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .remove(&slot);
        Ok(())
    }
}

fn bytes(s: &str) -> SecretBytes {
    SecretBytes::new(s.as_bytes().to_owned())
}
fn scoped(s: &str) -> CandidateIdentity {
    CandidateIdentity::OAuth(Some(s.to_owned()))
}
fn db(path: &Path, v: &Arc<Vault>) -> Result<AccountRegistry> {
    Ok(AccountRegistry::with_vault(path, Arc::<Vault>::clone(v))?)
}

#[test]
fn reconnect_preserves_unique_scoped_accounts_and_settings() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let a = scoped("user/org-a");
    let b = scoped("user/org-b");
    let secret = bytes("fixture");
    let id = registry.add_with_snapshot(
        UsageProvider::Claude,
        "same",
        &a,
        &secret,
        5,
        &json!({"left":0}),
    )?;
    let other = registry.add(UsageProvider::Claude, "same", &b, &secret)?;
    assert!(matches!(
        registry.add(UsageProvider::Claude, "other", &a, &secret),
        Err(ConnectionError::Duplicate)
    ));
    registry.rename(id, "my account")?;
    let prefs = AccountPreferences {
        pinned: ["weekly".to_owned()].into(),
        ..AccountPreferences::default()
    };
    registry.set_preferences(id, &prefs)?;
    registry.reorder(&[other, id])?;
    let old = registry.version(id)?;
    assert!(matches!(
        registry.replace(id, old, &b, &secret),
        Err(ConnectionError::DifferentIdentity)
    ));
    vault.lock_write.store(true, Ordering::Release);
    assert!(matches!(
        registry.replace(id, old, &a, &bytes("second")),
        Err(ConnectionError::Vault)
    ));
    assert!(matches!(
        registry.add(UsageProvider::Claude, "third", &scoped("x"), &secret),
        Err(ConnectionError::Vault)
    ));
    assert_eq!(registry.credential(id)?.1.expose(), secret.expose());
    assert_eq!(registry.snapshots()?[1].2, Some(json!({"left":0})));
    vault.lock_write.store(false, Ordering::Release);
    vault.values.lock().map_err(|_| "vault poisoned")?.clear();
    assert!(matches!(
        registry.credential(id),
        Err(ConnectionError::MissingCredential)
    ));
    registry.replace_with_snapshot(
        id,
        registry.version(id)?,
        &a,
        &bytes("new"),
        6,
        &json!({"left":10}),
    )?;
    let read = AccountRegistry::open_read_only(&path)?.ok_or("missing database")?;
    let rows = read.snapshots()?;
    assert_eq!(rows.len(), 2);
    let (account, time, snapshot) = &rows[1];
    assert_eq!(
        (
            account.id,
            account.label.as_str(),
            account.position,
            &account.preferences
        ),
        (id, "my account", 1, &prefs)
    );
    assert_eq!(
        (*time, snapshot.as_ref()),
        (Some(6), Some(&json!({"left":10})))
    );
    assert_eq!(registry.credential(id)?.1.expose(), b"new");
    Ok(())
}

#[test]
fn key_replacement_rejects_duplicates_and_retires_old_slots() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let a = CandidateIdentity::api_key(&Secret::new("fixture-a".into()));
    let b = CandidateIdentity::api_key(&Secret::new("fixture-b".into()));
    let c = CandidateIdentity::api_key(&Secret::new("fixture-c".into()));
    let id = registry.add(UsageProvider::OpenRouter, "A", &a, &bytes("first"))?;
    registry.add(UsageProvider::OpenRouter, "B", &b, &bytes("other"))?;
    let generation = registry.version(id)?;
    assert!(matches!(
        registry.replace(id, generation, &b, &bytes("second")),
        Err(ConnectionError::Duplicate)
    ));
    assert!(matches!(
        registry.replace(id, generation, &scoped("x"), &bytes("second")),
        Err(ConnectionError::CredentialKind)
    ));
    vault.lock_delete.store(true, Ordering::Release);
    registry.replace(id, generation, &c, &bytes("second"))?;
    assert_eq!(registry.credential(id)?.1.expose(), b"second");
    drop(registry);
    vault.lock_delete.store(false, Ordering::Release);
    let mut again = db(&path, &vault)?;
    again.clean_retired()?;
    assert_eq!(vault.values.lock().map_err(|_| "vault poisoned")?.len(), 2);
    assert!(matches!(
        again.rotate(id, generation, &bytes("late")),
        Err(ConnectionError::Changed)
    ));
    let live = again.version(id)?;
    again.remove(id)?;
    assert!(matches!(
        again.put_snapshot(id, live, 10, &json!({})),
        Err(ConnectionError::Changed)
    ));
    assert!(matches!(
        again.rotate(id, live, &bytes("late")),
        Err(ConnectionError::Changed)
    ));
    assert_eq!(again.snapshots()?.len(), 1);
    assert_eq!(vault.values.lock().map_err(|_| "vault poisoned")?.len(), 1);
    Ok(())
}

#[test]
fn offline_reader_leaves_missing_state_absent_and_rejects_symlinks() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let dir = temp.path().join("usage");
    let path = dir.join("accounts.db");
    assert!(AccountRegistry::open_read_only(&path)?.is_none());
    assert!(!dir.exists());
    let registry = AccountRegistry::open(&path)?;
    assert!(registry.list()?.is_empty());
    assert_eq!(path.metadata()?.permissions().mode() & 0o777, 0o600);
    assert_eq!(dir.metadata()?.permissions().mode() & 0o777, 0o700);
    let link = dir.join("other.db");
    symlink(&path, &link)?;
    AccountRegistry::open_read_only(&link).expect_err("read-only open must reject symlinks");
    AccountRegistry::open(&link).expect_err("read-write open must reject symlinks");
    Ok(())
}

#[test]
fn cancellation_during_vault_prompt_rolls_back_candidate() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let identity = scoped("member/org");
    let old = bytes("original");
    let cancel = tokio_util::sync::CancellationToken::new();
    *vault.cancel_store.lock().map_err(|_| "vault poisoned")? = Some(cancel.clone());
    assert!(matches!(
        registry.add_candidate(
            UsageProvider::Claude,
            "label",
            &identity,
            &old,
            jackin_usage::accounts::InitialRead::Pending,
            &cancel
        ),
        Err(ConnectionError::Cancelled)
    ));
    assert!(registry.list()?.is_empty());
    let id = registry.add(UsageProvider::Claude, "label", &identity, &old)?;
    let cancel = tokio_util::sync::CancellationToken::new();
    *vault.cancel_store.lock().map_err(|_| "vault poisoned")? = Some(cancel.clone());
    assert!(matches!(
        registry.replace_candidate(
            id,
            registry.version(id)?,
            &identity,
            &bytes("candidate"),
            None,
            &cancel
        ),
        Err(ConnectionError::Cancelled)
    ));
    assert_eq!(registry.credential(id)?.1.expose(), old.expose());
    assert_eq!(vault.values.lock().map_err(|_| "vault poisoned")?.len(), 1);
    Ok(())
}

#[path = "dashboard/tests.rs"]
mod dashboard_tests;

#[path = "connected_accounts/reset_actions.rs"]
mod reset_actions;

#[tokio::test]
async fn hub_reconciliation_preserves_local_preferences_and_filters_upstream_logins() -> Result<()>
{
    use jackin_core::Provider;
    use jackin_testkit::{FakeClock, Method, Route, UsageServer};
    use jackin_usage::onboarding::OwnedLogin;
    use tokio_util::sync::CancellationToken;
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let direct = registry.add(
        UsageProvider::Claude,
        "same",
        &scoped("direct"),
        &bytes("fixture"),
    )?;
    let server = UsageServer::start().await?;
    let service = OwnedLogin::new(path.clone(), server.endpoints(), Arc::new(FakeClock::new()))
        .with_vault(Arc::<Vault>::clone(&vault));
    let listing = "/v0/management/auth-files";
    let valid=Route::json(200,r#"{"files":[{"id":"local","provider":"claude","auth_index":"1","email":"same"},{"id":"hidden","provider":"antigravity","auth_index":"2"},{"id":"disabled","provider":"codex","auth_index":"3","disabled":true},{"id":"no-key","provider":"xai"}]}"#).requires_header("authorization","Bearer fixture");
    server.serve(Provider::Claude, Method::Get, listing, valid);
    let cancel = CancellationToken::new();
    let id = service
        .connect_hub(
            &format!("{}/management.html", server.provider_base(Provider::Claude)),
            &Secret::new("fixture".into()),
            None,
            &cancel,
        )
        .await?;
    let rows = registry.list()?;
    assert_eq!(rows.len(), 2);
    let mirror = rows
        .iter()
        .find(|r| r.id != direct)
        .ok_or("missing mirror")?
        .id;
    registry.rename(mirror, "my hub account")?;
    assert!(matches!(
        registry.remove(mirror),
        Err(ConnectionError::HubManaged)
    ));
    server.replace(
        Provider::Claude,
        Method::Get,
        listing,
        Route::json(
            200,
            r#"{"files":[{"id":"local","provider":"claude","email":"changed"}]}"#,
        ),
    );
    service.sync_hub(id, &cancel).await?;
    assert_eq!(registry.hub_link(mirror)?.ok_or("lost mirror")?.index, "1");
    server.replace(
        Provider::Claude,
        Method::Get,
        listing,
        Route::json(403, "{}"),
    );
    service
        .sync_hub(id, &cancel)
        .await
        .expect_err("sync must fail when the hub refuses the request");
    assert!(matches!(
        registry.hub_key(id),
        Err(ConnectionError::HubRefused)
    ));
    assert_eq!(registry.list()?.len(), 2);
    let old = registry.hub_version(id)?;
    let address = server.provider_base(Provider::Claude);
    service
        .connect_hub(
            &address,
            &Secret::new("replacement".into()),
            Some(id),
            &cancel,
        )
        .await
        .expect_err("a refused replacement hub must not replace the existing link");
    assert_eq!(registry.hub_version(id)?, old);
    server.replace(
        Provider::Claude,
        Method::Get,
        listing,
        Route::json(
            200,
            r#"{"files":[{"id":"local","provider":"claude","auth_index":"new"}]}"#,
        )
        .requires_header("authorization", "Bearer replacement"),
    );
    service
        .connect_hub(
            &address,
            &Secret::new("replacement".into()),
            Some(id),
            &cancel,
        )
        .await?;
    assert_ne!(registry.hub_version(id)?, old);
    assert_eq!(
        registry.hub_link(mirror)?.ok_or("lost mirror")?.index,
        "new"
    );
    assert_eq!(
        registry
            .list()?
            .iter()
            .find(|r| r.id == mirror)
            .ok_or("missing")?
            .label,
        "my hub account"
    );
    server.replace(
        Provider::Claude,
        Method::Get,
        listing,
        Route::json(200, r#"{"files":[]}"#),
    );
    service.sync_hub(id, &cancel).await?;
    registry.remove_hub(id)?;
    assert_eq!(
        registry.list()?.iter().map(|r| r.id).collect::<Vec<_>>(),
        [direct]
    );
    Ok(())
}

#[test]
fn retry_after_holds_manual_refresh_and_stale_failures_cannot_restore_a_connection() -> Result<()> {
    use jackin_usage::accounts::FailureKind;
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let identity = scoped("subject/org");
    let id = registry.add_with_snapshot(
        UsageProvider::Claude,
        "fixture",
        &identity,
        &bytes("first"),
        50,
        &serde_json::json!({"available":0}),
    )?;
    let v = registry.version(id)?;
    registry.record_failure(
        id,
        v,
        60,
        FailureKind::RateLimited,
        Some(180),
        "rate limited",
    )?;
    let reopened = db(&path, &vault)?;
    let status = reopened.failure(id)?.ok_or("missing hold")?;
    assert!(status.holds(239, true));
    assert!(!status.holds(240, true));
    assert_eq!(reopened.snapshots()?[0].1, Some(50));
    registry.put_snapshot(id, v, 242, &serde_json::json!({"available":3}))?;
    assert!(registry.failure(id)?.is_none());
    registry.replace(id, v, &identity, &bytes("replacement"))?;
    assert!(
        registry
            .record_failure(id, v, 250, FailureKind::Authentication, None, "old")
            .is_err()
    );
    registry.remove(id)?;
    assert!(
        registry
            .put_snapshot(id, v, 251, &serde_json::json!({}))
            .is_err()
    );
    assert!(registry.snapshots()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn hub_transport_limits_upstream_requests_to_the_mirrored_provider() -> Result<()> {
    use jackin_core::Provider;
    use jackin_testkit::{Method, Route, UsageServer};
    use jackin_usage_provider_core::{Auth, Http, HubAddress, HubProvider};
    for address in ["http://example.org", "http://100.128.0.1", "file:///etc"] {
        HubAddress::parse(address).expect_err("unsafe hub addresses must be rejected");
    }
    assert_eq!(
        HubAddress::parse("example.org/a/management.html?x=1")?.as_str(),
        "https://example.org/a"
    );
    let server = UsageServer::start().await?;
    let http = Http::new(std::time::Duration::from_secs(3)).routed_via_hub(
        HubAddress::parse(&server.provider_base(Provider::Claude))?,
        "only-hub".into(),
        "index".into(),
        HubProvider::Claude,
    );
    server.serve(
        Provider::Claude,
        Method::Post,
        "/v0/management/api-call",
        Route::json(
            200,
            r#"{"status_code":429,"header":{"Retry-After":["17"]},"body":"{}"}"#,
        )
        .requires_header("authorization", "Bearer only-hub")
        .matching_json(|v| {
            v["url"] == "https://api.anthropic.com/api/oauth/usage"
                && v["method"] == "GET"
                && v["header"]["Authorization"] == "Bearer $TOKEN$"
                && v["auth_index"] == "index"
        }),
    );
    let reply = http
        .get_reply(
            "https://api.anthropic.com/api/oauth/usage",
            Auth::Bearer("unused"),
            &[],
        )
        .await?;
    assert_eq!(reply.status, 429);
    assert_eq!(reply.retry_after_seconds, Some(17));
    for url in [
        "https://api.anthropic.com.example.org/api/oauth/usage",
        "https://chatgpt.com/backend-api/wham/usage",
        "http://api.anthropic.com/api/oauth/usage",
    ] {
        http.get_reply(url, Auth::Bearer("unused"), &[])
            .await
            .expect_err("hub transport must reject requests outside its provider");
    }
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[path = "connected_accounts/antigravity.rs"]
mod antigravity;
#[path = "connected_accounts/antigravity_direct.rs"]
mod antigravity_direct;
#[path = "connected_accounts/leases.rs"]
mod leases;

#[path = "connected_accounts/claude_budget.rs"]
mod claude_budget_tests;

#[path = "connected_accounts/regional_device.rs"]
mod regional_device;

#[test]
fn openrouter_account_details_explain_key_scope_in_the_console_and_cli() -> Result<()> {
    use jackin_testkit::FakeClock;
    use jackin_usage::{ProviderEndpoints, export, onboarding::OwnedLogin, presentation};

    for (key_type, console, expected) in [
        (
            Some("standard"),
            vec!["standard key; workspace balance needs a management key"],
            vec![
                "Key type · standard",
                "Workspace balance and activity history require a management key.",
            ],
        ),
        (
            Some("management"),
            vec!["management key"],
            vec!["Key type · management"],
        ),
        (None, vec![], vec![]),
    ] {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("usage/accounts.db");
        let vault = Arc::new(Vault::default());
        let mut registry = db(&path, &vault)?;
        let login = OwnedLogin::new(path, ProviderEndpoints::real(), Arc::new(FakeClock::new()))
            .with_vault(vault);
        let mut snapshot = json!({"resources":{}});
        if let Some(kind) = key_type {
            snapshot["key_type"] = json!(kind);
        }
        registry.add_with_snapshot(
            UsageProvider::OpenRouter,
            "Work",
            &CandidateIdentity::api_key(&Secret::new("fixture".into())),
            &bytes("fixture"),
            login.now().0,
            &snapshot,
        )?;
        let entry = presentation::entries(&login)?
            .pop()
            .ok_or("missing console account")?;
        let snapshot = registry.snapshots()?.pop().ok_or("missing snapshot")?;
        let cli = export::details_account(
            snapshot,
            0,
            &registry.dashboard_settings()?,
            None,
            login.now(),
        );
        let cli_lines: Vec<String> = serde_json::from_value(cli["details"].clone())?;
        for (lines, expected) in [(&entry.details, &console), (&cli_lines, &expected)] {
            let scope: Vec<&str> = lines
                .iter()
                .map(String::as_str)
                .map(str::trim)
                .filter(|line| line.starts_with("Key type") || line.contains("management key"))
                .collect();
            assert_eq!(&scope, expected, "{key_type:?}: {lines:?}");
        }
    }
    Ok(())
}

#[test]
fn usage_inspectors_keep_account_quota_and_leave_history_in_details() -> Result<()> {
    use jackin_testkit::FakeClock;
    use jackin_usage::dashboard::{Dashboard, DashboardMemory};
    use jackin_usage::{ProviderEndpoints, onboarding::OwnedLogin, presentation};

    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let login = OwnedLogin::new(path, ProviderEndpoints::real(), Arc::new(FakeClock::new()))
        .with_vault(vault);
    let now = login.now().0;
    for (label, remaining, reset) in [("Work", 12, 3600), ("Personal", 100, 7200)] {
        registry.add_with_snapshot(
            UsageProvider::Claude,
            label,
            &CandidateIdentity::OAuth(Some(label.to_owned())),
            &bytes(label),
            now,
            &json!({
                "resources": {
                    "session": {"kind":"quota", "remaining_percent":remaining, "resetsAt":now + reset},
                    "webSearches": {"kind":"quota", "unit":"searches", "used":0, "limit":50},
                    "credits": {"kind":"balance", "unit":"credits"},
                },
                "history": {"ranges": {"today": {"totals": {"tokens": 100, "inputTokens": 50}}}},
            }),
        )?;
    }

    let entries = presentation::entries(&login)?;
    for (label, expected_session) in [
        ("Work", "Session 88% used · resets in 1h 0m"),
        ("Personal", "Session 0% used · resets in 2h 0m"),
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry.label == label)
            .ok_or(label)?;
        assert!(
            entry
                .summary
                .iter()
                .any(|line| line.starts_with(expected_session)),
            "{:?}",
            entry.summary
        );
        assert!(entry.summary.iter().all(|line| !line.contains("tokens")));
        for line in [
            "Web searches",
            "0 of 50 searches used",
            "per-day counts: jackin usage details --json",
        ] {
            assert!(
                entry.details.iter().any(|found| found.contains(line)),
                "{line}: {:?}",
                entry.details
            );
        }
    }

    let data = Dashboard::load(&registry, &mut DashboardMemory::default(), now)?;
    let metric = |key| data.all.iter().find(|row| row.metric == key).ok_or(key);
    assert!(presentation::metric_inspector(metric("session")?).is_empty());
    assert_eq!(
        presentation::metric_inspector(metric("webSearches")?),
        ["Limit · 50 searches"]
    );
    assert!(presentation::metric_inspector(metric("today")?).is_empty());
    assert!(
        presentation::metric_details(metric("today")?, false)
            .iter()
            .any(|line| line == "50 input tokens")
    );

    Ok(())
}

#[test]
fn usage_overview_compacts_large_known_quantities_and_details_keep_exact_values() -> Result<()> {
    use jackin_testkit::FakeClock;
    use jackin_usage::dashboard::{Dashboard, DashboardMemory};
    use jackin_usage::{ProviderEndpoints, export, onboarding::OwnedLogin, presentation};

    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let vault = Arc::new(Vault::default());
    let mut registry = db(&path, &vault)?;
    let login = OwnedLogin::new(path, ProviderEndpoints::real(), Arc::new(FakeClock::new()))
        .with_vault(vault);
    let now = login.now();
    registry.add_with_snapshot(
        UsageProvider::Mimo,
        "Work",
        &CandidateIdentity::api_key(&Secret::new("fixture".into())),
        &bytes("fixture"),
        now.0,
        &json!({"resources": {
            "tokenPlan": {"kind":"quota", "unit":"credits", "used":1, "limit":50_474_251_079_i64, "available":50_474_251_078_i64},
            "booster": {"kind":"balance", "unit":"cny", "exponent":6, "available":125},
            "monthlyChargeUsed": {"kind":"amount", "unit":"usd", "amount":999_994_000},
            "monthlyChargeLimit": {"kind":"amount", "unit":"usd", "amount":999_996_000},
            "webSearches": {"kind":"amount", "unit":"provider units", "amount":999_996_000},
            "session": {"kind":"window", "remaining_percent":99.5},
            "weekly": {"kind":"window", "remaining_percent":0.5},
        }}),
    )?;
    let board = Dashboard::load(&registry, &mut DashboardMemory::default(), now.0)?;
    for (id, compact, exact) in [
        ("tokenPlan", "1 credits used", "1 credits used"),
        ("booster", "0.000125 cny", "0.000125 cny"),
        ("monthlyChargeUsed", "999.99M usd", "999994000 usd"),
        ("monthlyChargeLimit", "1B usd", "999996000 usd"),
        (
            "webSearches",
            "999996000 provider units",
            "999996000 provider units",
        ),
        ("session", "<1% used", "<1% used"),
        ("weekly", ">99% used", ">99% used"),
    ] {
        let row = board.all.iter().find(|row| row.metric == id).ok_or(id)?;
        assert_eq!(presentation::metric_value(row, false), compact);
        assert_eq!(presentation::exact_metric_value(row, false), exact);
    }
    let mut orders = AccountPreferences::default();
    orders.metric_order.push("tokenPlan".into());
    let snapshot = registry.snapshots()?.pop().ok_or("missing snapshot")?;
    registry.set_preferences(snapshot.0.id, &orders)?;
    let entry = presentation::entries(&login)?
        .pop()
        .ok_or("missing account")?;
    assert!(
        entry
            .summary
            .iter()
            .any(|line| line.starts_with("Token Plan <1% used"))
    );
    let cli = export::details_account(snapshot, 0, &registry.dashboard_settings()?, None, now);
    let cli_lines: Vec<String> = serde_json::from_value(cli["details"].clone())?;
    assert!(
        cli_lines
            .iter()
            .any(|line| line == "Token Plan · 1 credits")
    );
    assert!(
        cli_lines
            .iter()
            .any(|line| line == "Remaining · 50474251078 credits")
    );
    Ok(())
}
