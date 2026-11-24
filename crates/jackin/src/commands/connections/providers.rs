// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt};

use jackin_usage::accounts::{AccountRegistry, ConnectedAccount, ConnectionError, UsageProvider};
use jackin_usage::catalog::{InfoProduct, UsageProduct, info_only, providers};
use jackin_usage::view::{Line, Span, Tone};
use serde::Serialize;
use uuid::Uuid;

use super::{database, failure};
use crate::{app::App, error::CommandError};

#[derive(Serialize)]
pub(super) struct ProviderRow {
    id: &'static str,
    name: &'static str,
    status: &'static str,
    pub(super) account_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    unavailable_reason: Option<&'static str>,
}

impl fmt::Display for ProviderRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} · {} · ", self.name, self.id)?;
        if let Some(reason) = self.unavailable_reason {
            write!(f, "{reason}")
        } else {
            match self.account_count {
                0 => f.write_str("Not connected"),
                1 => f.write_str("1 connected account"),
                n => write!(f, "{n} connected accounts"),
            }
        }
    }
}

fn product(id: &str) -> Result<UsageProduct, CommandError> {
    providers().find(|p| p.id == id).ok_or_else(|| {
        CommandError::Argument(
            "unknown usage provider; run jackin usage providers for available IDs".into(),
        )
    })
}

pub(super) fn connector(id: &str) -> Result<UsageProvider, CommandError> {
    let p = product(id)?;
    p.connector.ok_or_else(|| {
        CommandError::Argument(format!(
            "{}: {}",
            p.name,
            p.unavailable_reason.unwrap_or("Usage tracking unavailable")
        ))
    })
}

pub(super) fn catalog(accounts: impl Iterator<Item = UsageProvider> + Clone) -> Vec<ProviderRow> {
    providers()
        .map(|p| {
            let account_count = accounts
                .clone()
                .filter(|provider| Some(*provider) == p.connector)
                .count();
            ProviderRow {
                id: p.id,
                name: p.name,
                status: if p.connector.is_none() {
                    "unavailable"
                } else if account_count == 0 {
                    "not_connected"
                } else {
                    "connected"
                },
                account_count,
                unavailable_reason: p.unavailable_reason,
            }
        })
        .collect()
}

#[derive(Serialize)]
struct InfoRow {
    id: &'static str,
    name: &'static str,
    tracked: bool,
    reason: &'static str,
}

impl From<InfoProduct> for InfoRow {
    fn from(product: InfoProduct) -> Self {
        InfoRow {
            id: product.id,
            name: product.name,
            tracked: false,
            reason: product.reason,
        }
    }
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

fn duplicate_note(
    accounts: &[ConnectedAccount],
    identities: &BTreeMap<Uuid, String>,
    connector: Option<UsageProvider>,
) -> Option<String> {
    let mut shared: BTreeMap<&str, usize> = BTreeMap::new();
    for identity in accounts
        .iter()
        .filter(|a| Some(a.provider) == connector)
        .filter_map(|a| identities.get(&a.id))
    {
        *shared.entry(identity.as_str()).or_default() += 1;
    }
    match shared.into_values().max().unwrap_or(0) {
        0 | 1 => None,
        2 => Some("same account connected twice".to_owned()),
        n => Some(format!("same account connected {n} times")),
    }
}

struct Cells {
    name: &'static str,
    id: &'static str,
    count: String,
    connected: bool,
    note: Option<(String, Tone)>,
}

fn cells(
    row: &ProviderRow,
    accounts: &[ConnectedAccount],
    identities: &BTreeMap<Uuid, String>,
) -> Cells {
    let connector = providers()
        .find(|p| p.id == row.id)
        .and_then(|p| p.connector);
    let (count, note) = if let Some(reason) = row.unavailable_reason {
        ("-".to_owned(), Some((reason.to_owned(), Tone::Plain)))
    } else if row.account_count == 0 {
        ("not connected".to_owned(), None)
    } else {
        (
            row.account_count.to_string(),
            duplicate_note(accounts, identities, connector).map(|note| (note, Tone::Warn)),
        )
    };
    Cells {
        name: row.name,
        id: row.id,
        count,
        connected: row.account_count > 0,
        note,
    }
}

fn pad(text: &str, width: usize) -> String {
    format!("{text:<width$}")
}

fn widest(cells: &[Cells], column: fn(&Cells) -> &str, header: &str) -> usize {
    cells
        .iter()
        .map(|c| column(c).chars().count())
        .max()
        .unwrap_or(0)
        .max(header.len())
}

fn table(
    rows: &[&ProviderRow],
    totals: &[ProviderRow],
    accounts: &[ConnectedAccount],
    identities: &BTreeMap<Uuid, String>,
) -> Vec<Line> {
    let tracked = totals.iter().filter(|r| r.status != "unavailable").count();
    let connected = totals.iter().filter(|r| r.account_count > 0).count();
    let total: usize = totals.iter().map(|r| r.account_count).sum();
    let cells: Vec<Cells> = rows
        .iter()
        .map(|row| cells(row, accounts, identities))
        .collect();
    let name_width = widest(&cells, |c| c.name, "PROVIDER");
    let id_width = widest(&cells, |c| c.id, "ID");
    let count_width = widest(&cells, |c| &c.count, "ACCOUNTS");
    let mut lines = vec![
        Line::of(vec![Span::strong(format!(
            "Providers · {tracked} tracked · {connected} connected · {}",
            plural(total, "account")
        ))]),
        Line::blank(),
        Line::of(vec![Span::dim(format!(
            "{}  {}  {:>count_width$}  NOTE",
            pad("PROVIDER", name_width),
            pad("ID", id_width),
            "ACCOUNTS"
        ))]),
    ];
    for cell in cells {
        let figure = format!("{:>count_width$}", cell.count);
        let mut spans = vec![
            Span::strong(pad(cell.name, name_width)),
            Span::plain(format!("  {}  ", pad(cell.id, id_width))),
            if cell.connected {
                Span::plain(figure)
            } else {
                Span::dim(figure)
            },
        ];
        if let Some((text, tone)) = cell.note {
            spans.push(Span::plain("  "));
            spans.push(Span::new(text, tone));
        }
        lines.push(Line::of(spans));
    }
    lines
}

fn info_lines() -> Vec<Line> {
    let mut lines = Vec::new();
    for info in info_only() {
        lines.push(Line::blank());
        lines.push(Line::of(vec![
            Span::strong("Info"),
            Span::plain(format!("  {}: {}.", info.name, info.reason)),
        ]));
        lines.push(Line::of(vec![Span::plain(format!("      {}", info.hint))]));
    }
    lines
}

fn footer() -> Vec<Line> {
    vec![
        Line::blank(),
        Line::of(vec![Span::plain(
            "Connect: jackin account usage connect <id>",
        )]),
        Line::of(vec![Span::plain("Filter:  jackin usage --provider <id>")]),
    ]
}

pub(crate) fn show(app: &mut App, id: Option<&str>, json: bool) -> Result<(), CommandError> {
    let selected = id.map(product).transpose()?;
    let (accounts, identities) = AccountRegistry::open_read_only(&database(app)?)
        .map_err(failure)?
        .map(|db| Ok::<_, ConnectionError>((db.list()?, db.identities()?)))
        .transpose()
        .map_err(failure)?
        .unwrap_or_default();
    let totals = catalog(accounts.iter().map(|a| a.provider));
    let rows: Vec<&ProviderRow> = totals
        .iter()
        .filter(|r| selected.as_ref().is_none_or(|p| r.id == p.id))
        .collect();
    if json {
        let info: Vec<InfoRow> = if selected.is_some() {
            Vec::new()
        } else {
            info_only().map(InfoRow::from).collect()
        };
        app.io
            .json(&serde_json::json!({"schema":1,"providers":rows,"info":info}))
    } else {
        let mut lines = table(&rows, &totals, &accounts, &identities);
        if selected.is_none() {
            lines.extend(info_lines());
        }
        lines.extend(footer());
        app.io.lines(&lines)
    }
}
