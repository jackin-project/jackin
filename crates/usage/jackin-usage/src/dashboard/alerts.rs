// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use std::collections::BTreeSet;

use super::reading::{Metrics, advanced};
use super::{DashboardRow, DashboardSettings, Severity};

#[derive(Clone, Debug)]
pub(super) struct State {
    reset: Option<i64>,
    seen: bool,
    fired: BTreeSet<Milestone>,
    bucket: i8,
    severity: Severity,
}

impl Default for State {
    fn default() -> Self {
        Self {
            reset: None,
            seen: false,
            fired: BTreeSet::new(),
            bucket: -1,
            severity: Severity::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Milestone {
    Burning,
    Critical,
    Warning,
    Unused,
}

pub(super) fn evaluate(
    state: &mut State,
    row: &DashboardRow,
    m: Metrics,
    s: &DashboardSettings,
    now: i64,
) -> Vec<String> {
    if advanced(row.reading.reset, state.reset, now) {
        state.fired.clear();
        state.bucket = -1;
        state.severity = Severity::Normal;
    }
    state.reset = row.reading.reset.or(state.reset);
    if !state.seen {
        state.seen = true;
        state.severity = m.severity;
        state.bucket = m.bucket;
        if m.unused {
            state.fired.insert(Milestone::Unused);
        }
        return Vec::new();
    }
    let burning = m.bucket >= 2 && state.bucket < 2;
    let critical = m.severity == Severity::Critical && state.severity != Severity::Critical;
    let warning = m.severity != Severity::Normal && state.severity == Severity::Normal;
    let mut fired_pace = false;
    let mut fired_level = false;
    let mut out = Vec::new();
    for (kind, candidate, on, label) in [
        (
            Milestone::Burning,
            burning,
            s.notify_burning,
            "Burning Fast",
        ),
        (Milestone::Critical, critical, s.notify_critical, "Critical"),
        (Milestone::Warning, warning, s.notify_warning, "Warning"),
        (
            Milestone::Unused,
            m.unused,
            s.notify_unused,
            "Use It or Lose It",
        ),
    ] {
        if !candidate || state.fired.contains(&kind) {
            continue;
        }
        if kind == Milestone::Warning && fired_level {
            state.fired.insert(kind);
            continue;
        }
        if !on {
            continue;
        }
        out.push(format!("{label} · {} · {}", row.label, row.metric_label));
        state.fired.insert(kind);
        fired_pace |= kind == Milestone::Burning;
        fired_level |= matches!(kind, Milestone::Critical | Milestone::Warning);
    }
    if m.bucket == 0 {
        state.fired.remove(&Milestone::Burning);
    }
    if m.severity != Severity::Critical {
        state.fired.remove(&Milestone::Critical);
    }
    if m.severity == Severity::Normal {
        state.fired.remove(&Milestone::Warning);
    }
    if m.bucket >= 0 && (!burning || fired_pace || state.fired.contains(&Milestone::Burning)) {
        state.bucket = m.bucket;
    }
    if !(warning || critical) || fired_level || state.fired.contains(&Milestone::Critical) {
        state.severity = m.severity;
    }
    out
}
