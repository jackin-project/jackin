// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{ProviderName, SandboxName};
use serde_json::Value;

use super::host::HostCommand;
use super::run::labels_of;
use super::{Boundaries, E2eArgs};

#[derive(Debug, Default)]
pub(crate) struct Stale {
    pub sandboxes: Vec<SandboxName>,
    pub providers: Vec<String>,
}

impl Stale {
    pub(crate) fn is_empty(&self) -> bool {
        self.sandboxes.is_empty() && self.providers.is_empty()
    }

    pub(crate) fn describe(&self) -> String {
        format!(
            "{} sandboxes and {} providers from earlier e2e runs",
            self.sandboxes.len(),
            self.providers.len()
        )
    }
}

pub(crate) fn is_e2e_provider(name: &str) -> bool {
    name.starts_with("jackin-e2e-") && name.ends_with("-codex")
}

pub(crate) async fn list_providers(
    b: &Boundaries,
    env: &BTreeMap<String, String>,
    gateway: &str,
) -> Result<Vec<String>, String> {
    let command = HostCommand::new("openshell", b.budgets.exec)
        .args(["-g", gateway, "provider", "list", "-o", "json"])
        .env(env.clone());
    let output = b
        .host
        .run(&command, &b.cancel)
        .await
        .map_err(|error| error.to_string())?;
    if !output.ok() {
        return Err(format!(
            "openshell provider list exited {}",
            output.exit.describe()
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("unreadable provider list: {error}"))?;
    let items = value
        .get("providers")
        .and_then(Value::as_array)
        .ok_or_else(|| "the provider list has no providers array".to_owned())?;
    items
        .iter()
        .map(|item| {
            item.get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| "a provider entry has no name".to_owned())
        })
        .collect()
}

pub(crate) async fn find_stale(
    b: &Boundaries,
    env: &BTreeMap<String, String>,
    gateway: &str,
) -> Result<Stale, String> {
    let query = jackin_openshell::SandboxQuery::Labels(jackin_core::LabelSelector::managed());
    let views = b
        .gateway
        .find_sandboxes(&query, &b.cancel)
        .await
        .map_err(|error| error.to_string())?;
    let sandboxes = views
        .iter()
        .filter(|view| {
            labels_of(view)
                .get("jackin.dev/workspace")
                .is_some_and(|workspace| workspace.starts_with("e2e-"))
        })
        .map(|view| view.name.clone())
        .collect();
    let providers = list_providers(b, env, gateway)
        .await?
        .into_iter()
        .filter(|name| is_e2e_provider(name))
        .collect();
    Ok(Stale {
        sandboxes,
        providers,
    })
}

#[derive(Debug)]
pub(crate) enum SweepError {
    Unavailable(String),
    Failed(String),
}

pub(crate) async fn sweep(
    args: &E2eArgs,
    b: &Boundaries,
    env: &BTreeMap<String, String>,
) -> Result<String, SweepError> {
    let stale = find_stale(b, env, &args.gateway)
        .await
        .map_err(SweepError::Unavailable)?;
    let mut failures = Vec::new();
    for name in &stale.sandboxes {
        if let Err(error) = b.gateway.delete_sandbox(name).await {
            failures.push(format!("sandbox {name}: {error}"));
        }
    }
    for name in &stale.providers {
        match ProviderName::parse(name) {
            Ok(provider) => {
                if let Err(error) = b.gateway.delete_provider(&provider).await {
                    failures.push(format!("provider {name}: {error}"));
                }
            }
            Err(error) => failures.push(format!("provider {name}: {error}")),
        }
    }
    if failures.is_empty() {
        Ok(format!("{} removed", stale.describe()))
    } else {
        Err(SweepError::Failed(failures.join("; ")))
    }
}
