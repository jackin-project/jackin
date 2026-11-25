// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::owned_usage::{BOARD, FixtureVault, activate, focus};
use super::support::Lab;
use jackin_console::screens::Go;
use jackin_core::{Clock, Secret, SecretBytes};
use jackin_testkit::TestResult;
use jackin_usage::accounts::{self, AccountRegistry, CandidateIdentity, UsageProvider};
use serde_json::{Value, json};
use std::sync::Arc;

fn registry(lab: &Lab) -> Result<AccountRegistry, Box<dyn std::error::Error>> {
    let path = lab
        .console
        .config()
        .path()
        .as_path()
        .parent()
        .ok_or("config directory")?
        .join(accounts::DATABASE_PATH);
    Ok(AccountRegistry::with_vault(&path, Arc::new(FixtureVault))?)
}

fn seed(
    db: &mut AccountRegistry,
    lab: &Lab,
    provider: UsageProvider,
    label: &str,
    snapshot: &Value,
) -> TestResult {
    let identity = if provider == UsageProvider::OpenRouter {
        CandidateIdentity::api_key(&Secret::new(label.into()))
    } else {
        CandidateIdentity::OAuth(Some(label.into()))
    };
    db.add_with_snapshot(
        &jackin_core::CriticalWrite::begin(),
        provider,
        label,
        &identity,
        &SecretBytes::new(vec![1]),
        (lab.rig.world.clock.now().0, snapshot),
    )?;
    Ok(())
}

async fn open_all(lab: &mut Lab) -> TestResult {
    lab.console.console().app().go(Go::AllUsage);
    lab.console.settle().await
}

#[tokio::test]
async fn moved_window_row_stays_above_the_main_limit_after_reopening() -> TestResult {
    let mut lab = Lab::open().await?;
    let mut db = registry(&lab)?;
    seed(
        &mut db,
        &lab,
        UsageProvider::Claude,
        "Everyday",
        &json!({"resources":{
            "weekly":{"kind":"consumption","unit":"percent","used":50,"limit":100},
            "session":{"kind":"consumption","unit":"percent","used":10,"limit":100}
        }}),
    )?;
    open_all(&mut lab).await?;
    activate(&mut lab, BOARD, "save").await?;
    focus(&mut lab, BOARD, "readings").await?;
    lab.step("down").await?;
    activate(&mut lab, BOARD, "save").await?;
    let details = lab.console.screen_at(150, 65);
    assert!(details.contains("Everyday · Session"), "{details}");
    activate(&mut lab, BOARD, "order").await?;
    activate(&mut lab, "usage.connected_board", "up").await?;
    lab.step("esc").await?;
    open_all(&mut lab).await?;
    activate(&mut lab, BOARD, "save").await?;
    let screen = lab.console.screen_at(150, 65);
    let session = screen.find("Session").ok_or("missing session")?;
    let weekly = screen.find("Weekly").ok_or("missing weekly")?;
    assert!(session < weekly, "{screen}");
    Ok(())
}

#[tokio::test]
async fn concealed_account_keeps_its_alias_across_dashboard_and_management() -> TestResult {
    let mut lab = Lab::open().await?;
    let mut db = registry(&lab)?;
    let snapshot = json!({"resources":{"weekly":{"kind":"consumption","unit":"percent","used":50,"limit":100}}});
    seed(
        &mut db,
        &lab,
        UsageProvider::Claude,
        "Secret Alpha",
        &snapshot,
    )?;
    seed(
        &mut db,
        &lab,
        UsageProvider::ChatGpt,
        "Secret Beta",
        &snapshot,
    )?;
    let mut settings = db.dashboard_settings()?;
    settings.provider_order = vec![UsageProvider::ChatGpt, UsageProvider::Claude];
    settings.conceal_names = true;
    db.set_dashboard_settings(&settings)?;
    open_all(&mut lab).await?;
    let screen = lab.console.screen_at(150, 65);
    assert!(
        screen.contains("Account 2 · Weekly") && !screen.contains("Secret Beta"),
        "{screen}"
    );
    let codex_alias = screen.find("Account 2 · Weekly").ok_or("missing alias")?;
    let claude_alias = screen.find("Account 1 · Weekly").ok_or("missing alias")?;
    assert!(codex_alias < claude_alias, "{screen}");
    lab.step("esc").await?;
    lab.console
        .console()
        .app()
        .go(Go::Accounts { select: None });
    lab.step("N").await?;
    let screen = lab.console.screen_at(150, 65);
    assert!(
        screen.contains("Account 2 · ChatGPT / Codex") && !screen.contains("Secret Beta"),
        "{screen}"
    );
    Ok(())
}

#[tokio::test]
async fn unified_hide_and_undo_restore_each_members_original_setting() -> TestResult {
    let mut lab = Lab::open().await?;
    let mut db = registry(&lab)?;
    let snapshot = json!({"resources":{"weekly":{"kind":"consumption","unit":"percent","used":50,"limit":100}}});
    for label in ["Home", "Work", "Shared"] {
        seed(&mut db, &lab, UsageProvider::Claude, label, &snapshot)?;
    }
    let members = db.list()?;
    let mut hidden = members[0].preferences.clone();
    hidden.hidden.insert("weekly".into());
    db.set_preferences(members[0].id, &hidden)?;
    let mut pinned = members[1].preferences.clone();
    pinned.pinned.insert("weekly".into());
    db.set_preferences(members[1].id, &pinned)?;
    let mut settings = db.dashboard_settings()?;
    settings.unify_claude = true;
    db.set_dashboard_settings(&settings)?;
    let original = db
        .list()?
        .into_iter()
        .map(|a| a.preferences)
        .collect::<Vec<_>>();
    open_all(&mut lab).await?;
    activate(&mut lab, "usage.connected_board", "save").await?;
    let screen = lab.console.screen_at(150, 65);
    assert!(
        screen.contains("Unpin") && screen.contains("Hide"),
        "{screen}"
    );
    activate(&mut lab, "usage.connected_board", "save").await?;
    assert_eq!(
        db.list()?
            .into_iter()
            .map(|a| a.preferences)
            .collect::<Vec<_>>(),
        original
    );

    activate(&mut lab, "usage.connected_board", "save").await?;
    activate(&mut lab, "usage.connected_board", "hide").await?;
    let hidden = db.list()?;
    assert!(hidden.iter().all(
        |a| a.preferences.hidden.contains("weekly") && !a.preferences.pinned.contains("weekly")
    ));
    activate(&mut lab, BOARD, "more").await?;
    activate(&mut lab, BOARD, "undo").await?;
    assert_eq!(
        db.list()?
            .into_iter()
            .map(|a| a.preferences)
            .collect::<Vec<_>>(),
        original
    );
    focus(&mut lab, BOARD, "row").await?;
    lab.step("home").await?;
    activate(&mut lab, BOARD, "save").await?;
    activate(&mut lab, BOARD, "pin").await?;
    assert!(
        db.list()?
            .iter()
            .all(|a| !a.preferences.pinned.contains("weekly"))
    );
    activate(&mut lab, BOARD, "more").await?;
    activate(&mut lab, BOARD, "undo").await?;
    assert_eq!(
        db.list()?
            .into_iter()
            .map(|a| a.preferences)
            .collect::<Vec<_>>(),
        original
    );
    Ok(())
}

#[tokio::test]
async fn stale_group_change_leaves_unaffected_members_and_other_metrics_untouched() -> TestResult {
    use jackin_usage::accounts::{MetricEdit, MetricState};
    let lab = Lab::open().await?;
    let mut db = registry(&lab)?;
    for label in ["Home", "Work"] {
        seed(&mut db, &lab, UsageProvider::Claude, label, &json!({}))?;
    }
    let accounts = db.list()?;
    let mut changed = accounts[1].preferences.clone();
    changed.pinned.extend(["weekly".into(), "monthly".into()]);
    db.set_preferences(accounts[1].id, &changed)?;
    let edit = accounts
        .iter()
        .map(|a| MetricEdit {
            account: a.id,
            before: MetricState::Visible,
            after: MetricState::Hidden,
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        db.edit_metric("weekly", &edit),
        Err(accounts::ConnectionError::Changed)
    ));
    let after = db.list()?;
    assert_eq!(after[0].preferences, accounts[0].preferences);
    assert_eq!(after[1].preferences, changed);
    Ok(())
}

#[tokio::test]
async fn moving_provider_up_reorders_the_two_connected_products() -> TestResult {
    let mut lab = Lab::open().await?;
    let mut db = registry(&lab)?;
    let snapshot = json!({"resources":{"weekly":{"kind":"consumption","unit":"percent","used":50,"limit":100}}});
    seed(
        &mut db,
        &lab,
        UsageProvider::Claude,
        "First product",
        &snapshot,
    )?;
    seed(
        &mut db,
        &lab,
        UsageProvider::OpenRouter,
        "Second product",
        &snapshot,
    )?;
    open_all(&mut lab).await?;
    lab.step("down enter").await?;
    let shown = lab.console.screen_at(150, 65);
    assert!(shown.contains("Second product · Weekly"), "{shown}");
    activate(&mut lab, BOARD, "order").await?;
    focus(&mut lab, BOARD, "scope").await?;
    lab.step("enter down down enter").await?;
    activate(&mut lab, "usage.connected_board", "up").await?;
    let shown = lab.console.screen_at(150, 65);
    let first = shown
        .find("First product · Weekly")
        .ok_or("first product missing")?;
    let second = shown
        .find("Second product · Weekly")
        .ok_or("second product missing")?;
    assert!(second < first, "{shown}");
    Ok(())
}

#[tokio::test]
async fn simultaneous_priority_alerts_identify_each_account_using_its_concealed_alias() -> TestResult
{
    let mut lab = Lab::open().await?;
    let mut db = registry(&lab)?;
    let usage = |used| json!({"resources":{"weekly":{"kind":"consumption","unit":"percent","used":used,"limit":100}}});
    for label in ["Account Alpha", "Account Beta"] {
        seed(&mut db, &lab, UsageProvider::Claude, label, &usage(20))?;
    }
    let mut settings = db.dashboard_settings()?;
    settings.conceal_names = true;
    settings.notify_critical = true;
    db.set_dashboard_settings(&settings)?;
    open_all(&mut lab).await?;
    for a in db.list()? {
        db.put_snapshot(
            a.id,
            db.version(a.id)?,
            lab.rig.world.clock.now().0,
            &usage(95),
        )?;
    }
    for _ in 0..2 {
        activate(&mut lab, "usage.connected_board", "switch").await?;
        let shown = lab.console.screen_at(160, 65);
        for label in ["Account 1", "Account 2"] {
            assert!(
                shown.contains(&format!("Critical · {label} · Weekly")),
                "{shown}"
            );
        }
        assert!(
            !shown.contains("Account Alpha") && !shown.contains("Account Beta"),
            "{shown}"
        );
    }
    Ok(())
}
