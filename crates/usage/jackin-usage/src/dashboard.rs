// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

mod alerts;
mod reading;
mod unify;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use uuid::Uuid;

use crate::accounts::{AccountRegistry, ConnectedAccount, ConnectionError, UsageProvider};

use alerts::State;
use reading::{Measure, Metrics};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Each switch is an independent user preference persisted by name."
)]
pub struct DashboardSettings {
    pub remaining: bool,
    pub default_all: bool,
    pub exact_resets: bool,
    pub pace_marks: bool,
    pub warning: u8,
    pub critical: u8,
    pub burning_fast: bool,
    pub use_it_or_lose_it: bool,
    pub conceal_names: bool,
    pub unify_claude: bool,
    pub unify_chatgpt: bool,
    pub notify_warning: bool,
    pub notify_critical: bool,
    pub notify_burning: bool,
    pub notify_unused: bool,
    pub provider_order: Vec<UsageProvider>,
}

impl Default for DashboardSettings {
    fn default() -> Self {
        Self {
            remaining: true,
            default_all: false,
            exact_resets: false,
            pace_marks: false,
            warning: 30,
            critical: 10,
            burning_fast: true,
            use_it_or_lose_it: true,
            conceal_names: false,
            unify_claude: false,
            unify_chatgpt: false,
            notify_warning: false,
            notify_critical: false,
            notify_burning: false,
            notify_unused: false,
            provider_order: Vec::new(),
        }
    }
}

impl DashboardSettings {
    pub fn set_warning(&mut self, n: u8) {
        self.warning = snap(n, 20, 70);
        self.critical = self.critical.min(self.warning - 5);
    }
    pub fn set_critical(&mut self, n: u8) {
        self.critical = snap(n, 5, 25);
        self.warning = self.warning.max(self.critical + 5);
    }
    pub(crate) fn normalize(&mut self) {
        self.critical = snap(self.critical, 5, 25);
        self.set_warning(self.warning);
        let mut seen = BTreeSet::new();
        self.provider_order.retain(|p| seen.insert(p.as_str()));
    }
}

fn snap(n: u8, lo: u8, hi: u8) -> u8 {
    ((n.clamp(lo, hi) + 2) / 5) * 5
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Serialize)]
pub enum Section {
    Pinned,
    RunningOut,
    UseItOrLoseIt,
    NeedsAttention,
    UsedUp,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pinned => "Pinned",
            Self::RunningOut => "Running Out",
            Self::UseItOrLoseIt => "Use It or Lose It",
            Self::NeedsAttention => "Needs Attention",
            Self::UsedUp => "Used Up",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Summary {
    WaitingForUsage,
    NoReportedLimits,
    Unavailable,
    AllClear,
    Listed,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WaitingForUsage => "Waiting for usage",
            Self::NoReportedLimits => "No reported limits",
            Self::Unavailable => "Usage unavailable",
            Self::AllClear => "All clear",
            Self::Listed => "Priority",
        })
    }
}

#[derive(Clone, Debug)]
pub struct DashboardRow {
    pub account: Option<Uuid>,
    pub members: Vec<Uuid>,
    pub provider: UsageProvider,
    pub label: String,
    pub metric: String,
    pub metric_label: String,
    pub value: String,
    pub reset: Option<String>,
    pub details: Vec<String>,
    pub plan: Option<String>,
    pub pinned: bool,
    pub hidden: bool,
    pub pace_mark: Option<f64>,
    pub stale: bool,
    pub severity: Severity,
    reading: Measure,
    section: Option<Section>,
    rank: (u8, f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Severity {
    #[default]
    Normal,
    Warning,
    Critical,
}

#[derive(Clone, Debug)]
pub struct Notice {
    pub account: Uuid,
    pub provider: UsageProvider,
    pub label: String,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct DashboardMemory {
    states: BTreeMap<(Uuid, String), State>,
    burning: BTreeMap<(Uuid, String), i64>,
}

#[derive(Clone, Debug)]
pub struct Dashboard {
    pub all: Vec<DashboardRow>,
    pub priority: Vec<(Section, DashboardRow)>,
    pub summary: Summary,
    pub notices: Vec<Notice>,
    pub waiting: Vec<Uuid>,
    pub alerts: Vec<String>,
    pub settings: DashboardSettings,
    pub shows_clear_row: bool,
    pub has_snapshot: bool,
}

struct Projection<'a> {
    rows: Vec<DashboardRow>,
    alerts: Vec<String>,
    seen: BTreeSet<(Uuid, String)>,
    settings: &'a DashboardSettings,
    memory: &'a mut DashboardMemory,
    now: i64,
}

pub fn concealed_label(index: usize) -> String {
    format!("Account {}", index + 1)
}

fn ordered_measures(body: &Value, order: &[String]) -> Vec<(String, Measure)> {
    let mut metrics = reading::read(body);
    metrics.sort_by_key(|(id, _)| {
        let saved = order.iter().position(|v| v == id);
        (
            saved.is_none(),
            saved.unwrap_or_else(|| reading::default_order(id)),
        )
    });
    metrics
}

pub fn metric_order(body: &Value, order: &[String]) -> Vec<String> {
    ordered_measures(body, order)
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

impl Projection<'_> {
    fn account(&mut self, account: &ConnectedAccount, body: &Value, label: &str, stale: bool) {
        let settings = self.settings;
        let now = self.now;
        let memory = &mut *self.memory;
        let metrics = ordered_measures(body, &account.preferences.metric_order);
        for (id, measure) in metrics {
            if account.preferences.hidden.contains(&id) {
                continue;
            }
            let key = (account.id, id.clone());
            self.seen.insert(key.clone());
            let mut row = DashboardRow {
                account: Some(account.id),
                members: vec![account.id],
                provider: account.provider,
                label: label.to_owned(),
                metric_label: measure.label.clone(),
                pinned: account.preferences.pinned.contains(&id),
                hidden: false,
                metric: id,
                value: String::new(),
                reset: None,
                details: Vec::new(),
                plan: body["plan"].as_str().map(str::to_owned),
                pace_mark: None,
                stale,
                severity: Severity::Normal,
                reading: measure,
                section: None,
                rank: (0, 0.0),
            };
            row.details.clone_from(&row.reading.detail);
            let calc = row.calculate(settings, now, memory.burning.get(&key).copied());
            if let Some(reset) = row.reading.reset.filter(|_| calc.burning) {
                memory.burning.insert(key.clone(), reset);
            } else {
                memory.burning.remove(&key);
            }
            if !row.stale && calc.bounded {
                self.alerts.extend(alerts::evaluate(
                    memory.states.entry(key).or_default(),
                    &row,
                    calc,
                    settings,
                    now,
                ));
            }
            self.rows.push(row);
        }
    }
}

impl Dashboard {
    pub fn load(
        db: &AccountRegistry,
        memory: &mut DashboardMemory,
        now: i64,
    ) -> Result<Self, ConnectionError> {
        let settings = db.dashboard_settings()?;
        let mut snapshots = db.snapshots()?.into_iter().enumerate().collect::<Vec<_>>();
        snapshots.sort_by_key(|(_, (a, _, _))| {
            settings
                .provider_order
                .iter()
                .position(|p| *p == a.provider)
                .unwrap_or(usize::MAX)
        });
        let empty = snapshots.is_empty();
        let has_snapshot = snapshots.iter().any(|(_, (_, _, body))| body.is_some());
        let mut notices = Vec::new();
        let mut waiting = Vec::new();
        let mut projected = Projection {
            rows: Vec::new(),
            alerts: Vec::new(),
            seen: BTreeSet::new(),
            settings: &settings,
            memory,
            now,
        };
        let mut families = Vec::<(UsageProvider, Vec<ConnectedAccount>)>::new();
        for (index, (account, stamp, payload)) in snapshots {
            let label = if settings.conceal_names {
                concealed_label(index)
            } else {
                account.label.clone()
            };
            let failed = db.failure(account.id)?;
            let notice = failed
                .as_ref()
                .map(|f| f.message.as_str())
                .or_else(|| payload.as_ref()?.get("notice")?.as_str());
            if let Some(message) = notice {
                notices.push(Notice {
                    account: account.id,
                    provider: account.provider,
                    label: label.clone(),
                    message: if settings.conceal_names {
                        "Current usage unavailable.".into()
                    } else {
                        message.to_owned()
                    },
                });
            } else if payload.is_none() {
                waiting.push(account.id);
            }
            if let Some(body) = payload {
                let stale = failed.is_some() || stamp.is_none_or(|s| now.saturating_sub(s) >= 600);
                projected.account(&account, &body, &label, stale);
            }
            if let Some((_, members)) = families.iter_mut().find(|(p, _)| *p == account.provider) {
                members.push(account);
            } else {
                families.push((account.provider, vec![account]));
            }
        }
        let Projection {
            rows,
            alerts,
            seen,
            memory,
            ..
        } = projected;
        memory.states.retain(|k, _| seen.contains(k));
        memory.burning.retain(|k, _| seen.contains(k));
        let mut all = Vec::new();
        for (provider, accounts) in families {
            let family = rows
                .iter()
                .filter(|row| row.provider == provider)
                .cloned()
                .collect::<Vec<_>>();
            let unified = match provider {
                UsageProvider::Claude => settings.unify_claude,
                UsageProvider::ChatGpt => settings.unify_chatgpt,
                _ => false,
            };
            if unified && accounts.len() > 1 {
                all.extend(unify::pool(&accounts, &family, &settings, now));
            } else {
                all.extend(family);
            }
        }
        let listed = all.iter().any(|r| r.section.is_some());
        let awaiting = empty || !waiting.is_empty();
        let summary = if listed {
            Summary::Listed
        } else if !notices.is_empty() {
            Summary::Unavailable
        } else if awaiting {
            Summary::WaitingForUsage
        } else if all.iter().any(|r| r.reading.bounded()) {
            Summary::AllClear
        } else {
            Summary::NoReportedLimits
        };
        let mut priority: Vec<_> = all
            .iter()
            .filter_map(|r| {
                let section = if r.pinned && r.section != Some(Section::UsedUp) {
                    Section::Pinned
                } else {
                    r.section?
                };
                Some((section, r.clone()))
            })
            .collect();
        priority.sort_by(|(a, left), (b, right)| {
            a.cmp(b).then_with(|| {
                if *a == Section::Pinned {
                    std::cmp::Ordering::Equal
                } else {
                    left.rank
                        .0
                        .cmp(&right.rank.0)
                        .then(left.rank.1.total_cmp(&right.rank.1))
                }
            })
        });
        let shows_clear_row = !awaiting
            && notices.is_empty()
            && !all.iter().any(|r| r.section == Some(Section::RunningOut));
        Ok(Self {
            all,
            priority,
            summary,
            notices,
            waiting,
            alerts,
            settings,
            shows_clear_row,
            has_snapshot,
        })
    }

    pub fn redacted_export(&self) -> Value {
        let rows = self
            .all
            .iter()
            .map(|r| {
                json!({
                    "provider": r.provider,
                    "metric": reading::safe_name(&r.metric),
                    "values": r.reading.redacted_values(),
                    "reset": r.reading.reset,
                    "stale": r.stale,
                })
            })
            .collect::<Vec<_>>();
        json!({"version":1,"summary":self.summary.to_string(),"rows":rows,"accounts_needing_attention":self.notices.len(),"waiting_accounts":self.waiting.len()})
    }
}

fn visible_reset(
    provider: UsageProvider,
    metric: &str,
    measure: &Measure,
    now: i64,
) -> Option<i64> {
    let at = measure.reset?;
    let idle = provider == UsageProvider::Antigravity
        && matches!(metric, "gemini-5h" | "3p-5h")
        && measure.bounded()
        && measure.used == Some(0.0)
        && at > now;
    (!idle).then_some(at)
}

pub(crate) fn account_detail_lines(
    provider: UsageProvider,
    body: &Value,
    settings: &DashboardSettings,
    now: i64,
) -> Vec<String> {
    let mut metrics = reading::read(body);
    metrics.sort_by_key(|(id, _)| reading::default_order(id));
    let mut lines = Vec::new();
    for (id, measure) in metrics {
        lines.push(format!(
            "{} · {}",
            measure.label,
            measure.display(settings.remaining)
        ));
        lines.extend(measure.quantity_details());
        if let Some(at) = visible_reset(provider, &id, &measure, now) {
            lines.push(format!(
                "Reset · {}",
                reading::reset_text(at, now, settings.exact_resets)
            ));
        }
        lines.extend(measure.detail);
    }
    lines
}

impl DashboardRow {
    pub(crate) fn quantity_details(&self) -> Vec<String> {
        self.reading.quantity_details()
    }

    pub fn display_fraction(&self, remaining: bool) -> Option<f64> {
        if !self.reading.bounded() {
            return None;
        }
        let used = (self.reading.used? / self.reading.limit?).clamp(0.0, 1.0);
        Some(if remaining { 1.0 - used } else { used })
    }

    fn calculate(
        &mut self,
        settings: &DashboardSettings,
        now: i64,
        prior_burn: Option<i64>,
    ) -> Metrics {
        let m = self.reading.metrics(settings, now, prior_burn);
        self.value = self.reading.display(settings.remaining);
        self.reset = visible_reset(self.provider, &self.metric, &self.reading, now)
            .map(|t| reading::reset_text(t, now, settings.exact_resets));
        self.severity = m.severity;
        self.pace_mark = m
            .share
            .filter(|_| settings.pace_marks && !m.spent && self.reset.is_some())
            .map(|s| if settings.remaining { 1.0 - s } else { s });
        let (section, rank) = if m.spent || self.reading.empty_balance {
            (
                Some(Section::UsedUp),
                (2, self.reading.reset.map_or(f64::INFINITY, |t| t as f64)),
            )
        } else if m.severity != Severity::Normal || settings.burning_fast && m.burning {
            (
                Some(Section::RunningOut),
                m.runway.map_or((1, m.remaining), |r| (0, r)),
            )
        } else if settings.use_it_or_lose_it && m.unused {
            (
                Some(Section::UseItOrLoseIt),
                (3, self.reading.reset.unwrap_or(i64::MAX) as f64),
            )
        } else {
            (None, (4, 0.0))
        };
        self.section = section;
        self.rank = rank;
        m
    }
}
