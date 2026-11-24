// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{Clock, Secret};
use jackin_testkit::FakeClock;
use jackin_usage::ProviderEndpoints;
use jackin_usage::accounts::{AccountRegistry, ConnectionError, UsageProvider};
use jackin_usage::onboarding::{
    LoginError, LoginPrompt, OwnedLogin, receive_antigravity_statusline,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::Result;

fn update(service: &OwnedLogin, email: &str, plan: Option<&str>, at: i64) -> Result<bool> {
    let input = json!({"product":"antigravity","email":email,"plan_tier":plan,"quota":{"3p-weekly":{"remaining_fraction":0.05}}});
    Ok(receive_antigravity_statusline(
        service.database(),
        &serde_json::to_vec(&input)?,
        at,
    )?)
}

async fn begin(
    service: &OwnedLogin,
    email: &str,
    replace: Option<Uuid>,
    cancel: &CancellationToken,
) -> Result<tokio::task::JoinHandle<std::result::Result<Uuid, LoginError>>> {
    let service = service.clone();
    let token = cancel.clone();
    let (send, mut prompts) = tokio::sync::mpsc::channel(2);
    let task = tokio::spawn(async move {
        service
            .connect_antigravity_statusline(replace, send, &token)
            .await
    });
    let Some(LoginPrompt::Field { reply, .. }) = prompts.recv().await else {
        return Err("missing email prompt".into());
    };
    reply
        .send(Secret::new(email.into()))
        .map_err(|_| "closed prompt")?;
    let Some(LoginPrompt::AntigravityStatusLine { command }) = prompts.recv().await else {
        return Err("missing status-line command".into());
    };
    assert!(command.contains("usage antigravity-statusline"));
    Ok(task)
}

#[tokio::test]
async fn a_stored_statusline_report_is_not_a_network_refresh() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("usage/accounts.sqlite3");
    let clock = Arc::new(FakeClock::new());
    let observed = clock.now().0;
    let service = OwnedLogin::new(path.clone(), ProviderEndpoints::real(), clock);
    let cancel = CancellationToken::new();
    let email = "cached@example.com";
    let pending = begin(&service, email, None, &cancel).await?;
    assert!(update(&service, email, Some("ultra"), observed)?);
    let id = pending.await??;
    let db = AccountRegistry::open(&path)?;
    let account = db
        .list()?
        .into_iter()
        .find(|row| row.id == id)
        .ok_or("missing account")?;
    let before = db
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    let manual = service.refresh(&account, &cancel).await;
    let scheduled = service.refresh_if_due(&account, false, &cancel).await;
    for result in [&manual, &scheduled] {
        let error = result
            .as_ref()
            .expect_err("a stored report is not a network refresh");
        let text = error.to_string();
        assert!(
            !text.contains("last report"),
            "the stored report must not be presented as a new reading"
        );
    }
    let after = db
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    assert_eq!(before.1, after.1);
    assert_eq!(before.2, after.2);
    assert!(db.failure(id)?.is_none());
    assert_eq!(after.1, Some(observed));
    Ok(())
}

#[tokio::test]
async fn only_opted_in_reports_create_and_update_accounts() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("usage/accounts.sqlite3");
    let clock = Arc::new(FakeClock::new());
    let now = clock.now().0;
    let service = OwnedLogin::new(path.clone(), ProviderEndpoints::real(), clock);
    let first = "first@example.com";
    let second = "second@example.com";
    assert!(!update(&service, first, None, now)?);
    assert!(!path.exists());
    let cancel = CancellationToken::new();
    let pending = begin(&service, first, None, &cancel).await?;
    assert!(!update(&service, second, None, now)?);
    cancel.cancel();
    assert!(matches!(pending.await?, Err(LoginError::Cancelled)));
    let mut db = AccountRegistry::open(&path)?;
    assert!(db.list()?.is_empty());
    assert!(!update(&service, first, None, now + 1)?);

    let cancel = CancellationToken::new();
    let a = begin(&service, first, None, &cancel).await?;
    let b = begin(&service, second, None, &cancel).await?;
    assert!(!update(&service, first, None, now + 601)?);
    assert!(update(&service, second, Some("pro"), now)?);
    assert!(update(&service, "FIRST@example.com", Some("ultra"), now)?);
    let id = a.await??;
    let other = b.await??;
    assert_ne!(id, other);
    db.rename(id, "Personal")?;
    assert!(update(&service, first, None, now + 10)?);
    assert!(!update(&service, first, Some("pro"), now + 1)?);
    let row = db
        .list()?
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("missing account")?;
    let refresh = service.refresh(&row, &cancel).await;
    let Err(LoginError::NotDirect) = &refresh else {
        return Err("expected a status-line refusal".into());
    };
    let text = LoginError::NotDirect.to_string();
    assert!(text.contains("status-line hook"), "{text}");
    assert!(!text.contains("no direct sign-in"), "{text}");
    let (account, at, snapshot) = db
        .snapshots()?
        .into_iter()
        .find(|(a, _, _)| a.id == id)
        .ok_or("missing snapshot")?;
    assert_eq!(account.label, "Personal");
    assert_eq!(at, Some(now + 10));
    assert_eq!(snapshot.ok_or("empty snapshot")?["plan"], "Google AI Ultra");
    let reconnect = begin(&service, first, Some(id), &cancel).await?;
    db.remove(id)?;
    assert!(!update(&service, first, None, now + 20)?);
    assert!(matches!(
        reconnect.await?,
        Err(LoginError::Store(ConnectionError::Changed))
    ));
    assert!(!update(&service, first, None, now + 21)?);
    assert_eq!(db.list()?.len(), 1);
    assert_eq!(db.list()?[0].id, other);
    Ok(())
}

#[tokio::test]
async fn untouched_antigravity_sessions_hide_their_future_reset_and_pace() -> Result<()> {
    use jackin_usage::dashboard::{Dashboard, DashboardMemory, DashboardSettings};
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("usage/accounts.sqlite3");
    let clock = Arc::new(FakeClock::new());
    let now = clock.now().0;
    let service = OwnedLogin::new(path.clone(), ProviderEndpoints::real(), clock);
    let email = "viewer@example.com";
    let pending = begin(&service, email, None, &CancellationToken::new()).await?;
    assert!(update(&service, email, None, now)?);
    let account = pending.await??;
    let mut registry = super::db(&path, &Arc::new(super::Vault::default()))?;
    let settings = DashboardSettings {
        pace_marks: true,
        ..Default::default()
    };
    registry.set_dashboard_settings(&settings)?;

    let other = registry.add(
        UsageProvider::Claude,
        "Other",
        &super::scoped("other"),
        &super::bytes("fixture"),
    )?;
    for (id, metric, period, fraction, offset, shows_reset, shows_pace) in [
        (account, "gemini-5h", 18_000, 1.0, 3600, false, false),
        (account, "3p-5h", 18_000, 1.0, 3600, false, false),
        (account, "gemini-5h", 18_000, 0.99965, 3600, true, true),
        (account, "gemini-weekly", 604_800, 1.0, 3600, true, true),
        (account, "3p-5h", 18_000, 1.0, -1, true, false),
        (other, "session", 18_000, 1.0, 3600, true, true),
    ] {
        let at = now + offset;
        let body = json!({"resources":{metric:{
            "kind":"quota","unit":"percent","remaining_fraction":fraction,
            "window_seconds":period,"resets_at":at
        }}});
        registry.put_snapshot(id, registry.version(id)?, now, &body)?;
        let board = Dashboard::load(&registry, &mut DashboardMemory::default(), now)?;
        let row = board
            .all
            .iter()
            .find(|r| r.account == Some(id))
            .ok_or("missing row")?;
        assert_eq!(
            row.reset.is_some(),
            shows_reset,
            "{metric} {fraction} {offset}"
        );
        assert_eq!(
            row.pace_mark.is_some(),
            shows_pace,
            "{metric} {fraction} {offset}"
        );
        let export = board.redacted_export();
        assert!(
            export["rows"]
                .as_array()
                .ok_or("missing rows")?
                .iter()
                .any(|r| r["provider"] == row.provider.to_string() && r["reset"] == at)
        );
        let snapshot = registry
            .snapshots()?
            .into_iter()
            .find(|r| r.0.id == id)
            .ok_or("missing snapshot")?;
        let details = jackin_usage::export::details_account(
            snapshot,
            0,
            &settings,
            None,
            jackin_core::Timestamp(now),
        );
        assert_eq!(details["usage"]["resources"][metric]["resets_at"], at);
        let lines: Vec<String> = serde_json::from_value(details["details"].clone())?;
        assert_eq!(
            lines.iter().any(|line| line.starts_with("Reset · ")),
            shows_reset
        );
    }
    Ok(())
}

#[tokio::test]
async fn account_summary_surfaces_a_limiting_fourth_window_and_counts_visible_extras() -> Result<()>
{
    use jackin_usage::accounts::AccountPreferences;
    use jackin_usage::presentation;

    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("usage/accounts.sqlite3");
    let clock = Arc::new(FakeClock::new());
    let now = clock.now().0;
    let service = OwnedLogin::new(path.clone(), ProviderEndpoints::real(), clock);
    let email = "viewer@example.com";
    let pending = begin(&service, email, None, &CancellationToken::new()).await?;
    assert!(update(&service, email, None, now)?);
    let account = pending.await??;
    let registry = AccountRegistry::open(&path)?;
    registry.put_snapshot(
        account,
        registry.version(account)?,
        now,
        &json!({"resources": {
            "gemini-5h": {"kind":"quota", "remaining_percent":99},
            "gemini-weekly": {"kind":"quota", "remaining_percent":95},
            "gemini-argon-wtu-weekly": {"kind":"quota", "remaining_percent":68},
            "3p-5h": {"kind":"quota", "remaining_percent":97},
            "3p-weekly": {"kind":"quota", "remaining_percent":0.5},
        }}),
    )?;
    let mut prefs = AccountPreferences::default();
    prefs.hidden.insert("gemini-argon-wtu-weekly".into());
    registry.set_preferences(account, &prefs)?;
    let entry = presentation::entries(&service)?
        .pop()
        .ok_or("Antigravity account missing")?;
    assert_eq!(
        entry.summary,
        [
            "plan not reported · fetched <1m ago",
            "Claude weekly >99% used · reset not reported",
        ]
    );
    Ok(())
}

fn oauth(email: &str) -> jackin_usage::accounts::CandidateIdentity {
    jackin_usage::accounts::CandidateIdentity::OAuth(Some(email.to_owned()))
}

pub(super) async fn statusline_account(
    service: &OwnedLogin,
    email: &str,
    now: i64,
) -> Result<Uuid> {
    let cancel = CancellationToken::new();
    let pending = begin(service, email, None, &cancel).await?;
    assert!(update(service, email, Some("ultra"), now)?);
    Ok(pending.await??)
}

#[tokio::test]
async fn direct_sign_in_for_a_status_line_google_account_is_a_duplicate() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("usage/accounts.sqlite3");
    let clock = Arc::new(FakeClock::new());
    let now = clock.now().0;
    let service = OwnedLogin::new(path.clone(), ProviderEndpoints::real(), clock);
    statusline_account(&service, "me@example.com", now).await?;
    let vault = Arc::new(super::Vault::default());
    let mut registry = super::db(&path, &vault)?;
    let error = registry.add(
        UsageProvider::Antigravity,
        "Direct",
        &oauth("Me@Example.com"),
        &super::bytes("grant"),
    );
    assert!(
        matches!(error, Err(ConnectionError::DuplicateStatusline)),
        "{error:?}"
    );
    let text = ConnectionError::DuplicateStatusline.to_string();
    assert!(text.contains("status line"), "{text}");
    assert_eq!(registry.list()?.len(), 1);
    registry.add(
        UsageProvider::Antigravity,
        "Other",
        &oauth("other@example.com"),
        &super::bytes("grant"),
    )?;
    assert_eq!(registry.list()?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn direct_sign_in_can_replace_a_status_line_account_of_the_same_google_account() -> Result<()>
{
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("usage/accounts.sqlite3");
    let clock = Arc::new(FakeClock::new());
    let now = clock.now().0;
    let service = OwnedLogin::new(path.clone(), ProviderEndpoints::real(), clock);
    let id = statusline_account(&service, "me@example.com", now).await?;
    let vault = Arc::new(super::Vault::default());
    let mut registry = super::db(&path, &vault)?;
    let version = registry.version(id)?;
    registry.replace(
        id,
        version,
        &oauth("me@example.com"),
        &super::bytes("grant"),
    )?;
    assert_eq!(registry.list()?.len(), 1);
    assert_eq!(registry.credential(id)?.1.expose(), b"grant");
    let error = registry.add(
        UsageProvider::Antigravity,
        "Again",
        &oauth("me@example.com"),
        &super::bytes("grant"),
    );
    assert!(matches!(error, Err(ConnectionError::Duplicate)));
    assert!(!update(&service, "me@example.com", None, now + 1)?);
    Ok(())
}

#[tokio::test]
async fn status_line_enrollment_for_a_direct_google_account_is_a_duplicate() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("usage/accounts.sqlite3");
    let clock = Arc::new(FakeClock::new());
    let vault = Arc::new(super::Vault::default());
    let mut registry = super::db(&path, &vault)?;
    registry.add(
        UsageProvider::Antigravity,
        "Direct",
        &oauth("me@example.com"),
        &super::bytes("grant"),
    )?;
    let service = OwnedLogin::new(path.clone(), ProviderEndpoints::real(), clock);
    let (send, mut prompts) = tokio::sync::mpsc::channel(2);
    let cancel = CancellationToken::new();
    let task = {
        let service = service.clone();
        tokio::spawn(async move {
            service
                .connect_antigravity_statusline(None, send, &cancel)
                .await
        })
    };
    let Some(LoginPrompt::Field { reply, .. }) = prompts.recv().await else {
        return Err("missing email prompt".into());
    };
    reply
        .send(Secret::new("ME@example.com".into()))
        .map_err(|_| "closed prompt")?;
    assert!(matches!(
        task.await?,
        Err(LoginError::Store(ConnectionError::DuplicateDirect))
    ));
    assert_eq!(registry.list()?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn direct_sign_in_cannot_convert_a_status_line_row_to_another_google_account() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("usage/accounts.sqlite3");
    let clock = Arc::new(FakeClock::new());
    let now = clock.now().0;
    let service = OwnedLogin::new(path.clone(), ProviderEndpoints::real(), clock);
    let id = statusline_account(&service, "me@example.com", now).await?;
    let vault = Arc::new(super::Vault::default());
    let mut registry = super::db(&path, &vault)?;
    let version = registry.version(id)?;
    let error = registry.replace(
        id,
        version,
        &oauth("other@example.com"),
        &super::bytes("grant"),
    );
    assert!(
        matches!(error, Err(ConnectionError::DifferentIdentity)),
        "{error:?}"
    );
    assert!(registry.is_statusline(id)?);
    let kept = registry.list()?.remove(0);
    assert_eq!(kept.label, "me@example.com");
    let version = registry.version(id)?;
    registry.replace(
        id,
        version,
        &oauth("ME@example.com"),
        &super::bytes("grant"),
    )?;
    assert!(!registry.is_statusline(id)?);
    Ok(())
}
