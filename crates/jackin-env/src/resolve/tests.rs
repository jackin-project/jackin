#[cfg(test)]
use super::*;
use jackin_core::{JackinPaths, OpRef};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

static ACTIVE_RUN_TEST_LOCK: Mutex<()> = Mutex::new(());

struct FakeOpRunner;

impl OpRunner for FakeOpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        Ok(format!("secret-for-{reference}"))
    }
}

#[test]
fn collect_on_demand_bindings_extracts_kinds_and_excludes_always_available() {
    let mut config = AppConfig::default();
    // Always-available values must never appear as on-demand bindings.
    config
        .env
        .insert("PLAIN".to_owned(), EnvValue::Plain("v".to_owned()));
    config.env.insert(
        "OP_ALWAYS".to_owned(),
        EnvValue::OpRef(OpRef {
            op: "op://v/i/f".to_owned(),
            path: "V/I/F".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    // One on-demand value per kind.
    config.env.insert(
        "OP_DEMAND".to_owned(),
        EnvValue::OpRef(OpRef {
            op: "op://v/i/key".to_owned(),
            path: "V/I/Key".to_owned(),
            account: None,
            on_demand: true,
        }),
    );
    config.env.insert(
        "ENV_DEMAND".to_owned(),
        EnvValue::Extended(jackin_core::Extended {
            value: "$HOST_TOK".to_owned(),
            on_demand: true,
        }),
    );
    config.env.insert(
        "LIT_DEMAND".to_owned(),
        EnvValue::Extended(jackin_core::Extended {
            value: "literal".to_owned(),
            on_demand: true,
        }),
    );

    let bindings = collect_on_demand_bindings(&config, None, None);
    assert_eq!(
        bindings,
        vec![
            jackin_protocol::ExecBinding {
                name: "ENV_DEMAND".to_owned(),
                kind: jackin_protocol::ExecKind::Env,
                source: "$HOST_TOK".to_owned(),
            },
            jackin_protocol::ExecBinding {
                name: "LIT_DEMAND".to_owned(),
                kind: jackin_protocol::ExecKind::Literal,
                source: "literal".to_owned(),
            },
            jackin_protocol::ExecBinding {
                name: "OP_DEMAND".to_owned(),
                kind: jackin_protocol::ExecKind::Op,
                source: "op://v/i/key".to_owned(),
            },
        ]
    );
}

#[test]
fn resolve_operator_env_skips_on_demand_vars() {
    let mut config = AppConfig::default();
    config
        .env
        .insert("ALWAYS".to_owned(), EnvValue::Plain("here".to_owned()));
    // An on-demand op ref must NOT be resolved at launch — that would run
    // `op read` for a credential the agent should only get via jackin-exec.
    config.env.insert(
        "SECRET".to_owned(),
        EnvValue::OpRef(OpRef {
            op: "op://v/i/f".to_owned(),
            path: "V/I/F".to_owned(),
            account: None,
            on_demand: true,
        }),
    );
    let resolved = resolve_operator_env_with(&config, None, None, &FakeOpRunner, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .expect("resolution must succeed");
    assert_eq!(resolved.get("ALWAYS").map(String::as_str), Some("here"));
    assert!(
        !resolved.contains_key("SECRET"),
        "on_demand var must be filtered out of launch-time resolution"
    );
}

#[test]
fn has_operator_env_tracks_applicable_layers_without_resolution() {
    let mut config = AppConfig::default();
    assert!(!has_operator_env(
        &config,
        Some("agent-smith"),
        Some("workspace")
    ));

    config.env.insert(
        "GLOBAL_TOKEN".to_owned(),
        EnvValue::Plain("global".to_owned()),
    );
    assert!(has_operator_env(
        &config,
        Some("agent-smith"),
        Some("workspace")
    ));
    config.env.clear();

    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://example.invalid/agent-smith.git".to_owned(),
            trusted: true,
            env: [("ROLE_TOKEN".to_owned(), EnvValue::Plain("role".to_owned()))].into(),
        },
    );
    assert!(has_operator_env(
        &config,
        Some("agent-smith"),
        Some("workspace")
    ));
    assert!(!has_operator_env(
        &config,
        Some("other-role"),
        Some("workspace")
    ));

    {
        let workspace = config.workspaces.entry("workspace".to_owned()).or_default();
        workspace.env.insert(
            "WORKSPACE_TOKEN".to_owned(),
            EnvValue::Plain("workspace".to_owned()),
        );
    }
    assert!(has_operator_env(
        &config,
        Some("other-role"),
        Some("workspace")
    ));
    assert!(!has_operator_env(
        &config,
        Some("other-role"),
        Some("other-workspace")
    ));

    {
        let workspace = config.workspaces.entry("workspace".to_owned()).or_default();
        workspace.env.clear();
        workspace.roles.insert(
            "agent-smith".to_owned(),
            jackin_config::WorkspaceRoleOverride {
                env: [(
                    "WORKSPACE_ROLE_TOKEN".to_owned(),
                    EnvValue::Plain("workspace-role".to_owned()),
                )]
                .into(),
                ..Default::default()
            },
        );
    }
    assert!(has_operator_env(
        &config,
        Some("agent-smith"),
        Some("workspace")
    ));
    assert!(!has_operator_env(
        &config,
        Some("other-role"),
        Some("workspace")
    ));
}

struct ConcurrentOpRunner {
    active: AtomicUsize,
    max_active: AtomicUsize,
}

impl ConcurrentOpRunner {
    const fn new() -> Self {
        Self {
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
        }
    }

    fn record_active(&self, active: usize) {
        let mut observed = self.max_active.load(Ordering::SeqCst);
        while active > observed {
            match self.max_active.compare_exchange(
                observed,
                active,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return,
                Err(next) => observed = next,
            }
        }
    }
}

impl OpRunner for ConcurrentOpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.record_active(active);
        #[expect(
            clippy::disallowed_methods,
            reason = "test runner deliberately holds worker OS threads open to prove overlap"
        )]
        std::thread::sleep(std::time::Duration::from_millis(25));
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(format!("secret-for-{reference}"))
    }
}

#[test]
fn operator_env_resolution_emits_per_key_timings_without_values() {
    let _lock = ACTIVE_RUN_TEST_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let run = jackin_diagnostics::RunDiagnostics::start(&paths, true, "load").unwrap();
    let _active = run.activate();
    let mut config = AppConfig::default();
    config.env.insert(
        "LITERAL_TOKEN".to_owned(),
        EnvValue::Plain("literal-secret".to_owned()),
    );
    config.env.insert(
        "HOST_TOKEN".to_owned(),
        EnvValue::Plain("$HOST_TOKEN".to_owned()),
    );
    config.env.insert(
        "OP_TOKEN".to_owned(),
        EnvValue::OpRef(OpRef {
            op: "op://vault/item/field".to_owned(),
            path: "Vault/Item/Field".to_owned(),
            account: None,
            on_demand: false,
        }),
    );

    let resolved =
        resolve_operator_env_with(&config, None, None, &FakeOpRunner, |name| match name {
            "HOST_TOKEN" => Ok("host-secret".to_owned()),
            _ => Err(std::env::VarError::NotPresent),
        })
        .unwrap();

    assert_eq!(resolved["LITERAL_TOKEN"], "literal-secret");
    assert_eq!(resolved["HOST_TOKEN"], "host-secret");
    assert_eq!(resolved["OP_TOKEN"], "secret-for-op://vault/item/field");
    let contents = std::fs::read_to_string(run.path()).unwrap();
    for key in ["LITERAL_TOKEN", "HOST_TOKEN", "OP_TOKEN"] {
        assert!(
            contents.contains(&format!("operator_env:{key}")),
            "missing timing for {key}: {contents}"
        );
    }
    for detail in ["literal", "host", "op"] {
        assert!(
            contents.contains(&format!(r#"\"detail\":\"{detail}\""#)),
            "{contents}"
        );
    }
    for secret in [
        "literal-secret",
        "host-secret",
        "secret-for-op://vault/item/field",
    ] {
        assert!(
            !contents.contains(secret),
            "operator env timing must not leak {secret}: {contents}"
        );
    }
}

#[test]
fn operator_env_resolution_reads_independent_op_refs_concurrently() {
    let mut config = AppConfig::default();
    for key in ["FIRST_TOKEN", "SECOND_TOKEN", "THIRD_TOKEN"] {
        config.env.insert(
            key.to_owned(),
            EnvValue::OpRef(OpRef {
                op: format!("op://vault/item/{key}"),
                path: format!("Vault/Item/{key}"),
                account: None,
                on_demand: false,
            }),
        );
    }

    let runner = ConcurrentOpRunner::new();
    let resolved = resolve_operator_env_with(&config, None, None, &runner, |_name| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();

    assert_eq!(resolved.len(), 3);
    assert!(
        runner.max_active.load(Ordering::SeqCst) > 1,
        "expected overlapping op reads"
    );
}
}
