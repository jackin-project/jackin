// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_console::screens::Go;
use jackin_core::{Clock, Secret, SecretBytes};
use jackin_testkit::TestResult;
use jackin_usage::accounts::{
    self, AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault, UsageProvider,
};
use serde_json::Value;
use termrock::core::id::WidgetId;
use uuid::Uuid;

use super::support::Lab;

#[derive(Debug)]
pub(super) struct FixtureVault;

impl CredentialVault for FixtureVault {
    fn store(&self, _id: Uuid, _secret: &SecretBytes) -> Result<(), ConnectionError> {
        Ok(())
    }
    fn delete(&self, _id: Uuid) -> Result<(), ConnectionError> {
        Ok(())
    }
    fn load(&self, _id: Uuid) -> Result<SecretBytes, ConnectionError> {
        Err(ConnectionError::MissingCredential)
    }
}

pub(super) struct Expected<'a> {
    pub(super) account: &'a [&'a str],
    pub(super) board: &'a [&'a str],
    pub(super) absent: &'a [&'a str],
}

pub(super) async fn focus(lab: &mut Lab, dialog: &str, action: &str) -> TestResult {
    let button = WidgetId::of(dialog).sub(action);
    for _ in 0..24 {
        if lab.console.console().app().shell.focus.current() == Some(button) {
            return Ok(());
        }
        lab.step("tab").await?;
    }
    Err(format!("cannot focus {action}: {}", lab.console.screen()).into())
}

pub(super) async fn activate(lab: &mut Lab, dialog: &str, action: &str) -> TestResult {
    focus(lab, dialog, action).await?;
    lab.step("enter").await
}

pub(super) const BOARD: &str = "usage.connected_board";
const ACCOUNT: &str = "accounts.usage_connection";

pub(super) async fn inspect(
    provider: UsageProvider,
    snapshot: Value,
    expected: Expected<'_>,
) -> TestResult {
    journey(provider, snapshot, expected, None).await
}

pub(super) async fn journey(
    provider: UsageProvider,
    snapshot: Value,
    expected: Expected<'_>,
    metric: Option<(&str, &[&str])>,
) -> TestResult {
    let mut lab = Lab::open().await?;
    let path = lab
        .console
        .config()
        .path()
        .as_path()
        .parent()
        .ok_or("config directory")?
        .join(accounts::DATABASE_PATH);
    let mut db = AccountRegistry::with_vault(&path, Arc::new(FixtureVault))?;
    let label = format!("Fixture {provider}");
    let identity = if matches!(
        provider,
        UsageProvider::Zai
            | UsageProvider::OpenRouter
            | UsageProvider::Mimo
            | UsageProvider::Kimi
            | UsageProvider::MiniMax
    ) {
        CandidateIdentity::api_key(&Secret::new("synthetic".into()))
    } else {
        CandidateIdentity::OAuth(Some("synthetic".into()))
    };
    db.add_with_snapshot(
        provider,
        &label,
        &identity,
        &SecretBytes::new(vec![1]),
        lab.rig.world.clock.now().0,
        &snapshot,
    )?;
    lab.console
        .console()
        .app()
        .go(Go::Accounts { select: None });
    lab.step("N").await?;
    let list = lab.console.screen_at(180, 80);
    let product = jackin_console::usage_connections::PRODUCTS
        .iter()
        .find(|(key, _)| *key == provider.as_str())
        .ok_or("missing product label")?
        .1;
    assert!(list.contains(&format!("{label} · {product}")), "{list}");
    activate(&mut lab, ACCOUNT, "save").await?;
    activate(&mut lab, ACCOUNT, "save").await?;
    let account = lab.console.screen_at(180, 120);
    for text in expected.account {
        assert!(
            account.contains(text),
            "missing account detail {text:?}\n{account}"
        );
    }
    activate(&mut lab, ACCOUNT, "save").await?;
    lab.step("esc").await?;
    lab.console.console().app().go(Go::AllUsage);
    lab.console.settle().await?;
    let overview = lab.console.screen_at(180, 120);
    assert!(overview.contains("All Usage"), "{overview}");
    let total = jackin_usage::dashboard::metric_order(&snapshot, &[]).len();
    if total > 1 {
        activate(&mut lab, BOARD, "save").await?;
    }
    let board = format!("{overview}\n{}", lab.console.screen_at(180, 120));
    for text in expected.board {
        assert!(board.contains(text), "missing board row {text:?}\n{board}");
    }
    for text in expected.absent {
        assert!(
            !account.contains(text) && !board.contains(text),
            "unexpected {text:?}\n{account}\n{board}"
        );
    }
    if let Some((name, details)) = metric {
        for index in 0..total {
            if total > 1 {
                focus(&mut lab, BOARD, "readings").await?;
                lab.step("home").await?;
                for _ in 0..index {
                    lab.console.keys("down")?;
                }
            }
            activate(&mut lab, BOARD, "save").await?;
            let view = lab.console.screen_at(180, 100);
            if view.contains(&format!("{label} · {name}")) {
                for value in details {
                    assert!(
                        view.contains(value),
                        "missing metric detail {value:?}\n{view}"
                    );
                }
                return Ok(());
            }
            activate(&mut lab, BOARD, "save").await?;
        }
        return Err(format!("metric {name} unavailable").into());
    }
    Ok(())
}
