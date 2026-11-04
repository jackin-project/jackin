// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    ProcessError, ProcessRunner, ProcessSpec, ProcessStatus, ProcessStdin, Program, plain,
};
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::credentials::Document;

const OP_TIMEOUT: Duration = Duration::from_secs(30);
const TAIL_CHARS: usize = 4;
const TAIL_MIN_CHARS: usize = 8;
const END_OF_FLAGS: &str = "--";
const FORMAT: [&str; 2] = ["--format", "json"];
const ACCOUNT: &str = "--account";
const LOCKED: [&str; 3] = [
    "not currently signed in",
    "not signed in",
    "session expired",
];
const UNAUTHORIZED: [&str; 3] = ["authorization", "authorize", "approve"];
const FORBIDDEN: [&str; 4] = [
    "do not have permission",
    "don't have permission",
    "forbidden",
    "access denied",
];
const NO_VAULT: &str = "isn't a vault";
const NO_ITEM: &str = "isn't an item";
const NO_ACCOUNT: &str = "isn't an account";

#[derive(Debug, Error)]
pub enum OpFault {
    #[error("1Password is locked")]
    Locked,
    #[error("the 1Password command line tool cannot run: {}", plain(.0))]
    NotInstalled(String),
    #[error("1Password needs your approval for account {}", plain(.account))]
    AuthorizationRequired { account: String },
    #[error("no access to vault {}", plain(.vault))]
    PermissionDenied { vault: String },
    #[error("1Password account {} not found", plain(.account))]
    MissingAccount { account: String },
    #[error("1Password vault {} not found", plain(.vault))]
    MissingVault { vault: String },
    #[error("1Password item {} not found in vault {}", plain(.item), plain(.vault))]
    MissingItem { item: String, vault: String },
    #[error("1Password field {} not found in item {}", plain(.field), plain(.item))]
    MissingField { field: String, item: String },
    #[error("the 1Password name {} starts with a dash", plain(.name))]
    FlagLikeName { name: String },
    #[error("the 1Password command line tool printed something unexpected")]
    Malformed,
    #[error("the 1Password command line tool failed")]
    Failed,
    #[error(transparent)]
    Process(ProcessError),
}

impl OpFault {
    pub fn is_cancelled(&self) -> bool {
        matches!(self, OpFault::Process(ProcessError::Cancelled { .. }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpAccountInfo {
    pub id: String,
    pub email: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpVaultInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpItemInfo {
    pub id: String,
    pub title: String,
    pub category: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpFieldKind {
    Concealed,
    Text,
    Url,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpFieldInfo {
    pub id: String,
    pub label: String,
    pub kind: OpFieldKind,
    pub section: Option<String>,
    pub tail: String,
    pub empty: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpItemDetail {
    pub id: String,
    pub title: String,
    pub fields: Vec<OpFieldInfo>,
}

#[derive(Clone, Copy, Debug)]
pub struct OpScope<'a> {
    pub account: &'a str,
    pub vault: Option<&'a str>,
    pub item: Option<&'a str>,
}

#[derive(Clone, Debug)]
pub struct OnePasswordCli {
    runner: Arc<dyn ProcessRunner>,
}

fn contains_any(text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| text.contains(marker))
}

fn classify(stderr: &[u8], scope: OpScope<'_>) -> OpFault {
    let text = Zeroizing::new(String::from_utf8_lossy(stderr).to_lowercase());
    let vault = || scope.vault.unwrap_or_default().to_owned();
    if text.contains(NO_VAULT) {
        OpFault::MissingVault { vault: vault() }
    } else if text.contains(NO_ITEM) {
        OpFault::MissingItem {
            item: scope.item.unwrap_or_default().to_owned(),
            vault: vault(),
        }
    } else if text.contains(NO_ACCOUNT) {
        OpFault::MissingAccount {
            account: scope.account.to_owned(),
        }
    } else if contains_any(&text, &LOCKED) {
        OpFault::Locked
    } else if contains_any(&text, &UNAUTHORIZED) {
        OpFault::AuthorizationRequired {
            account: scope.account.to_owned(),
        }
    } else if contains_any(&text, &FORBIDDEN) {
        OpFault::PermissionDenied { vault: vault() }
    } else {
        OpFault::Failed
    }
}

fn flag_like(scope: OpScope<'_>) -> Option<&str> {
    [Some(scope.account), scope.vault, scope.item]
        .into_iter()
        .flatten()
        .find(|name| name.starts_with('-'))
}

fn text_of<'a>(object: &'a Value, key: &str) -> Result<&'a str, OpFault> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(OpFault::Malformed)
}

fn rows(document: &Document) -> Result<&Vec<Value>, OpFault> {
    document.0.as_array().ok_or(OpFault::Malformed)
}

fn kind_of(kind: &str) -> OpFieldKind {
    match kind {
        "CONCEALED" => OpFieldKind::Concealed,
        "URL" => OpFieldKind::Url,
        _ => OpFieldKind::Text,
    }
}

fn tail_of(value: &str) -> String {
    let count = value.chars().count();
    if count <= TAIL_MIN_CHARS {
        return String::new();
    }
    value
        .chars()
        .skip(count.saturating_sub(TAIL_CHARS))
        .collect()
}

fn field(raw: &Value) -> Result<OpFieldInfo, OpFault> {
    let id = text_of(raw, "id")?;
    let kind = kind_of(raw.get("type").and_then(Value::as_str).unwrap_or_default());
    let value = raw.get("value").and_then(Value::as_str).unwrap_or_default();
    let tail = Zeroizing::new(if kind == OpFieldKind::Concealed {
        tail_of(value)
    } else {
        String::new()
    });
    Ok(OpFieldInfo {
        id: id.to_owned(),
        label: raw
            .get("label")
            .and_then(Value::as_str)
            .filter(|label| !label.is_empty())
            .unwrap_or(id)
            .to_owned(),
        kind,
        section: raw
            .get("section")
            .and_then(|section| section.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        tail: tail.as_str().to_owned(),
        empty: value.is_empty(),
    })
}

impl OnePasswordCli {
    pub fn new(runner: Arc<dyn ProcessRunner>) -> OnePasswordCli {
        OnePasswordCli { runner }
    }

    async fn run(
        &self,
        arguments: &[&str],
        positional: &[&str],
        scope: OpScope<'_>,
        cancel: &CancellationToken,
    ) -> Result<Document, OpFault> {
        if let Some(name) = flag_like(scope) {
            return Err(OpFault::FlagLikeName {
                name: name.to_owned(),
            });
        }
        let mut args: Vec<String> = arguments.iter().map(|arg| (*arg).to_owned()).collect();
        if !scope.account.is_empty() {
            args.extend([ACCOUNT.to_owned(), scope.account.to_owned()]);
        }
        args.extend(FORMAT.map(str::to_owned));
        if !positional.is_empty() {
            args.push(END_OF_FLAGS.to_owned());
            args.extend(positional.iter().map(|arg| (*arg).to_owned()));
        }
        let spec = ProcessSpec {
            program: Program::Op,
            args,
            cwd: None,
            env: Vec::new(),
            env_remove_prefixes: Vec::new(),
            stdin: ProcessStdin::Null,
            timeout: OP_TIMEOUT,
        };
        let output = self
            .runner
            .run_secret(&spec, cancel)
            .await
            .map_err(|error| match error {
                ProcessError::Spawn { message, .. } => OpFault::NotInstalled(message),
                other => OpFault::Process(other),
            })?;
        if output.status != ProcessStatus::Exited(0) {
            return Err(classify(&output.stderr, scope));
        }
        let value =
            serde_json::from_slice(output.stdout.expose()).map_err(|_| OpFault::Malformed)?;
        Ok(Document(value))
    }

    pub async fn accounts(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<OpAccountInfo>, OpFault> {
        let scope = OpScope {
            account: "",
            vault: None,
            item: None,
        };
        let document = self.run(&["account", "list"], &[], scope, cancel).await?;
        rows(&document)?
            .iter()
            .map(|raw| {
                Ok(OpAccountInfo {
                    id: text_of(raw, "account_uuid")?.to_owned(),
                    email: text_of(raw, "email")?.to_owned(),
                })
            })
            .collect()
    }

    pub async fn vaults(
        &self,
        account: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<OpVaultInfo>, OpFault> {
        let scope = OpScope {
            account,
            vault: None,
            item: None,
        };
        let document = self.run(&["vault", "list"], &[], scope, cancel).await?;
        rows(&document)?
            .iter()
            .map(|raw| {
                Ok(OpVaultInfo {
                    id: text_of(raw, "id")?.to_owned(),
                    name: text_of(raw, "name")?.to_owned(),
                })
            })
            .collect()
    }

    pub async fn items(
        &self,
        account: &str,
        vault: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<OpItemInfo>, OpFault> {
        let scope = OpScope {
            account,
            vault: Some(vault),
            item: None,
        };
        let document = self
            .run(&["item", "list", "--vault", vault], &[], scope, cancel)
            .await?;
        rows(&document)?
            .iter()
            .map(|raw| {
                Ok(OpItemInfo {
                    id: text_of(raw, "id")?.to_owned(),
                    title: text_of(raw, "title")?.to_owned(),
                    category: raw
                        .get("category")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                })
            })
            .collect()
    }

    pub async fn item(
        &self,
        account: &str,
        vault: &str,
        item: &str,
        cancel: &CancellationToken,
    ) -> Result<OpItemDetail, OpFault> {
        let scope = OpScope {
            account,
            vault: Some(vault),
            item: Some(item),
        };
        let document = self
            .run(&["item", "get", "--vault", vault], &[item], scope, cancel)
            .await?;
        let raw = &document.0;
        let fields = raw
            .get("fields")
            .and_then(Value::as_array)
            .map_or(Ok(Vec::new()), |fields| {
                fields.iter().map(field).collect::<Result<Vec<_>, _>>()
            })?;
        Ok(OpItemDetail {
            id: text_of(raw, "id")?.to_owned(),
            title: text_of(raw, "title")?.to_owned(),
            fields,
        })
    }
}
