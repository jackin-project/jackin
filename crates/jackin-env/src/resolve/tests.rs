use std::collections::BTreeMap;
use std::sync::Mutex;

use jackin_config::{AppConfig, RoleSource, WorkspaceConfig, WorkspaceRoleOverride};
use jackin_core::{EnvValue, Extended, OpRef, WorkspaceName};
use jackin_protocol::ExecKind;

use super::*;
use crate::op_runner::OpRunner;

fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

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

    let bindings = collect_on_demand_bindings(&config, Some("alpha"), Some(&wn("work")));

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

#[test]
fn operator_env_error_message_parity_variants() {
    assert_eq!(
        OperatorEnvError::NotOpRef {
            value: "plain".into()
        }
        .to_string(),
        "not an op:// reference: plain"
    );
    assert_eq!(
        OperatorEnvError::ShellVarInRef {
            value: "op://v/${x}/f".into()
        }
        .to_string(),
        "jackin does not support shell variable substitution inside `op://` URIs \
         (`op://v/${x}/f`). Use a plain string value, or substitute before passing."
    );
    assert_eq!(
        OperatorEnvError::MalformedRef {
            value: "op://only/two".into()
        }
        .to_string(),
        "malformed op:// URI (expected 3 or 4 path segments): op://only/two"
    );
    assert_eq!(
        OperatorEnvError::VaultNotFound {
            vault: "missing".into()
        }
        .to_string(),
        "vault not found: \"missing\""
    );
    assert_eq!(
        OperatorEnvError::ItemNotFound {
            item: "tok".into(),
            vault: "Personal".into()
        }
        .to_string(),
        "item \"tok\" not found in vault \"Personal\""
    );
    assert_eq!(
        OperatorEnvError::AmbiguousItem {
            count: 2,
            item: "tok".into(),
            vault: "Personal".into(),
            suggestions: "  op://Personal/tok[a]\n  op://Personal/tok[b]".into(),
        }
        .to_string(),
        "2 items named \"tok\" in vault \"Personal\". Disambiguate with:\n  \
         op://Personal/tok[a]\n  op://Personal/tok[b]"
    );
    assert_eq!(
        OperatorEnvError::FieldNotFound {
            field: "password".into(),
            item: "tok".into()
        }
        .to_string(),
        "field \"password\" not found in item \"tok\""
    );
    assert_eq!(
        OperatorEnvError::Aggregated {
            count: 1,
            summary: "  - API_TOKEN: boom".into()
        }
        .to_string(),
        "operator env resolution failed for 1 var(s):\n  - API_TOKEN: boom"
    );
    let reserved = OperatorEnvError::ReservedNames {
        count: 1,
        details: "  - \"JACKIN_AGENT\" is reserved by the jackin runtime; declared in global"
            .into(),
    };
    assert!(
        reserved
            .to_string()
            .starts_with("operator env map contains 1 reserved runtime name(s):\n"),
        "{reserved}"
    );
}

#[test]
fn aggregated_resolution_error_is_typed_source() {
    let mut config = AppConfig::default();
    config
        .env
        .insert("API_TOKEN".into(), op_ref("missing", None, false));
    let runner = FakeOpRunner::default().with_error("op://vault/item/missing", None, "no item");

    let err = resolve_operator_env_with(&config, None, None, &runner, host_env)
        .expect_err("missing op ref should fail");
    let typed = err
        .downcast_ref::<OperatorEnvError>()
        .expect("OperatorEnvError source");
    match typed {
        OperatorEnvError::Aggregated { count, summary } => {
            assert_eq!(*count, 1);
            assert!(summary.contains("API_TOKEN"), "{summary}");
            assert!(summary.contains("no item"), "{summary}");
        }
        other => panic!("expected Aggregated, got {other}"),
    }
}

/// Property: any reserved runtime name in global env is rejected.
#[test]
fn prop_reserved_names_always_rejected() {
    use jackin_core::RESERVED_RUNTIME_ENV_VARS;
    use proptest::prelude::*;

    let reserved: Vec<&'static str> = RESERVED_RUNTIME_ENV_VARS.iter().map(|(n, _)| *n).collect();

    proptest!(|(idx in 0usize..reserved.len())| {
        let key = reserved[idx];
        let mut config = AppConfig::default();
        config
            .env
            .insert(key.to_owned(), EnvValue::Plain("x".into()));
        let err = validate_reserved_names(&config).expect_err("reserved must fail");
        match err {
            OperatorEnvError::ReservedNames { count, details } => {
                prop_assert!(count >= 1);
                prop_assert!(details.contains(key), "{details}");
            }
            other => prop_assert!(false, "unexpected error: {other}"),
        }
    });
}

/// Property: non-reserved names are accepted by the reserved-name gate.
#[test]
fn prop_non_reserved_names_accepted() {
    use jackin_core::is_reserved;
    use proptest::prelude::*;

    proptest!(|(name in "[A-Z][A-Z0-9_]{0,24}")| {
        prop_assume!(!is_reserved(&name));
        let mut config = AppConfig::default();
        config
            .env
            .insert(name, EnvValue::Plain("ok".into()));
        prop_assert!(validate_reserved_names(&config).is_ok());
    });
}

/// Property: later selected layers always override earlier layers.
#[test]
fn prop_operator_env_follows_declared_layer_precedence() {
    use proptest::prelude::*;

    proptest!(|(
        global in "[a-zA-Z0-9_-]{0,24}",
        role in "[a-zA-Z0-9_-]{0,24}",
        workspace in "[a-zA-Z0-9_-]{0,24}",
        workspace_role in "[a-zA-Z0-9_-]{0,24}",
    )| {
        let key = "LAYERED_VALUE";
        let mut config = AppConfig::default();
        config.env.insert(key.into(), EnvValue::Plain(global));
        config.roles.insert(
            "alpha".into(),
            RoleSource {
                git: "https://example.invalid/alpha.git".into(),
                trusted: true,
                env: BTreeMap::from([(key.into(), EnvValue::Plain(role))]),
            },
        );
        config.workspaces.insert(
            "work".into(),
            WorkspaceConfig {
                workdir: "/workspace".into(),
                env: BTreeMap::from([(key.into(), EnvValue::Plain(workspace))]),
                roles: BTreeMap::from([(
                    "alpha".into(),
                    WorkspaceRoleOverride {
                        env: BTreeMap::from([(
                            key.into(),
                            EnvValue::Plain(workspace_role.clone()),
                        )]),
                        ..WorkspaceRoleOverride::default()
                    },
                )]),
                ..WorkspaceConfig::default()
            },
        );

        prop_assert_eq!(
            lookup_operator_env_raw(&config, Some("alpha"), Some(&wn("work")), key),
            Some(workspace_role),
        );
    });
}
