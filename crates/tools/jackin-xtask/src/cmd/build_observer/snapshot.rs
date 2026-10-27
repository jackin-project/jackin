// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Instant,
};

#[derive(Clone, Debug, Default)]
pub(super) struct Stats {
    pub(super) count: u64,
    pub(super) alive_cpu_ms: [u64; 6],
    pub(super) rss_kib: [u64; 6],
    pub(super) max_rss_kib: u64,
}

impl Stats {
    pub(super) fn line(&self) -> String {
        format!(
            "count:{};alive_cpu_ms:{:?};rss_kib:{:?};max_rss_kib:{}",
            self.count, self.alive_cpu_ms, self.rss_kib, self.max_rss_kib
        )
    }
}

pub(super) fn read(root: u32, end: Instant) -> Option<Stats> {
    let first = relations(&super::capture::ps(
        &["-axo", "pid=,ppid="],
        128 * 1024,
        end,
    )?)?;
    let selected = descendants(root, &first, end)?;
    if selected.is_empty() || selected.len() > 128 {
        return None;
    }
    let pids = selected
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let raw = super::capture::ps(
        &[
            "-p",
            &pids,
            "-o",
            "pid=,ppid=,lstart=,time=,rss=,state=,comm=",
        ],
        16 * 1024,
        end,
    )?;
    let rows = details(&raw)?;
    let last = relations(&super::capture::ps(
        &["-axo", "pid=,ppid="],
        128 * 1024,
        end,
    )?)?;
    let rechecked = details(&super::capture::ps(
        &[
            "-p",
            &pids,
            "-o",
            "pid=,ppid=,lstart=,time=,rss=,state=,comm=",
        ],
        16 * 1024,
        end,
    )?)?;
    aggregate(root, &first, &last, rows, &rechecked, end)
}

pub(super) fn aggregate(
    root: u32,
    first: &BTreeMap<u32, u32>,
    last: &BTreeMap<u32, u32>,
    rows: Vec<Row>,
    rechecked: &[Row],
    end: Instant,
) -> Option<Stats> {
    let selected = descendants(root, first, end)?;
    let admitted = descendants(root, last, end)?;
    let mut stats = Stats::default();
    for row in rows {
        if Instant::now() >= end {
            return None;
        }
        if !rechecked
            .iter()
            .any(|last| last.pid == row.pid && last.ppid == row.ppid && last.started == row.started)
        {
            continue;
        }
        if !selected.contains(&row.pid)
            || !admitted.contains(&row.pid)
            || first.get(&row.pid) != Some(&row.ppid)
            || last.get(&row.pid) != Some(&row.ppid)
        {
            continue;
        }
        stats.count = stats.count.checked_add(1)?;
        let cpu = stats.alive_cpu_ms.get_mut(row.role)?;
        *cpu = cpu.checked_add(row.cpu_ms)?;
        let rss = stats.rss_kib.get_mut(row.role)?;
        *rss = rss.checked_add(row.rss_kib)?;
        stats.max_rss_kib = stats.max_rss_kib.max(row.rss_kib);
    }
    (stats.count > 0).then_some(stats)
}

pub(super) fn relations(raw: &[u8]) -> Option<BTreeMap<u32, u32>> {
    let mut result = BTreeMap::new();
    for line in std::str::from_utf8(raw).ok()?.lines() {
        if result.len() >= 8192 {
            return None;
        }
        let mut parts = line.split_ascii_whitespace();
        let pid = parts.next()?.parse().ok()?;
        let ppid = parts.next()?.parse().ok()?;
        if pid == 0 || parts.next().is_some() || result.insert(pid, ppid).is_some() {
            return None;
        }
    }
    Some(result)
}

pub(super) fn descendants(
    root: u32,
    rows: &BTreeMap<u32, u32>,
    end: Instant,
) -> Option<BTreeSet<u32>> {
    let mut result = BTreeSet::new();
    for &pid in rows.keys() {
        let mut current = pid;
        let mut seen = BTreeSet::new();
        for _ in 0..64 {
            if Instant::now() >= end {
                return None;
            }
            if current == root {
                result.insert(pid);
                break;
            }
            if !seen.insert(current) {
                break;
            }
            let Some(&parent) = rows.get(&current) else {
                break;
            };
            current = parent;
        }
        if result.len() > 128 {
            return None;
        }
    }
    Some(result)
}

#[derive(Clone)]
pub(super) struct Row {
    pub(super) pid: u32,
    pub(super) ppid: u32,
    pub(super) started: String,
    pub(super) cpu_ms: u64,
    pub(super) rss_kib: u64,
    pub(super) role: usize,
}

pub(super) fn details(raw: &[u8]) -> Option<Vec<Row>> {
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    for line in std::str::from_utf8(raw).ok()?.lines() {
        if rows.len() >= 128 {
            return None;
        }
        let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
        if fields.len() != 11 {
            return None;
        }
        let pid = fields.first()?.parse().ok()?;
        if pid == 0 || !seen.insert(pid) {
            return None;
        }
        rows.push(Row {
            pid,
            ppid: fields.get(1)?.parse().ok()?,
            started: fields.get(2..7)?.join(" "),
            cpu_ms: cpu_ms(fields.get(7)?)?,
            rss_kib: fields.get(8)?.parse().ok()?,
            role: role(fields.get(10)?),
        });
    }
    Some(rows)
}

pub(super) fn role(command: &str) -> usize {
    match Path::new(command)
        .file_name()
        .and_then(|value| value.to_str())
    {
        Some("cargo") => 0,
        Some("rustc") => 1,
        Some("rustdoc") => 2,
        Some("ld" | "lld" | "rust-lld" | "ld.lld") => 3,
        Some("clang" | "clang++" | "cc" | "gcc" | "zig") => 4,
        _ => 5,
    }
}

pub(super) fn cpu_ms(raw: &str) -> Option<u64> {
    let (days, clock) = raw.split_once('-').map_or(Some((0, raw)), |(day, rest)| {
        Some((day.parse::<u64>().ok()?, rest))
    })?;
    let fields = clock.split(':').collect::<Vec<_>>();
    if !(2..=3).contains(&fields.len()) {
        return None;
    }
    let mut seconds = 0_u64;
    for (index, value) in fields.iter().enumerate() {
        if index + 1 == fields.len() {
            let (whole, decimal) = value.split_once('.').unwrap_or((value, ""));
            if decimal.len() > 3 || !decimal.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            seconds = seconds
                .checked_mul(60)?
                .checked_add(whole.parse::<u64>().ok()?)?;
            let fraction = if decimal.is_empty() {
                0
            } else {
                decimal
                    .parse::<u64>()
                    .ok()?
                    .checked_mul(10_u64.pow(u32::try_from(3 - decimal.len()).ok()?))?
            };
            return days
                .checked_mul(86_400_000)?
                .checked_add(seconds.checked_mul(1000)?)?
                .checked_add(fraction);
        }
        seconds = seconds
            .checked_mul(60)?
            .checked_add(value.parse::<u64>().ok()?)?;
    }
    None
}
