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
use jackin_usage::presentation::{Level, ReadingText};
use jackin_usage::view::{self, AccountQuota, ProviderGroup, UsageView, ViewFilter};
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

fn percent(fraction: f64) -> Option<u8> {
    jackin_usage::Percent::new(fraction * 100.0)
        .ok()
        .map(jackin_usage::Percent::rounded)
}

fn tone(level: Level) -> termrock::theme::Tone {
    use termrock::theme::Tone;
    match level {
        Level::Normal => Tone::Normal,
        Level::Warning => Tone::Warning,
        Level::Critical => Tone::Danger,
        Level::Muted => Tone::Muted,
    }
}

fn pace(row: Option<&DashboardRow>, settings: &DashboardSettings) -> Option<u8> {
    let mark = row?.pace_mark?;
    percent(if settings.remaining { 1.0 - mark } else { mark })
}

fn stored_reading(row: &DashboardRow, settings: &DashboardSettings) -> ReadingText {
    use jackin_usage::dashboard::Severity;
    ReadingText {
        value: display::metric_value(row, false),
        exact: display::exact_metric_value(row, false),
        reset: row
            .reset
            .as_deref()
            .map(|text| display::reset_caption(text, settings.exact_resets)),
        inspector: display::metric_inspector(row),
        details: display::metric_details(row, false),
        level: match row.severity {
            Severity::Normal => Level::Normal,
            Severity::Warning => Level::Warning,
            Severity::Critical => Level::Critical,
        },
        bar: row.display_fraction(false).and_then(percent),
    }
}

fn reading_of(view: &UsageView, row: &DashboardRow, settings: &DashboardSettings) -> ReadingText {
    row.account
        .and_then(|id| view.account(id))
        .and_then(|(_, account)| {
            account
                .meters()
                .find(|meter| meter.key == row.metric)
                .map(|meter| display::meter_reading(view, account, meter))
        })
        .unwrap_or_else(|| stored_reading(row, settings))
}

fn metric(
    group: String,
    row: DashboardRow,
    reading: ReadingText,
    accounts: &[UsageEntry],
    registry: &[ConnectedAccount],
    settings: &DashboardSettings,
) -> BoardMetric {
    let bar = reading
        .bar
        .map(|amount| (amount, pace(Some(&row), settings)));
    let title_label = row
        .account
        .and_then(|id| accounts.iter().find(|entry| entry.id == id.to_string()))
        .map_or_else(|| row.label.clone(), |entry| entry.label.clone());
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
        title: display::caption(title_label, &row.metric_label),
        label: plain(row.metric_label),
        value: reading.value,
        exact_value: reading.exact,
        metric: row.metric,
        group,
        inspector: reading.inspector,
        details: reading.details,
        pinned: row.pinned,
        bounded: bar.is_some(),
        bar,
        severity: tone(reading.level),
        reset: reading.reset,
    }
}

fn account_row(
    group: &ProviderGroup,
    account: &AccountQuota,
    key: &str,
    name: &str,
    reading: ReadingText,
    pinned: bool,
    pace: Option<u8>,
) -> BoardMetric {
    let bar = reading.bar.map(|amount| (amount, pace));
    BoardMetric {
        member_preferences: Vec::new(),
        account: Some(account.id.to_string()),
        members: Vec::new(),
        provider: group.provider.to_string(),
        metric: key.to_owned(),
        title: display::caption(display::entry_label(account), name),
        label: plain(name),
        bounded: bar.is_some(),
        value: reading.value,
        exact_value: reading.exact,
        group: group.name.to_owned(),
        bar,
        severity: tone(reading.level),
        reset: reading.reset,
        inspector: reading.inspector,
        details: reading.details,
        pinned,
    }
}

fn account_rows(
    view: &UsageView,
    group: &ProviderGroup,
    stored: &[DashboardRow],
    accounts: &[UsageEntry],
    registry: &[ConnectedAccount],
    settings: &DashboardSettings,
) -> Vec<BoardMetric> {
    let mut rows = Vec::new();
    for account in &group.accounts {
        let mut own = Vec::new();
        let pinned = |key: &str| {
            registry
                .iter()
                .find(|connected| connected.id == account.id)
                .is_some_and(|connected| connected.preferences.pinned.contains(key))
        };
        for meter in account.meters() {
            let marked = stored
                .iter()
                .find(|row| row.account == Some(account.id) && row.metric == meter.key);
            own.push(account_row(
                group,
                account,
                &meter.key,
                &meter.name,
                display::meter_reading(view, account, meter),
                pinned(&meter.key),
                pace(marked, settings).filter(|_| !meter.rolled_over),
            ));
        }
        for note in &account.notes {
            let (name, reading) = display::note_reading(view, account, note);
            let key = display::note_key(note);
            own.push(account_row(
                group,
                account,
                key,
                name,
                reading,
                pinned(key),
                None,
            ));
        }
        let covered = display::covered_keys(account);
        for row in stored.iter().filter(|row| {
            row.account == Some(account.id) && !covered.contains(&row.metric.as_str())
        }) {
            own.push(metric(
                group.name.to_owned(),
                row.clone(),
                stored_reading(row, settings),
                accounts,
                registry,
                settings,
            ));
        }
        let order = registry
            .iter()
            .find(|connected| connected.id == account.id)
            .map(|connected| connected.preferences.metric_order.as_slice())
            .unwrap_or_default();
        if !order.is_empty() {
            own.sort_by_key(|row| {
                order
                    .iter()
                    .position(|key| *key == row.metric)
                    .unwrap_or(usize::MAX)
            });
        }
        rows.extend(own);
    }
    rows
}

fn all_rows(
    view: &UsageView,
    data: &Dashboard,
    accounts: &[UsageEntry],
    registry: &[ConnectedAccount],
) -> Vec<BoardMetric> {
    use jackin_usage::accounts::UsageProvider;
    let mut rows = Vec::new();
    for group in &view.groups {
        let unified = match group.provider {
            UsageProvider::Claude => data.settings.unify_claude,
            UsageProvider::ChatGpt => data.settings.unify_chatgpt,
            _ => false,
        } && group.accounts.len() > 1;
        if unified {
            rows.extend(
                data.all
                    .iter()
                    .filter(|row| row.provider == group.provider)
                    .map(|row| {
                        let reading = stored_reading(row, &data.settings);
                        metric(
                            group.name.to_owned(),
                            row.clone(),
                            reading,
                            accounts,
                            registry,
                            &data.settings,
                        )
                    }),
            );
        } else {
            rows.extend(account_rows(
                view,
                group,
                &data.all,
                accounts,
                registry,
                &data.settings,
            ));
        }
    }
    rows
}

fn attention(view: &UsageView, accounts: &[UsageEntry]) -> Vec<UsageEntry> {
    use jackin_usage::view::Attention;
    let mut ids: Vec<Uuid> = Vec::new();
    for item in &view.attention {
        let listed: &[Uuid] = match item {
            Attention::Failing { account } => std::slice::from_ref(account),
            Attention::NearLimit { accounts, .. } | Attention::Duplicate { accounts, .. } => {
                accounts
            }
        };
        for id in listed {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
    }
    ids.iter()
        .filter_map(|id| {
            accounts
                .iter()
                .find(|entry| entry.id == id.to_string())
                .cloned()
        })
        .collect()
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
    let view = view::build(
        &db,
        &ViewFilter::default(),
        jackin_core::Timestamp(now),
        service.utc_offset_secs(),
    )?;
    let rows: Vec<BoardMetric> = if priority {
        data.priority
            .iter()
            .map(|(section, row)| {
                metric(
                    section.to_string(),
                    row.clone(),
                    reading_of(&view, row, &data.settings),
                    &accounts,
                    &registry,
                    &data.settings,
                )
            })
            .collect()
    } else {
        all_rows(&view, &data, &accounts, &registry)
    };
    let alerts: Vec<_> = data.alerts.iter().cloned().map(plain).collect();
    let (alerts, mut notices) = if alerts.len() > 1 {
        (vec![display::join_captions(&alerts)], alerts)
    } else {
        (alerts, Vec::new())
    };
    notices.extend(display::attention_notices(&view));
    for id in &data.waiting {
        if let Some(entry) = accounts.iter().find(|a| a.id == id.to_string()) {
            notices.push(display::waiting(&entry.label));
        }
    }
    let attention = attention(&view, &accounts);
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
            display::board_summary(&view)
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
