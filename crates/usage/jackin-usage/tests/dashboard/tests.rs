// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{Vault, bytes, db, scoped};
use jackin_usage::accounts::{AccountPreferences, AccountRegistry, FailureKind, UsageProvider};
use jackin_usage::dashboard::{
    Dashboard, DashboardMemory, DashboardSettings, Section, Severity, Summary,
};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn limits(used: i64, end: i64) -> Value {
    json!({"resources":{"weekly":{"kind":"consumption","unit":"percent","used":used,"limit":100,"resetsAt":end,"windowSeconds":604800}}})
}
fn put(registry: &mut AccountRegistry, id: Uuid, t: i64, v: &Value) -> Result {
    registry.put_snapshot(id, registry.version(id)?, t, v)?;
    Ok(())
}

#[test]
fn priority_distinguishes_waiting_hidden_attention_empty_and_spent() -> Result {
    let temp = tempfile::tempdir()?;
    let mut registry = db(
        &temp.path().join("usage/accounts.db"),
        &Arc::new(Vault::default()),
    )?;
    let mut mem = DashboardMemory::default();
    let now = 1700000000;
    let id = registry.add(
        UsageProvider::Claude,
        "private@example.invalid",
        &scoped("a"),
        &bytes("sample"),
    )?;
    assert_eq!(
        Dashboard::load(&registry, &mut mem, now)?.summary,
        Summary::WaitingForUsage
    );
    put(&mut registry, id, now, &json!({"resources":{}}))?;
    assert_eq!(
        Dashboard::load(&registry, &mut mem, now)?.summary,
        Summary::NoReportedLimits
    );
    let mut body = limits(69, now + 260000);
    body["resources"]["credits"] = json!({"kind":"balance","unit":"credits","available":0});
    body["resources"]["rateLimitResets"] = json!({"kind":"balance","unit":"resets","available":0});
    let mut prefs = AccountPreferences {
        pinned: ["weekly".into()].into(),
        ..Default::default()
    };
    registry.set_preferences(id, &prefs)?;
    put(&mut registry, id, now, &body)?;
    let d = Dashboard::load(&registry, &mut mem, now)?;
    assert_eq!(
        d.priority
            .iter()
            .map(|(section, r)| (*section, r.metric.as_str()))
            .collect::<Vec<_>>(),
        [(Section::Pinned, "weekly"), (Section::UsedUp, "credits")]
    );
    assert!(
        !d.redacted_export()
            .to_string()
            .contains("private@example.invalid")
    );
    body["resources"]["weekly"]["used"] = json!(99.72);
    put(&mut registry, id, now, &body)?;
    let low = Dashboard::load(&registry, &mut mem, now)?;
    assert_eq!(
        low.all
            .iter()
            .find(|r| r.metric == "weekly")
            .map(|r| r.value.as_str()),
        Some(">99%")
    );
    assert!(
        !low.priority
            .iter()
            .any(|(s, r)| *s == Section::UsedUp && r.metric == "weekly")
    );
    body["resources"]["weekly"]["used"] = json!(100);
    put(&mut registry, id, now, &body)?;
    assert!(
        Dashboard::load(&registry, &mut mem, now)?
            .priority
            .iter()
            .any(|(s, r)| *s == Section::UsedUp && r.metric == "weekly")
    );
    prefs.hidden.insert("weekly".into());
    registry.set_preferences(id, &prefs)?;
    assert!(
        !Dashboard::load(&registry, &mut mem, now)?
            .all
            .iter()
            .any(|r| r.metric == "weekly")
    );
    registry.record_failure(
        id,
        registry.version(id)?,
        now,
        FailureKind::Authentication,
        None,
        "sign in again",
    )?;
    let d = Dashboard::load(&registry, &mut mem, now)?;
    assert_eq!(d.notices.len(), 1);
    assert!(d.all.iter().all(|r| r.stale));
    Ok(())
}

#[test]
fn warnings_rearm_after_recovery_and_fast_pace_latches_until_ninety_percent() -> Result {
    let temp = tempfile::tempdir()?;
    let mut registry = db(
        &temp.path().join("usage/accounts.db"),
        &Arc::new(Vault::default()),
    )?;
    let id = registry.add(
        UsageProvider::Claude,
        "Account",
        &scoped("a"),
        &bytes("sample"),
    )?;
    let mut mem = DashboardMemory::default();
    registry.set_dashboard_settings(&DashboardSettings {
        notify_warning: true,
        notify_critical: true,
        notify_burning: true,
        ..Default::default()
    })?;
    let end = 1701000000;
    let now = end - 302400;
    let reading = |registry: &mut AccountRegistry, mem: &mut DashboardMemory, value| {
        registry.put_snapshot(id, registry.version(id)?, now, &limits(value, end))?;
        Dashboard::load(registry, mem, now)
    };
    assert!(reading(&mut registry, &mut mem, 10)?.alerts.is_empty());
    let fast = reading(&mut registry, &mut mem, 51)?;
    assert!(fast.alerts.iter().any(|s| s.starts_with("Burning Fast")));
    assert_eq!(fast.priority[0].0, Section::RunningOut);
    assert_eq!(
        reading(&mut registry, &mut mem, 48)?.priority[0].0,
        Section::RunningOut
    );
    assert!(reading(&mut registry, &mut mem, 45)?.priority.is_empty());
    let critical = reading(&mut registry, &mut mem, 90)?;
    assert!(critical.alerts.iter().any(|s| s.starts_with("Critical")));
    assert!(!critical.alerts.iter().any(|s| s.starts_with("Warning")));
    assert!(reading(&mut registry, &mut mem, 90)?.alerts.is_empty());
    let restored = reading(&mut registry, &mut mem, 30)?;
    assert_eq!(restored.all[0].severity, Severity::Normal);
    assert!(
        reading(&mut registry, &mut mem, 70)?
            .alerts
            .iter()
            .any(|s| s.starts_with("Warning"))
    );
    Ok(())
}

#[test]
fn unified_limits_keep_members_units_and_the_earliest_reset() -> Result {
    let temp = tempfile::tempdir()?;
    let mut registry = db(
        &temp.path().join("usage/accounts.db"),
        &Arc::new(Vault::default()),
    )?;
    let now = 1700000000;
    for (label, used, unit, end) in [
        ("first", 120, "usd", now + 600),
        ("second", 49, "eur", now + 1200),
    ] {
        let mut body = limits(used, end);
        body["plan"] = json!(label);
        body["resources"]["extraUsage"] =
            json!({"kind":"consumption","unit":unit,"used":10,"limit":20});
        registry.add_with_snapshot(
            UsageProvider::Claude,
            label,
            &scoped(label),
            &bytes("sample"),
            now,
            &body,
        )?;
    }
    registry.set_dashboard_settings(&DashboardSettings {
        unify_claude: true,
        exact_resets: true,
        ..Default::default()
    })?;
    let d = Dashboard::load(&registry, &mut DashboardMemory::default(), now)?;
    let week = d
        .all
        .iter()
        .find(|r| r.metric == "weekly")
        .ok_or("missing weekly")?;
    assert_eq!(week.members.len(), 2);
    assert_eq!(week.plan.as_deref(), Some("Multiple"));
    assert_eq!(week.value, "75%");
    assert_eq!(week.reset.as_deref(), Some("2023-11-14 22:23 UTC"));
    assert_eq!(week.details.len(), 2);
    assert!(week.pace_mark.is_none());
    let spend = d
        .all
        .iter()
        .find(|r| r.metric == "extraUsage")
        .ok_or("missing spend")?;
    assert_eq!(spend.value, "10 usd · 10 eur");
    Ok(())
}

#[test]
fn a_pooled_row_of_exact_amounts_does_not_inherit_an_exhausted_member() -> Result {
    let temp = tempfile::tempdir()?;
    let mut registry = db(
        &temp.path().join("usage/accounts.db"),
        &Arc::new(Vault::default()),
    )?;
    let now = 1700000000;
    for (label, used) in [("spent", "10"), ("fresh", "1")] {
        registry.add_with_snapshot(
            UsageProvider::Claude,
            label,
            &scoped(label),
            &bytes("sample"),
            now,
            &json!({"resources":{"extraUsage":{"kind":"consumption","unit":"usd","used":used,"limit":"10"}}}),
        )?;
    }
    registry.set_dashboard_settings(&DashboardSettings {
        unify_claude: true,
        ..Default::default()
    })?;
    let d = Dashboard::load(&registry, &mut DashboardMemory::default(), now)?;
    let pooled = d
        .all
        .iter()
        .find(|r| r.metric == "extraUsage")
        .ok_or("missing pooled row")?;
    assert_eq!(pooled.members.len(), 2);
    assert_eq!(pooled.value, "9 usd");
    assert!(
        !d.priority
            .iter()
            .any(|(section, _)| *section == Section::UsedUp)
    );
    Ok(())
}

#[test]
fn unused_warning_needs_a_long_non_spend_window_and_an_observed_reset() -> Result {
    let temp = tempfile::tempdir()?;
    let mut registry = db(
        &temp.path().join("usage/accounts.db"),
        &Arc::new(Vault::default()),
    )?;
    let id = registry.add(
        UsageProvider::Claude,
        "Account",
        &scoped("a"),
        &bytes("sample"),
    )?;
    let mut memory = DashboardMemory::default();
    let now = 1700000000;
    registry.set_dashboard_settings(&DashboardSettings {
        notify_unused: true,
        ..Default::default()
    })?;
    let mut body = limits(20, now + 86401);
    put(&mut registry, id, now, &body)?;
    assert!(
        Dashboard::load(&registry, &mut memory, now)?
            .priority
            .is_empty()
    );
    body["resources"]["session"] = json!({"kind":"consumption","unit":"percent","limit":100,"used":10,"resetsAt":now+15,"windowSeconds":18000});
    body["resources"]["extraUsage"] = json!({"kind":"consumption","unit":"usd","limit":100,"used":10,"resetsAt":now+15,"windowSeconds":604800});
    body["resources"]["weekly"]["resetsAt"] = json!(now + 86400);
    put(&mut registry, id, now, &body)?;
    let d = Dashboard::load(&registry, &mut memory, now)?;
    assert_eq!(d.priority.len(), 1);
    assert_eq!(d.priority[0].0, Section::UseItOrLoseIt);
    assert_eq!(d.priority[0].1.metric, "weekly");
    assert_eq!(d.alerts.len(), 1);
    body["resources"]["weekly"]["resetsAt"] = json!(now + 86000);
    put(&mut registry, id, now, &body)?;
    assert!(
        Dashboard::load(&registry, &mut memory, now)?
            .alerts
            .is_empty()
    );
    body["resources"]["weekly"]["resetsAt"] = json!(now + 86900);
    put(&mut registry, id, now + 86001, &body)?;
    assert_eq!(
        Dashboard::load(&registry, &mut memory, now + 86001)?
            .alerts
            .len(),
        1
    );
    Ok(())
}

#[test]
fn a_stored_payload_with_the_retired_remaining_key_still_loads() -> Result {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("usage/accounts.db");
    let registry = db(&path, &Arc::new(Vault::default()))?;
    registry.set_dashboard_settings(&DashboardSettings::default())?;
    let stored = rusqlite::Connection::open(&path)?;
    stored.execute(
        "UPDATE dashboard_settings SET value=?1 WHERE id=1",
        [r#"{"remaining":false,"pace_marks":true,"warning":40,"critical":15}"#],
    )?;
    let loaded = registry.dashboard_settings()?;
    assert!(loaded.pace_marks);
    assert_eq!((loaded.warning, loaded.critical), (40, 15));
    Ok(())
}
