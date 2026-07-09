use std::collections::BTreeMap;
use std::sync::Mutex;

use jackin_config::{AppConfig, RoleSource, WorkspaceConfig, WorkspaceRoleOverride};
use jackin_core::{EnvValue, Extended, OpRef};
use jackin_protocol::ExecKind;

use super::*;
use crate::op_runner::OpRunner;

#[derive(Debug, Default)]
struct FakeOpRunner {
    values: BTreeMap<(String, Option<String>), anyhow::Result<String>>,
    calls: Mutex<Vec<(String, Option<String>)>>,
}

impl FakeOpRunner {
    fn with_value(mut self, reference: &str, account: Option<&str>, value: &str) -> Self {
        self.values.insert(
            (reference.to_owned(), account.map(str::to_owned)),
            Ok(value.to_owned()),
        );
        self
    }

    fn with_error(mut self, reference: &str, account: Option<&str>, message: &str) -> Self {
        self.values.insert(
            (reference.to_owned(), account.map(str::to_owned)),
            Err(anyhow::anyhow!(message.to_owned())),
        );
        self
    }

    fn calls(&self) -> Vec<(String, Option<String>)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl OpRunner for FakeOpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        self.read_with_account(reference, None)
    }

    fn read_with_account(&self, reference: &str, account: Option<&str>) -> anyhow::Result<String> {
        let account = account.map(str::to_owned);
        self.calls
            .lock()
            .expect("calls lock")
            .push((reference.to_owned(), account.clone()));
        match self.values.get(&(reference.to_owned(), account)) {
            Some(Ok(value)) => Ok(value.clone()),
            Some(Err(error)) => Err(anyhow::anyhow!(error.to_string())),
            None => anyhow::bail!("not found"),
        }
    }
}

fn op_ref(name: &str, account: Option<&str>, on_demand: bool) -> EnvValue {
    EnvValue::OpRef(OpRef {
        op: format!("op://vault/item/{name}"),
        path: format!("Vault/Item/{name}"),
        account: account.map(str::to_owned),
        on_demand,
    })
}

fn host_env(_name: &str) -> Result<String, std::env::VarError> {
    Err(std::env::VarError::NotPresent)
}

#[test]
fn op_miss_returns_aggregated_error() {
    let mut config = AppConfig::default();
    config
        .env
        .insert("API_TOKEN".into(), op_ref("missing", None, false));
    let runner = FakeOpRunner::default().with_error("op://vault/item/missing", None, "no item");

    let err = resolve_operator_env_with(&config, None, None, &runner, host_env)
        .expect_err("missing op ref should fail");
    let message = err.to_string();

    assert!(message.contains("operator env resolution failed for 1 var(s)"));
    assert!(message.contains("API_TOKEN"));
    assert!(message.contains("no item"));
}

#[test]
fn timeout_error_is_surfaced_not_blank() {
    let mut config = AppConfig::default();
    config
        .env
        .insert("API_TOKEN".into(), op_ref("slow", None, false));
    let runner = FakeOpRunner::default().with_error(
        "op://vault/item/slow",
        None,
        "1Password CLI timed out after 120s resolving",
    );

    let err = resolve_operator_env_with(&config, None, None, &runner, host_env)
        .expect_err("timeout should fail");

    assert!(err.to_string().contains("timed out"), "{err:#}");
}

#[test]
fn empty_op_value_resolves_as_empty_string() {
    let mut config = AppConfig::default();
    config
        .env
        .insert("EMPTY_TOKEN".into(), op_ref("empty", None, false));
    let runner = FakeOpRunner::default().with_value("op://vault/item/empty", None, "");

    let resolved =
        resolve_operator_env_with(&config, None, None, &runner, host_env).expect("resolve env");

    assert_eq!(resolved.get("EMPTY_TOKEN").map(String::as_str), Some(""));
}

#[test]
fn multi_account_refs_resolve_with_their_own_accounts() {
    let mut config = AppConfig::default();
    config
        .env
        .insert("TOKEN_A".into(), op_ref("shared", Some("acct-a"), false));
    config
        .env
        .insert("TOKEN_B".into(), op_ref("shared", Some("acct-b"), false));
    let runner = FakeOpRunner::default()
        .with_value("op://vault/item/shared", Some("acct-a"), "value-a")
        .with_value("op://vault/item/shared", Some("acct-b"), "value-b");

    let resolved =
        resolve_operator_env_with(&config, None, None, &runner, host_env).expect("resolve env");

    assert_eq!(resolved.get("TOKEN_A").map(String::as_str), Some("value-a"));
    assert_eq!(resolved.get("TOKEN_B").map(String::as_str), Some("value-b"));
    let calls = runner.calls();
    assert!(calls.contains(&(
        "op://vault/item/shared".to_owned(),
        Some("acct-a".to_owned())
    )));
    assert!(calls.contains(&(
        "op://vault/item/shared".to_owned(),
        Some("acct-b".to_owned())
    )));
}

#[test]
fn role_scoped_secret_is_not_resolved_for_other_role() {
    let mut config = AppConfig::default();
    config.roles.insert(
        "alpha".into(),
        RoleSource {
            git: "https://example.invalid/alpha.git".into(),
            trusted: true,
            env: BTreeMap::from([("ALPHA_TOKEN".into(), op_ref("alpha", None, false))]),
        },
    );
    config.roles.insert(
        "beta".into(),
        RoleSource {
            git: "https://example.invalid/beta.git".into(),
            trusted: true,
            env: BTreeMap::new(),
        },
    );
    let runner = FakeOpRunner::default().with_value("op://vault/item/alpha", None, "alpha-secret");

    let resolved = resolve_operator_env_with(&config, Some("beta"), None, &runner, host_env)
        .expect("resolve env");

    assert!(resolved.is_empty());
    assert!(runner.calls().is_empty());
}

#[test]
fn collect_on_demand_bindings_keeps_sources_without_resolving_values() {
    let mut config = AppConfig::default();
    config.workspaces.insert(
        "work".into(),
        WorkspaceConfig {
            workdir: "/workspace".into(),
            roles: BTreeMap::from([(
                "alpha".into(),
                WorkspaceRoleOverride {
                    env: BTreeMap::from([
                        ("OP_TOKEN".into(), op_ref("exec", None, true)),
                        (
                            "HOST_TOKEN".into(),
                            EnvValue::Extended(Extended {
                                value: "$HOST_TOKEN".into(),
                                on_demand: true,
                            }),
                        ),
                        (
                            "LITERAL_TOKEN".into(),
                            EnvValue::Extended(Extended {
                                value: "literal-secret".into(),
                                on_demand: true,
                            }),
                        ),
                    ]),
                    ..WorkspaceRoleOverride::default()
                },
            )]),
            ..WorkspaceConfig::default()
        },
    );

    let bindings = collect_on_demand_bindings(&config, Some("alpha"), Some("work"));

    assert_eq!(bindings.len(), 3);
    assert_eq!(bindings[0].name, "HOST_TOKEN");
    assert_eq!(bindings[0].kind, ExecKind::Env);
    assert_eq!(bindings[1].name, "LITERAL_TOKEN");
    assert_eq!(bindings[1].kind, ExecKind::Literal);
    assert_eq!(bindings[2].name, "OP_TOKEN");
    assert_eq!(bindings[2].kind, ExecKind::Op);
}

#[test]
fn validate_reserved_names_rejects_runtime_env_names() {
    let mut config = AppConfig::default();
    config.env.insert("JACKIN_AGENT".into(), "operator".into());
    config.env.insert("SAFE_OPERATOR_ENV".into(), "ok".into());

    let err = validate_reserved_names(&config).expect_err("reserved name should fail");

    assert!(err.to_string().contains("JACKIN_AGENT"), "{err:#}");
    assert!(!err.to_string().contains("SAFE_OPERATOR_ENV"), "{err:#}");
}

#[test]
fn validate_reserved_names_accepts_non_reserved_names() {
    let mut config = AppConfig::default();
    config.env.insert("SAFE_OPERATOR_ENV".into(), "ok".into());

    validate_reserved_names(&config).expect("non-reserved env should pass");
}
