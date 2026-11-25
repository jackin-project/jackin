// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use std::collections::BTreeSet;

use super::{DashboardRow, DashboardSettings};
use crate::accounts::ConnectedAccount;

pub(super) fn pool(
    accounts: &[ConnectedAccount],
    rows: &[DashboardRow],
    settings: &DashboardSettings,
    now: i64,
) -> Vec<DashboardRow> {
    let mut order = Vec::<String>::new();
    for a in accounts {
        let keys = rows
            .iter()
            .filter(|r| r.account == Some(a.id))
            .map(|r| r.metric.clone())
            .collect::<Vec<_>>();
        for (index, key) in keys.iter().enumerate() {
            if !order.contains(key) {
                let pos = keys[index + 1..]
                    .iter()
                    .find_map(|k| order.iter().position(|known| known == k))
                    .unwrap_or(order.len());
                order.insert(pos, key.clone());
            }
        }
    }
    let plans: BTreeSet<_> = rows.iter().filter_map(|r| r.plan.as_deref()).collect();
    let plan = if plans.len() > 1 {
        Some("Multiple".into())
    } else {
        plans.first().map(|p| (*p).to_owned())
    };
    order
        .into_iter()
        .filter_map(|id| {
            let group = rows.iter().filter(|r| r.metric == id).collect::<Vec<_>>();
            let mut row = (**group.first()?).clone();
            row.account = None;
            row.members = accounts.iter().map(|a| a.id).collect();
            row.label = format!("{} accounts", accounts.len());
            row.plan.clone_from(&plan);
            row.pinned = accounts.iter().any(|a| a.preferences.pinned.contains(&id));
            row.stale = group.iter().any(|r| r.stale);
            row.details = group
                .iter()
                .map(|r| format!("{} · {}", r.label, r.value))
                .collect();
            let reading = &mut row.reading;
            reading.period = None;
            reading.reset = group.iter().filter_map(|r| r.reading.reset).min();
            reading.clear_exact();
            if group.iter().all(|r| {
                r.reading.bounded()
                    && r.reading.unit == reading.unit
                    && r.reading.kind == reading.kind
            }) {
                let limit = group.iter().filter_map(|r| r.reading.limit).sum::<f64>();
                let used = group
                    .iter()
                    .filter_map(|r| Some(r.reading.used?.min(r.reading.limit?)))
                    .sum::<f64>();
                let scale = if reading.unit == "percent" {
                    100.0 / limit
                } else {
                    1.0
                };
                reading.limit = Some(limit * scale);
                reading.used = Some(used * scale);
                reading.available = Some((limit - used) * scale);
                if reading.spent() && group.iter().any(|r| !r.reading.spent()) {
                    let least = if reading.unit == "percent" {
                        1.0
                    } else if reading.unit.len() == 3 {
                        0.01
                    } else {
                        0.1
                    };
                    reading.used = Some(limit * scale - least);
                    reading.available = Some(least);
                }
            } else {
                reading.limit = None;
                reading.used = None;
                reading.available = None;
                reading.parts.clear();
                sum_parts(&group, reading);
                reading.empty_balance = group.iter().all(|r| r.reading.empty_balance);
            }
            row.calculate(settings, now, None);
            Some(row)
        })
        .collect()
}

fn sum_parts(group: &[&DashboardRow], reading: &mut super::Measure) {
    for r in group {
        let m = &r.reading;
        if let Some(n) = if m.bounded() {
            m.used
        } else {
            m.available.or(m.used)
        } {
            if let Some((_, total)) = reading.parts.iter_mut().find(|(unit, _)| unit == &m.unit) {
                *total += n;
            } else {
                reading.parts.push((m.unit.clone(), n));
            }
        }
    }
}
