// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::usage_connections::{
    BoardEdit, BoardMetric, BoardProvider, BoardSettings, MetricPreference, OrderingScope,
    UsageBoard, UsageEntry,
};
use jackin_core::plain;
use jackin_usage::accounts::{
    AccountRegistry, ConnectedAccount, ConnectionError, MetricEdit, MetricState,
};
use jackin_usage::dashboard::{Dashboard, DashboardMemory, DashboardRow, DashboardSettings};
use jackin_usage::onboarding::OwnedLogin;
use uuid::Uuid;

use jackin_usage::presentation as display;

fn selected(db: &AccountRegistry, id: &str) -> Result<ConnectedAccount, ConnectionError> {
    let id = Uuid::parse_str(id).map_err(|_| ConnectionError::Changed)?;
    db.list()?
        .into_iter()
        .find(|account| account.id == id)
        .ok_or(ConnectionError::Changed)
}

fn shift<T: PartialEq>(items: &mut [T], key: &T, up: bool) {
    let Some(at) = items.iter().position(|item| item == key) else {
        return;
    };
    let next = if up {
        at.saturating_sub(1)
    } else {
        (at + 1).min(items.len() - 1)
    };
    items.swap(at, next);
}

fn move_item(
    db: &mut AccountRegistry,
    scope: OrderingScope,
    id: &str,
    metric: String,
    up: bool,
) -> Result<(), ConnectionError> {
    let account = selected(db, id)?;
    match scope {
        OrderingScope::Account => {
            let ordered = db.list()?;
            let mut group: Vec<_> = ordered
                .iter()
                .filter(|a| a.provider == account.provider)
                .map(|a| a.id)
                .collect();
            shift(&mut group, &account.id, up);
            let mut group = group.into_iter();
            let ids: Vec<_> = ordered
                .into_iter()
                .map(|a| {
                    if a.provider == account.provider {
                        group.next().unwrap_or(a.id)
                    } else {
                        a.id
                    }
                })
                .collect();
            db.reorder(&ids)
        }
        OrderingScope::Provider => {
            let mut settings = db.dashboard_settings()?;
            let mut visible = Vec::new();
            for a in db.list()? {
                if !visible.contains(&a.provider) {
                    visible.push(a.provider);
                }
            }
            visible.sort_by_key(|p| {
                settings
                    .provider_order
                    .iter()
                    .position(|kind| kind == p)
                    .unwrap_or(usize::MAX)
            });
            shift(&mut visible, &account.provider, up);
            settings.provider_order.retain(|p| !visible.contains(p));
            visible.append(&mut settings.provider_order);
            settings.provider_order = visible;
            db.set_dashboard_settings(&settings)
        }
        OrderingScope::Metric => {
            let mut prefs = account.preferences;
            let (_, _, payload) = db
                .snapshots()?
                .into_iter()
                .find(|(a, _, _)| a.id == account.id)
                .ok_or(ConnectionError::Changed)?;
            if let Some(body) = payload {
                let mut visible = jackin_usage::dashboard::metric_order(&body, &prefs.metric_order);
                visible.retain(|key| !prefs.hidden.contains(key));
                shift(&mut visible, &metric, up);
                prefs.metric_order.retain(|key| !visible.contains(key));
                visible.append(&mut prefs.metric_order);
                prefs.metric_order = visible;
            }
            db.set_preferences(account.id, &prefs)
        }
    }
}

fn apply(db: &mut AccountRegistry, edit: BoardEdit) -> Result<(), ConnectionError> {
    match edit {
        BoardEdit::DefaultTab | BoardEdit::Read | BoardEdit::Refresh(_) | BoardEdit::Proxy(_) => {
            Ok(())
        }
        BoardEdit::Pin { account, metric } => {
            let mut a = selected(db, &account)?;
            if !a.preferences.pinned.remove(&metric) {
                a.preferences.pinned.insert(metric);
            }
            db.set_preferences(a.id, &a.preferences)
        }
        BoardEdit::Hide {
            account,
            metric,
            hidden,
            restore_pin,
        } => {
            let mut a = selected(db, &account)?;
            if hidden {
                a.preferences.hidden.insert(metric);
            } else {
                a.preferences.hidden.remove(&metric);
                if restore_pin {
                    a.preferences.pinned.insert(metric);
                }
            }
            db.set_preferences(a.id, &a.preferences)
        }
        BoardEdit::Unified {
            metric,
            before,
            after,
        } => {
            if before.len() != after.len() {
                return Err(ConnectionError::Changed);
            }
            let changes = before
                .iter()
                .zip(&after)
                .map(|(from, to)| {
                    if from.account != to.account {
                        return Err(ConnectionError::Changed);
                    }
                    let state = |p: &MetricPreference| match (p.hidden, p.pinned) {
                        (true, false) => Ok(MetricState::Hidden),
                        (false, true) => Ok(MetricState::Pinned),
                        (false, false) => Ok(MetricState::Visible),
                        (true, true) => Err(ConnectionError::Invalid),
                    };
                    Ok(MetricEdit {
                        account: Uuid::parse_str(&from.account)
                            .map_err(|_| ConnectionError::Changed)?,
                        before: state(from)?,
                        after: state(to)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            db.edit_metric(&metric, &changes)
        }
        BoardEdit::Move {
            scope,
            account,
            metric,
            up,
        } => move_item(db, scope, &account, metric, up),
        BoardEdit::Settings(input) => {
            let mut s = db.dashboard_settings()?;
            for (name, _, value) in input.flags {
                display::set_flag(&mut s, name, value);
            }
            s.set_warning(input.warning);
            s.set_critical(input.critical);
            db.set_dashboard_settings(&s)
        }
    }
}

fn metric(
    group: String,
    row: DashboardRow,
    accounts: &[UsageEntry],
    registry: &[ConnectedAccount],
    settings: &DashboardSettings,
) -> BoardMetric {
    let remaining = settings.remaining;
    let percent = |fraction: f64| {
        jackin_usage::Percent::new(fraction * 100.0)
            .ok()
            .map(jackin_usage::Percent::rounded)
    };
    let bar = row
        .display_fraction(remaining)
        .and_then(percent)
        .map(|amount| (amount, row.pace_mark.and_then(percent)));
    use jackin_usage::dashboard::Severity;
    use termrock::theme::Tone;
    let severity = match row.severity {
        Severity::Normal => Tone::Normal,
        Severity::Warning => Tone::Warning,
        Severity::Critical => Tone::Danger,
    };
    let reset = row
        .reset
        .as_deref()
        .map(|text| display::reset_caption(text, settings.exact_resets));
    let value = display::metric_value(&row, remaining);
    let details = display::metric_details(&row);
    BoardMetric {
        member_preferences: if row.account.is_none() {
            row.members
                .iter()
                .filter_map(|id| registry.iter().find(|a| a.id == *id))
                .map(|a| MetricPreference {
                    account: a.id.to_string(),
                    pinned: a.preferences.pinned.contains(&row.metric),
                    hidden: a.preferences.hidden.contains(&row.metric),
                })
                .collect()
        } else {
            Vec::new()
        },
        account: row.account.map(|id| id.to_string()),
        members: if row.account.is_none() {
            row.members
                .iter()
                .filter_map(|id| {
                    accounts
                        .iter()
                        .find(|entry| entry.id == id.to_string())
                        .cloned()
                })
                .collect()
        } else {
            Vec::new()
        },
        provider: row.provider.to_string(),
        title: display::caption(row.label, row.metric_label),
        value,
        metric: row.metric,
        group,
        details,
        pinned: row.pinned,
        bar,
        severity,
        reset,
    }
}

pub(super) fn load(
    service: &OwnedLogin,
    priority: bool,
    edit: BoardEdit,
    memory: &mut DashboardMemory,
    now: i64,
) -> Result<UsageBoard, ConnectionError> {
    let use_default = matches!(edit, BoardEdit::DefaultTab);
    let db = if matches!(
        edit,
        BoardEdit::DefaultTab | BoardEdit::Read | BoardEdit::Refresh(_) | BoardEdit::Proxy(_)
    ) {
        match AccountRegistry::open_read_only(service.database())? {
            Some(db) => db,
            None => AccountRegistry::open(service.database())?,
        }
    } else {
        let mut db = AccountRegistry::open(service.database())?;
        apply(&mut db, edit)?;
        db
    };
    let data = Dashboard::load(&db, memory, now)?;
    let priority = if use_default {
        !data.settings.default_all
    } else {
        priority
    };
    let share = data.redacted_export().to_string();
    let accounts = super::entries(service)?;
    let registry = db.list()?;
    let rows: Vec<BoardMetric> = if priority {
        data.priority
            .into_iter()
            .map(|(section, row)| {
                metric(
                    section.to_string(),
                    row,
                    &accounts,
                    &registry,
                    &data.settings,
                )
            })
            .collect()
    } else {
        data.all
            .into_iter()
            .map(|row| {
                metric(
                    row.provider.metric_group().into(),
                    row,
                    &accounts,
                    &registry,
                    &data.settings,
                )
            })
            .collect()
    };
    let alerts: Vec<_> = data.alerts.into_iter().map(plain).collect();
    let mut notices = Vec::new();
    if alerts.len() > 1 {
        notices.extend(alerts.iter().cloned());
    }
    if !data.notices.is_empty() {
        notices.push(display::NEEDS_ATTENTION.into());
        notices.extend(
            data.notices
                .iter()
                .map(|n| display::caption(&n.label, &n.message)),
        );
    }
    for id in &data.waiting {
        if let Some(entry) = accounts.iter().find(|a| a.id == id.to_string()) {
            notices.push(display::waiting(&entry.label));
        }
    }
    let attention = data
        .notices
        .iter()
        .filter_map(|n| {
            accounts
                .iter()
                .find(|a| a.id == n.account.to_string())
                .cloned()
        })
        .collect();
    if priority
        && data.shows_clear_row
        && data.summary != jackin_usage::dashboard::Summary::AllClear
    {
        notices.push(display::ALL_CLEAR.into());
    }
    let settings = BoardSettings {
        hidden: registry
            .into_iter()
            .flat_map(|a| {
                let label = accounts
                    .iter()
                    .find(|entry| entry.id == a.id.to_string())
                    .map(|entry| entry.label.clone())
                    .unwrap_or_default();
                a.preferences
                    .hidden
                    .into_iter()
                    .map(move |key| (a.id.to_string(), key.clone(), display::caption(&label, key)))
            })
            .collect(),
        proxy: db.proxy_configured()?,
        flags: display::flags(&data.settings),
        warning: data.settings.warning,
        critical: data.settings.critical,
    };
    let providers = jackin_usage::catalog::providers()
        .map(|product| BoardProvider {
            id: product.id.into(),
            name: product.name.into(),
            accounts: accounts
                .iter()
                .filter(|entry| entry.provider == product.id)
                .cloned()
                .collect(),
            unavailable_reason: product.unavailable_reason.map(str::to_owned),
        })
        .collect();
    Ok(UsageBoard {
        providers,
        priority,
        summary: if priority || rows.is_empty() {
            data.summary.to_string()
        } else {
            display::ALL_USAGE_TITLE.into()
        },
        notices,
        attention,
        empty: accounts.is_empty(),
        alerts,
        rows,
        settings,
        share,
    })
}
