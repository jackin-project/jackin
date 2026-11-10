// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AccountName, LaunchId};
use serde_json::Value;

use crate::e2e::adapter::{UsageBehavior, UsageWindow};
use crate::e2e::run::{Fail, Res, Run, fail, labels_of, tail_of};
use crate::e2e::target::E2eTarget;
use crate::e2e::verdict::Verdict;

pub(crate) const USAGE_ATTEMPTS: u32 = 4;
const KINDS: [&str; 6] = [
    "no_credentials",
    "expired_login",
    "unsupported",
    "rate_limited",
    "network",
    "provider",
];

#[derive(Debug)]
pub(crate) struct UsageRow {
    pub account: String,
    pub windows: Option<Vec<Value>>,
    pub reason: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum A1Usage {
    Good,
    Transient(String),
    Broken(String),
}

pub(crate) fn parse_usage(bytes: &[u8]) -> Result<Vec<UsageRow>, String> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let accounts = value
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or("the usage output has no accounts list")?;
    accounts
        .iter()
        .map(|row| {
            let account = row
                .get("account")
                .and_then(Value::as_str)
                .ok_or("a usage record has no account")?;
            let outcome = row.get("outcome").ok_or("a usage record has no outcome")?;
            let windows = outcome.get("windows").and_then(Value::as_array).cloned();
            let reason = outcome
                .pointer("/reason/kind")
                .and_then(Value::as_str)
                .map(str::to_owned);
            Ok(UsageRow {
                account: account.to_owned(),
                windows,
                reason,
            })
        })
        .collect()
}

fn rate_problem(window: &Value, kind: &str) -> Option<String> {
    let used = window.get("used_percent").and_then(Value::as_f64);
    if !used.is_some_and(|percent| (0.0..=100.0).contains(&percent)) {
        return Some(format!(
            "A1 window {kind} has no used percentage between 0 and 100"
        ));
    }
    (!window.get("resets_at").is_some_and(Value::is_number))
        .then(|| format!("A1 window {kind} has no reset time"))
}

fn amount_problem(window: &Value, kind: &str) -> Option<String> {
    let remaining = window.get("remaining");
    let text = |key: &str| {
        remaining
            .and_then(|found| found.get(key))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    };
    let amount = remaining
        .and_then(|found| found.get("kind"))
        .and_then(Value::as_str)
        == Some("amount")
        && text("value").is_some_and(|value| value.parse::<f64>().is_ok())
        && text("unit").is_some();
    (!amount).then(|| format!("A1 window {kind} has no remaining amount"))
}

fn judge_window(window: &Value) -> Option<String> {
    let kind = window.get("kind").and_then(Value::as_str).unwrap_or("");
    match kind {
        "five_hour" | "weekly" | "monthly" => rate_problem(window, kind),
        "credits" | "spend" => amount_problem(window, kind),
        "other" => rate_problem(window, kind).and(amount_problem(window, kind)),
        _ => Some(format!("A1 window kind {kind:?} is unknown")),
    }
}

fn window_name(window: &UsageWindow) -> &str {
    match window {
        UsageWindow::FiveHour => "five_hour",
        UsageWindow::Weekly => "weekly",
        UsageWindow::Monthly => "monthly",
        UsageWindow::Credits => "credits",
        UsageWindow::Spend => "spend",
        UsageWindow::Other(_) => "other",
    }
}

pub(crate) fn judge_a1(row: Option<&UsageRow>, behavior: &UsageBehavior) -> A1Usage {
    let Some(row) = row else {
        return A1Usage::Broken("there is no usage record for the A1 account".to_owned());
    };
    match behavior {
        UsageBehavior::Unsupported { reason: expected } => {
            if row.windows.is_some() {
                return A1Usage::Broken(
                    "A1 returned windows with an unsupported usage contract".to_owned(),
                );
            }
            if row.reason.as_deref() == Some(expected.as_str()) {
                A1Usage::Good
            } else {
                A1Usage::Broken(format!(
                    "A1 usage reason {:?} does not match the declared unsupported reason {expected}",
                    row.reason
                ))
            }
        }
        UsageBehavior::Windows { required, optional } => {
            if let Some(windows) = &row.windows {
                if let Some(problem) = windows.iter().find_map(judge_window) {
                    return A1Usage::Broken(problem);
                }
                let kinds: Vec<&str> = windows
                    .iter()
                    .filter_map(|window| window.get("kind").and_then(Value::as_str))
                    .collect();
                for required in required {
                    let name = window_name(required);
                    if !kinds.contains(&name) {
                        return A1Usage::Broken(format!("A1 usage lacks required {name}"));
                    }
                }
                if let Some(unexpected) = kinds.iter().find(|kind| {
                    !required
                        .iter()
                        .chain(optional)
                        .any(|declared| window_name(declared) == **kind)
                }) {
                    return A1Usage::Broken(format!("A1 usage has undeclared window {unexpected}"));
                }
                return A1Usage::Good;
            }
            match row.reason.as_deref() {
                Some(reason @ ("network" | "rate_limited" | "provider")) => {
                    A1Usage::Transient(reason.to_owned())
                }
                Some(reason) if KINDS.contains(&reason) => A1Usage::Broken(format!(
                    "A1 usage is unavailable: {reason}; the login is broken"
                )),
                other => A1Usage::Broken(format!("A1 usage has an untyped reason {other:?}")),
            }
        }
    }
}

pub(crate) fn judge_fixture(account: &str, row: Option<&UsageRow>) -> Option<String> {
    let Some(row) = row else {
        return Some(format!("there is no usage record for {account}"));
    };
    if row.windows.is_some() {
        return Some(format!("the fixture account {account} returned windows"));
    }
    match row.reason.as_deref() {
        Some("expired_login") => None,
        Some(reason) if KINDS.contains(&reason) => Some(format!(
            "the fixture account {account} returned {reason}, expected expired_login"
        )),
        other => Some(format!(
            "the fixture account {account} has an untyped reason {other:?}"
        )),
    }
}

pub(crate) fn judge_selection(
    step: &str,
    label: Option<&str>,
    providers: &[String],
    target: &E2eTarget,
    expected: &str,
) -> Option<String> {
    let account = match AccountName::parse(expected) {
        Ok(account) => account,
        Err(error) => return Some(format!("{step}: invalid account {expected}: {error}")),
    };
    let provider = target.provider_name(&account).to_string();
    let agents = match crate::e2e::steps::j03::agents_label(target, &account) {
        Ok(agents) => agents,
        Err(error) => return Some(format!("{step}: cannot derive the agents label: {error}")),
    };
    if label != Some(agents.as_str()) {
        return Some(format!(
            "{step}: the agents label is {label:?}, expected {agents}"
        ));
    }
    (providers != [provider.clone()])
        .then(|| format!("{step}: the sandbox providers are {providers:?}, expected [{provider}]"))
}

impl Run<'_> {
    pub(crate) async fn j17(&mut self) -> Verdict {
        self.j17_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn load_for_account(&mut self, label: &str, expected: &str) -> Res<Option<String>> {
        let main = self.projects.main.clone();
        let launch = LaunchId::generate().map_err(|error| fail(error.to_string()))?;
        let project = main.display().to_string();
        let loaded = self
            .jackin_target(
                label,
                &[
                    "load",
                    "sentinel",
                    &project,
                    "--launch-id",
                    launch.as_str(),
                    "--provider",
                    "zai",
                    "--model",
                    "glm-5.3",
                ],
                &main,
            )
            .await?;
        if !loaded.ok() {
            return Ok(Some(format!(
                "{label}: the load exited {}: {}",
                loaded.exit.describe(),
                tail_of(&self.redactor, &loaded.stderr)
            )));
        }
        let found = self.find_launch(&launch).await?;
        let Some(view) = found.first() else {
            return Ok(Some(format!("{label}: no sandbox was created")));
        };
        let detail = self.b.gateway.sandbox_detail(&view.name).await?;
        let providers: Vec<String> = detail.providers.iter().map(ToString::to_string).collect();
        let labels = labels_of(&detail.view);
        Ok(judge_selection(
            label,
            labels.get("jackin.dev/agents").map(String::as_str),
            &providers,
            &self.credentials.target,
            expected,
        ))
    }

    async fn usage_once(&mut self) -> Res<Vec<UsageRow>> {
        let main = self.projects.main.clone();
        let output = self
            .jackin("J17 usage", &["usage", "--json"], &main)
            .await?;
        if !output.ok() {
            return Err(fail(format!(
                "jackin usage --json exited {}: {}",
                output.exit.describe(),
                tail_of(&self.redactor, &output.stderr)
            )));
        }
        parse_usage(&output.stdout).map_err(fail)
    }

    async fn usage_problems(&mut self) -> Res<Vec<String>> {
        let a1 = self.names.a1.clone();
        let fixtures = [self.names.fxr.clone(), self.names.fxw.clone()];
        let behavior = self.credentials.target.behavior().usage.clone();
        let mut problems = Vec::new();
        for attempt in 1..=USAGE_ATTEMPTS {
            let rows = self.usage_once().await?;
            let find = |name: &str| rows.iter().find(|row| row.account == name);
            if attempt == 1 {
                for fixture in &fixtures {
                    problems.extend(judge_fixture(fixture, find(fixture)));
                }
            }
            match judge_a1(find(&a1), &behavior) {
                A1Usage::Good => return Ok(problems),
                A1Usage::Broken(problem) => {
                    problems.push(problem);
                    return Ok(problems);
                }
                A1Usage::Transient(reason) if attempt == USAGE_ATTEMPTS => {
                    problems.push(format!(
                        "A1 usage stayed unavailable ({reason}) after {USAGE_ATTEMPTS} attempts"
                    ));
                }
                A1Usage::Transient(_) => tokio::time::sleep(self.b.budgets.usage_backoff).await,
            }
        }
        Ok(problems)
    }

    async fn j17_inner(&mut self) -> Res<Verdict> {
        let main = self.projects.main.clone();
        let workspace = self.names.main.clone();
        let mut problems: Vec<String> = Vec::new();
        problems.extend(self.load_for_account("J17 provider default", "zai").await?);
        let bound = self
            .jackin(
                "J17 bind role",
                &["account", "bind", "zai-role", "--role", "sentinel"],
                &main,
            )
            .await?;
        if !bound.ok() {
            return Err(fail("jackin account bind --role failed"));
        }
        problems.extend(
            self.load_for_account("J17 role binding", "zai-role")
                .await?,
        );
        let bound = self
            .jackin(
                "J17 bind workspace",
                &[
                    "account",
                    "bind",
                    "zai-workspace",
                    "--workspace",
                    &workspace,
                ],
                &main,
            )
            .await?;
        if !bound.ok() {
            return Err(fail("jackin account bind --workspace failed"));
        }
        problems.extend(
            self.load_for_account("J17 workspace binding", "zai-workspace")
                .await?,
        );
        problems.extend(self.usage_problems().await?);
        if problems.is_empty() {
            Ok(Verdict::pass(
                "workspace beats role beats default; usage has A1 windows and typed fixture reasons",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
