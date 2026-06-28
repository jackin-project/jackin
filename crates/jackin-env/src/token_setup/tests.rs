//! Tests for `token setup`.
use super::*;
use jackin_config::{AppConfig, WorkspaceConfig};
use jackin_core::OpRef;
use std::cell::RefCell;
use std::sync::Mutex;
use tempfile::tempdir;

struct FakeOpWriter {
    last_create: RefCell<Option<(String, String, String)>>, // (vault, title, field)
    produced_ref: OpRef,
    recorded_value: RefCell<Option<String>>,
    /// Records the `field_id` passed to the last `item_field_set`
    /// call so the edit-existing threading can be asserted. Outer
    /// `Option` = was the method called; inner = the `Option<&str>` arg.
    #[allow(
        clippy::option_option,
        reason = "outer = call-recorded, inner = the Option<&str> arg"
    )]
    recorded_field_id: RefCell<Option<Option<String>>>,
    /// When `true`, `item_create` returns Err instead of recording.
    fail_create: bool,
    /// When `true`, `item_delete` records the call AND returns Err
    /// so revoke-error paths are exercisable.
    fail_delete: bool,
    /// Records every `item_delete` call so cleanup-on-failure
    /// paths can be asserted.
    deletes: RefCell<Vec<(String, String)>>,
}

impl FakeOpWriter {
    fn new(produced_ref: OpRef) -> Self {
        Self {
            last_create: RefCell::new(None),
            produced_ref,
            recorded_value: RefCell::new(None),
            recorded_field_id: RefCell::new(None),
            fail_create: false,
            fail_delete: false,
            deletes: RefCell::new(Vec::new()),
        }
    }
    fn failing() -> Self {
        Self {
            last_create: RefCell::new(None),
            produced_ref: OpRef {
                op: "op://_/_/_".into(),
                path: "_/_/_".into(),
                account: None,
                on_demand: false,
            },
            recorded_value: RefCell::new(None),
            recorded_field_id: RefCell::new(None),
            fail_create: true,
            fail_delete: false,
            deletes: RefCell::new(Vec::new()),
        }
    }
    fn with_failing_delete(mut self) -> Self {
        self.fail_delete = true;
        self
    }
}

impl OpWriteRunner for FakeOpWriter {
    fn item_create(
        &self,
        params: crate::op_struct::OpItemCreateParams<'_>,
    ) -> anyhow::Result<OpRef> {
        if self.fail_create {
            anyhow::bail!("simulated item_create failure");
        }
        *self.last_create.borrow_mut() = Some((
            params.vault_id.to_owned(),
            params.title.to_owned(),
            params.field_label.to_owned(),
        ));
        *self.recorded_value.borrow_mut() = Some(params.value.to_owned());
        Ok(self.produced_ref.clone())
    }
    fn item_delete(&self, item_id: &str, vault_id: &str, _: Option<&str>) -> anyhow::Result<()> {
        self.deletes
            .borrow_mut()
            .push((vault_id.to_owned(), item_id.to_owned()));
        if self.fail_delete {
            anyhow::bail!("simulated item_delete failure");
        }
        Ok(())
    }
    fn item_field_set(
        &self,
        _item_id: &str,
        _vault_id: &str,
        target: &FieldTarget,
        value: &str,
        _section: Option<&str>,
    ) -> anyhow::Result<OpRef> {
        if self.fail_create {
            anyhow::bail!("simulated item_field_set failure");
        }
        *self.last_create.borrow_mut() = Some((
            "existing-vault".to_owned(),
            "existing-item".to_owned(),
            target.label().to_owned(),
        ));
        *self.recorded_field_id.borrow_mut() = Some(target.id().map(str::to_owned));
        *self.recorded_value.borrow_mut() = Some(value.to_owned());
        Ok(self.produced_ref.clone())
    }
    fn item_tags(
        &self,
        _item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<Vec<String>> {
        // The setup/rotate-into-fresh-item tests never reach the
        // prior-item ownership check (that lives in app::rotate).
        anyhow::bail!("token_setup tests do not exercise item_tags")
    }
}

struct FakeOpReader {
    /// Per-call queue. Each call pops one. When empty, `read`
    /// reuses the last value indefinitely so single-call tests
    /// can keep using `Self { values: vec![token] }`.
    values: Mutex<Vec<anyhow::Result<String>>>,
    last_ref: Mutex<Vec<String>>,
}
impl FakeOpReader {
    fn ok(value: &str) -> Self {
        Self {
            values: Mutex::new(vec![Ok(value.into())]),
            last_ref: Mutex::new(Vec::new()),
        }
    }
    fn err(msg: &'static str) -> Self {
        Self {
            values: Mutex::new(vec![Err(anyhow::anyhow!(msg))]),
            last_ref: Mutex::new(Vec::new()),
        }
    }
}
impl OpRunner for FakeOpReader {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        self.last_ref.lock().unwrap().push(reference.to_owned());
        let mut q = self.values.lock().unwrap();
        if q.len() == 1 {
            // Stable: keep returning the same value/err forever.
            match &q[0] {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(anyhow::anyhow!(e.to_string())),
            }
        } else {
            q.remove(0)
        }
    }
}

fn workspace(name: &str) -> WorkspaceConfig {
    WorkspaceConfig {
        workdir: format!("/tmp/{name}"),
        ..Default::default()
    }
}

#[test]
fn sha256_prefix_is_stable_12_hex_chars() {
    let p = sha256_prefix("hello");
    assert_eq!(p.len(), 12);
    assert!(p.chars().all(|c| c.is_ascii_hexdigit()));
}

fn seed_paths_with_workspace(name: &str) -> (tempfile::TempDir, JackinPaths, AppConfig) {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    let mut cfg = AppConfig::default();
    cfg.workspaces.insert(name.into(), workspace(name));
    std::fs::write(&paths.config_file, toml::to_string(&cfg).unwrap()).unwrap();
    (temp, paths, cfg)
}

fn dummy_probe() -> host_claude::ClaudeProbe {
    host_claude::ClaudeProbe {
        binary: "claude".into(),
        version: "2.1.4".into(),
    }
}

fn dummy_op_ref() -> OpRef {
    OpRef {
        op: "op://VID/IID/FID".into(),
        path: "Personal/jackin · proj · claude-token/token".into(),
        account: None,
        on_demand: false,
    }
}

#[test]
fn prior_token_slot_reads_the_scoped_slot() {
    use jackin_config::WorkspaceRoleOverride;

    let mut cfg = AppConfig::default();
    let mut ws = workspace("proj");
    ws.env.insert(
        CLAUDE_OAUTH_TOKEN_ENV.to_owned(),
        EnvValue::Plain("ws-level".into()),
    );
    let mut role_override = WorkspaceRoleOverride::default();
    role_override.env.insert(
        CLAUDE_OAUTH_TOKEN_ENV.to_owned(),
        EnvValue::Plain("role-level".into()),
    );
    ws.roles.insert("org/agent".into(), role_override);
    cfg.workspaces.insert("proj".into(), ws);

    // Workspace scope reads the workspace-level slot.
    let ws_scope = TokenSetupScope::Workspace("proj".into());
    assert!(matches!(
        prior_token_slot(&cfg, &ws_scope),
        Some(EnvValue::Plain(v)) if v == "ws-level"
    ));

    // Role scope reads the role override slot, not the workspace one.
    let role_scope = TokenSetupScope::WorkspaceRole {
        workspace: "proj".into(),
        role: "org/agent".into(),
    };
    assert!(matches!(
        prior_token_slot(&cfg, &role_scope),
        Some(EnvValue::Plain(v)) if v == "role-level"
    ));

    // A role with no slot returns None (rotate then needs --vault).
    let empty_role = TokenSetupScope::WorkspaceRole {
        workspace: "proj".into(),
        role: "org/other".into(),
    };
    assert!(prior_token_slot(&cfg, &empty_role).is_none());
}

#[test]
fn run_setup_with_runner_role_scope_wires_role_override_not_workspace() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    let token = "sk-ant-oat01-ROLE";
    let reader = FakeOpReader::ok(token);
    let probe = dummy_probe();

    run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::WorkspaceRole {
            workspace: "proj".into(),
            role: "org/agent".into(),
        },
        &TokenSetupArgs {
            vault: Some("Personal".into()),
            ..Default::default()
        },
        Some(&probe),
        || Ok(secrecy::SecretString::from(token.to_owned())),
        &reader,
        &writer,
    )
    .unwrap();

    let ws = cfg.workspaces.get("proj").unwrap();
    // Token lands in the role override slot...
    let role_val = ws
        .roles
        .get("org/agent")
        .and_then(|r| r.env.get("CLAUDE_CODE_OAUTH_TOKEN"));
    assert!(
        matches!(role_val, Some(EnvValue::OpRef(_))),
        "role-scoped token must wire into roles.<role>.env"
    );
    // ...and NOT the workspace-level slot.
    assert!(
        !ws.env.contains_key("CLAUDE_CODE_OAUTH_TOKEN"),
        "role scope must not write the workspace-level slot"
    );
}

#[test]
fn run_setup_with_runner_creates_item_and_wires_workspace_config() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    let token = "sk-ant-oat01-EXAMPLE";
    let reader = FakeOpReader::ok(token);
    let probe = dummy_probe();

    let report = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            vault: Some("Personal".into()),
            ..Default::default()
        },
        Some(&probe),
        || Ok(secrecy::SecretString::from(token.to_owned())),
        &reader,
        &writer,
    )
    .unwrap();

    // Item create was invoked with the expected vault / title.
    let last = writer.last_create.borrow().clone().unwrap();
    assert_eq!(last.0, "Personal");
    assert_eq!(
        last.1,
        DEFAULT_ITEM_TEMPLATE.replace("{ws}", "proj"),
        "title must be derived from DEFAULT_ITEM_TEMPLATE"
    );
    assert_eq!(last.2, DEFAULT_FIELD_LABEL);
    assert_eq!(writer.recorded_value.borrow().as_deref(), Some(token));
    // No fall-back delete was triggered on the happy path.
    assert!(writer.deletes.borrow().is_empty());

    // auth_forward set on claude block.
    let claude = cfg
        .workspaces
        .get("proj")
        .and_then(|w| w.claude.as_ref())
        .unwrap();
    assert_eq!(claude.auth_forward, AuthForwardMode::OAuthToken);
    // Token stored in env block, not a dedicated field.
    let env_val = cfg
        .workspaces
        .get("proj")
        .and_then(|w| w.env.get("CLAUDE_CODE_OAUTH_TOKEN"));
    assert!(matches!(env_val, Some(EnvValue::OpRef(_))));

    // Report values plumbed through.
    assert_eq!(report.workspace, "proj");
    assert_eq!(report.claude_cli_version.as_deref(), Some("2.1.4"));
    assert_eq!(report.token_sha256_prefix, sha256_prefix(token));
    assert!(report.created);
    assert!(report.expiry_estimate.is_some());

    // Expiry stamp landed on disk and round-trips.
    let stamp_path = expiry_cache_path(&paths, "proj");
    assert!(stamp_path.exists(), "expiry stamp must be written");
    let read_back = expiry_days_for_launch(&paths, "proj").unwrap();
    assert!(read_back.is_some(), "stamp must parse to a day count");

    // Post-write read used the canonical UUID URI, not the path.
    assert_eq!(
        reader.last_ref.lock().unwrap().last().unwrap(),
        "op://VID/IID/FID"
    );
}

/// The `Global` scope wires the global `[claude]` auth + global env
/// slot (no workspace require, no `op_account`, no expiry stamp), and
/// stamps the op item with the `global` label.
#[test]
fn run_setup_with_runner_global_scope_wires_global_config() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    let mut cfg = AppConfig::default();
    std::fs::write(&paths.config_file, toml::to_string(&cfg).unwrap()).unwrap();

    let writer = FakeOpWriter::new(dummy_op_ref());
    let token = "sk-ant-oat01-GLOBAL";
    let reader = FakeOpReader::ok(token);
    let probe = dummy_probe();

    let report = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Global,
        &TokenSetupArgs {
            vault: Some("Personal".into()),
            ..Default::default()
        },
        Some(&probe),
        || Ok(secrecy::SecretString::from(token.to_owned())),
        &reader,
        &writer,
    )
    .unwrap();

    // Item title used the `global` label, not a workspace name.
    let last = writer.last_create.borrow().clone().unwrap();
    assert_eq!(last.1, DEFAULT_ITEM_TEMPLATE.replace("{ws}", "global"));

    // Global [claude] auth_forward set, no workspace touched.
    let claude = cfg.claude.as_ref().unwrap();
    assert_eq!(claude.auth_forward, AuthForwardMode::OAuthToken);
    assert!(
        cfg.workspaces.is_empty(),
        "global scope must not add a workspace"
    );
    // Token wired into the global env block.
    assert!(matches!(
        cfg.env.get("CLAUDE_CODE_OAUTH_TOKEN"),
        Some(EnvValue::OpRef(_))
    ));

    // Report uses the `global` label; no expiry stamp keyed by workspace.
    assert_eq!(report.workspace, "global");
    assert!(report.created);
    assert!(
        report.expiry_estimate.is_none(),
        "global scope must not stamp a per-workspace expiry"
    );
    assert!(!expiry_cache_path(&paths, "global").exists());
}

#[test]
fn run_setup_with_runner_aborts_when_workspace_missing() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut cfg = AppConfig::default();
    let writer = FakeOpWriter::new(dummy_op_ref());
    let reader = FakeOpReader::ok("ignored");
    let probe = dummy_probe();
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("ghost".into()),
        &TokenSetupArgs::default(),
        Some(&probe),
        || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_owned())),
        &reader,
        &writer,
    )
    .unwrap_err();
    assert!(err.to_string().contains("unknown workspace"));
    assert!(writer.last_create.borrow().is_none());
}

#[test]
fn run_setup_with_runner_aborts_when_vault_missing_and_no_reuse() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    let reader = FakeOpReader::ok("ignored");
    let probe = dummy_probe();
    let capture_called = std::cell::Cell::new(false);
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs::default(),
        Some(&probe),
        || {
            capture_called.set(true);
            Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_owned()))
        },
        &reader,
        &writer,
    )
    .unwrap_err();
    assert!(err.to_string().contains("--vault"));
    assert!(writer.last_create.borrow().is_none());
    assert!(
        !capture_called.get(),
        "OAuth flow must not start when --vault is missing"
    );
}

/// `op_writer.item_create` returning Err must abort the
/// orchestrator BEFORE the workspace config is touched and
/// BEFORE the expiry stamp is written.
#[test]
fn run_setup_with_runner_propagates_item_create_failure_without_touching_config() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::failing();
    let reader = FakeOpReader::ok("sk-ant-oat01-X");
    let probe = dummy_probe();
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            vault: Some("Personal".into()),
            ..Default::default()
        },
        Some(&probe),
        || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_owned())),
        &reader,
        &writer,
    )
    .unwrap_err();
    assert!(err.to_string().contains("simulated item_create failure"));
    // Slot must NOT be wired.
    assert!(
        cfg.workspaces
            .get("proj")
            .and_then(|w| w.claude.as_ref())
            .is_none(),
        "config must stay untouched on item_create failure"
    );
    // Expiry stamp must NOT exist.
    assert!(
        !expiry_cache_path(&paths, "proj").exists(),
        "expiry stamp must not be written when create fails"
    );
}

/// `capture` closure returning Err must abort BEFORE any 1P
/// write or config edit.
#[test]
fn run_setup_with_runner_propagates_capture_failure_without_calling_op() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    let reader = FakeOpReader::ok("ignored");
    let probe = dummy_probe();
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            vault: Some("Personal".into()),
            ..Default::default()
        },
        Some(&probe),
        || Err(anyhow::anyhow!("OAuth flow cancelled by operator")),
        &reader,
        &writer,
    )
    .unwrap_err();
    assert!(err.to_string().contains("OAuth flow cancelled"));
    assert!(writer.last_create.borrow().is_none());
    assert!(
        cfg.workspaces
            .get("proj")
            .and_then(|w| w.claude.as_ref())
            .is_none()
    );
}

/// Post-write `op_reader.read` failure must abort, must NOT
/// persist any config, and must best-effort delete the
/// just-created 1P item to keep the operator's vault tidy.
#[test]
fn run_setup_with_runner_post_write_read_failure_cleans_up_orphan_item() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    let reader = FakeOpReader::err("op read failed: vault not found");
    let probe = dummy_probe();
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            vault: Some("Personal".into()),
            ..Default::default()
        },
        Some(&probe),
        || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_owned())),
        &reader,
        &writer,
    )
    .unwrap_err();
    assert!(err.to_string().contains("post-write validation failed"));
    // Config must NOT be wired.
    assert!(
        cfg.workspaces
            .get("proj")
            .and_then(|w| w.claude.as_ref())
            .is_none()
    );
    // Cleanup deletion must have fired against the canonical UUIDs.
    let deletes = writer.deletes.borrow();
    assert_eq!(deletes.len(), 1, "exactly one cleanup delete expected");
    assert_eq!(deletes[0], ("VID".to_owned(), "IID".to_owned()));
}

/// Post-write SHA mismatch must abort + clean up the orphan +
/// leave config untouched. This is the load-bearing safety net
/// for the "wrote into the wrong vault" scenario.
#[test]
fn run_setup_with_runner_post_write_sha_mismatch_aborts_and_cleans_up() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    // Reader returns a value whose SHA-256 prefix differs from
    // what the orchestrator just captured.
    let reader = FakeOpReader::ok("a-totally-different-token");
    let probe = dummy_probe();
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            vault: Some("Personal".into()),
            ..Default::default()
        },
        Some(&probe),
        || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_owned())),
        &reader,
        &writer,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("SHA-256 prefix"), "got: {msg}");
    assert!(msg.contains("No on-disk config was changed"), "got: {msg}");
    assert!(
        cfg.workspaces
            .get("proj")
            .and_then(|w| w.claude.as_ref())
            .is_none()
    );
    assert_eq!(
        writer.deletes.borrow().len(),
        1,
        "orphan must be best-effort deleted"
    );
    assert!(
        !expiry_cache_path(&paths, "proj").exists(),
        "no expiry stamp should be written on validation failure"
    );
}

/// `--reuse` validation failure must wrap the inner error with a
/// `--reuse`-specific message and not touch any state.
#[test]
fn run_setup_with_runner_reuse_path_surfaces_validation_error() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    let reader = FakeOpReader::err("op read failed: item not found");
    let probe = dummy_probe();
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            reuse: Some(OpRef {
                op: "op://Other/Item/Field".into(),
                path: "Other/Item/Field".into(),
                account: None,
                on_demand: false,
            }),
            ..Default::default()
        },
        Some(&probe),
        || panic!("capture must NOT run on the reuse path"),
        &reader,
        &writer,
    )
    .unwrap_err();
    assert!(err.to_string().contains("--reuse reference"));
    assert!(writer.last_create.borrow().is_none());
    assert!(
        cfg.workspaces
            .get("proj")
            .and_then(|w| w.claude.as_ref())
            .is_none()
    );
}

#[test]
fn run_setup_with_runner_reuse_path_skips_capture_and_no_expiry_stamp() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    let reader = FakeOpReader::ok("sk-ant-oat01-EXISTING");
    let probe = dummy_probe();

    let report = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            reuse: Some(OpRef {
                op: "op://VID/IID/FID".into(),
                path: "Personal/Existing/token".into(),
                account: None,
                on_demand: false,
            }),
            ..Default::default()
        },
        Some(&probe),
        || panic!("capture must NOT be called on the reuse path"),
        &reader,
        &writer,
    )
    .unwrap();

    assert!(!report.created);
    assert!(
        report.expiry_estimate.is_none(),
        "reuse path must not stamp expiry"
    );
    assert!(writer.last_create.borrow().is_none());
    assert!(
        !expiry_cache_path(&paths, "proj").exists(),
        "reuse path must not write the expiry stamp"
    );
    assert_eq!(
        report.op_ref.as_ref().unwrap().path,
        "Personal/Existing/token"
    );
}

/// The plain-text path mints the token, stores it as a literal in
/// the workspace env block, and never touches the op writer. The
/// report carries no op reference.
#[test]
fn run_setup_with_runner_plain_text_wires_literal_and_skips_op_writer() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    let token = "sk-ant-oat01-PLAIN";
    // Reader must never be consulted on the plain path — a panic
    // here proves no post-write validation read fired.
    let reader = FakeOpReader::ok("UNUSED-ON-PLAIN-PATH");
    let probe = dummy_probe();

    let report = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            plain_text: true,
            ..Default::default()
        },
        Some(&probe),
        || Ok(secrecy::SecretString::from(token.to_owned())),
        &reader,
        &writer,
    )
    .unwrap();

    // Op writer was never invoked (no create, no edit, no delete).
    assert!(
        writer.last_create.borrow().is_none(),
        "plain-text path must not call the op writer"
    );
    assert!(writer.deletes.borrow().is_empty());
    // No op read-back validation happened.
    assert!(
        reader.last_ref.lock().unwrap().is_empty(),
        "plain-text path must not read back from op"
    );

    // Token wired as a literal, not an op ref.
    let env_val = cfg
        .workspaces
        .get("proj")
        .and_then(|w| w.env.get("CLAUDE_CODE_OAUTH_TOKEN"));
    assert_eq!(env_val, Some(&EnvValue::Plain(token.to_owned())));

    // auth_forward still flips to oauth_token.
    let claude = cfg
        .workspaces
        .get("proj")
        .and_then(|w| w.claude.as_ref())
        .unwrap();
    assert_eq!(claude.auth_forward, AuthForwardMode::OAuthToken);

    // Report carries no op ref; created + expiry stamp still set
    // because jackin minted the token.
    assert!(report.op_ref.is_none(), "plain path has no op reference");
    assert!(report.created);
    assert_eq!(report.token_sha256_prefix, sha256_prefix(token));
    assert!(report.expiry_estimate.is_some());
}

/// `plain_text` combined with `reuse` is rejected before any
/// capture: the two pick different storage targets.
#[test]
fn run_setup_with_runner_plain_text_with_reuse_bails() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    let reader = FakeOpReader::ok("ignored");
    let probe = dummy_probe();
    let capture_called = std::cell::Cell::new(false);
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            plain_text: true,
            reuse: Some(OpRef {
                op: "op://Other/Item/Field".into(),
                path: "Other/Item/Field".into(),
                account: None,
                on_demand: false,
            }),
            ..Default::default()
        },
        Some(&probe),
        || {
            capture_called.set(true);
            Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_owned()))
        },
        &reader,
        &writer,
    )
    .unwrap_err();
    assert!(err.to_string().contains("--plain is mutually exclusive"));
    assert!(
        !capture_called.get(),
        "no mint must start when --plain conflicts with --reuse"
    );
    assert!(writer.last_create.borrow().is_none());
}

/// `expiry_days_for_launch` distinguishes "absent" (Ok(None))
/// from "present-but-malformed" (Err) so the launch banner can
/// warn instead of silently omitting the countdown.
#[test]
fn expiry_days_for_launch_distinguishes_absent_from_malformed() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    // Absent → Ok(None) (silent).
    assert!(matches!(expiry_days_for_launch(&paths, "ws"), Ok(None)));

    // Present + valid → Ok(Some(_)).
    let future = (chrono::Utc::now().date_naive() + chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();
    write_expiry_stamp(&paths, "ws", &future).unwrap();
    let days = expiry_days_for_launch(&paths, "ws").unwrap().unwrap();
    assert!((6..=7).contains(&days), "days = {days}");

    // Present + malformed → Err.
    std::fs::write(expiry_cache_path(&paths, "ws"), "not-a-date\n").unwrap();
    let err = expiry_days_for_launch(&paths, "ws").unwrap_err();
    assert!(err.to_string().contains("malformed"));

    // Present + empty → Err.
    std::fs::write(expiry_cache_path(&paths, "ws"), "  \n").unwrap();
    let err = expiry_days_for_launch(&paths, "ws").unwrap_err();
    assert!(err.to_string().contains("empty"));
}

/// `clear_expiry_stamp` is idempotent on missing files and does
/// not propagate the `NotFound` error.
#[test]
fn clear_expiry_stamp_is_idempotent_on_missing_file() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    // Two consecutive clears on a never-written path must not panic.
    clear_expiry_stamp(&paths, "ws");
    clear_expiry_stamp(&paths, "ws");
}

/// `run_revoke` clears the canonical slot, switches mode to
/// `ignore`, and clears the cached expiry stamp.
#[test]
fn run_revoke_clears_slot_mode_and_expiry_cache() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    // Pre-stamp + pre-wired env var.
    write_expiry_stamp(&paths, "proj", "2027-01-01").unwrap();
    let mut ws = cfg.workspaces.get("proj").unwrap().clone();
    ws.claude = Some(jackin_config::AgentAuthConfig {
        auth_forward: AuthForwardMode::OAuthToken,
        ..Default::default()
    });
    ws.env.insert(
        "CLAUDE_CODE_OAUTH_TOKEN".into(),
        EnvValue::OpRef(OpRef {
            op: "op://VID/IID/FID".into(),
            path: "Personal/Item/token".into(),
            account: None,
            on_demand: false,
        }),
    );
    cfg.workspaces.insert("proj".into(), ws);
    std::fs::write(&paths.config_file, toml::to_string(&cfg).unwrap()).unwrap();

    let report = run_revoke(&paths, &mut cfg, "proj", false).unwrap();
    assert!(report.cleared_slot);
    assert!(!report.deleted_op_item);

    // Env var cleared.
    assert!(
        cfg.workspaces
            .get("proj")
            .and_then(|w| w.env.get("CLAUDE_CODE_OAUTH_TOKEN"))
            .is_none()
    );
    // auth_forward flipped to Ignore.
    let claude = cfg
        .workspaces
        .get("proj")
        .and_then(|w| w.claude.as_ref())
        .unwrap();
    assert_eq!(claude.auth_forward, AuthForwardMode::Ignore);
    // Expiry cache gone.
    assert!(!expiry_cache_path(&paths, "proj").exists());
}

/// `vault_for_rotate` derives the prior item's vault id when
/// `--vault` is not supplied, so the documented default rotate
/// flow (`jackin workspace claude-token rotate <ws>`) does not
/// hard-error inside `create_op_item` after PTY token capture
/// completes.
#[test]
fn vault_for_rotate_falls_back_to_prior_op_ref_vault() {
    let prior = EnvValue::OpRef(OpRef {
        op: "op://VAULT_UUID/ITEM_UUID/FIELD_UUID".into(),
        path: "Personal/jackin · proj · claude-token/token".into(),
        account: None,
        on_demand: false,
    });
    assert_eq!(
        vault_for_rotate(None, Some(&prior)),
        Some("VAULT_UUID".to_owned()),
        "no --vault and prior op-ref ⇒ vault id parsed from prior URI"
    );
}

/// Explicit `--vault` overrides the prior item's vault — operators
/// can move the rotated item to a different vault without first
/// running `revoke`.
#[test]
fn vault_for_rotate_prefers_explicit_cli_vault() {
    let prior = EnvValue::OpRef(OpRef {
        op: "op://OldVault/ITEM/FIELD".into(),
        path: "OldVault/Item/token".into(),
        account: None,
        on_demand: false,
    });
    assert_eq!(
        vault_for_rotate(Some("NewVault".into()), Some(&prior)),
        Some("NewVault".to_owned()),
    );
}

/// Literal prior slot has no embedded vault context, so rotate
/// without `--vault` must fall through to the existing
/// "no --vault supplied" error path inside `create_op_item`.
#[test]
fn vault_for_rotate_returns_none_for_literal_prior() {
    let prior = EnvValue::Plain("sk-ant-oat01-LITERAL".into());
    assert_eq!(vault_for_rotate(None, Some(&prior)), None);
}

/// No prior slot AND no `--vault` ⇒ `None`. `create_op_item`
/// will surface the "no --vault supplied" error to the operator.
#[test]
fn vault_for_rotate_returns_none_when_neither_set() {
    assert_eq!(vault_for_rotate(None, None), None);
}

/// `doctor` on a workspace with a literal `oauth_token` slot
/// must hash the actual stored token, not a placeholder string.
/// Regression test for the bug where the `Plain(_)` branch
/// returned `"(literal slot — resolves verbatim)"` and that
/// placeholder was the value fed into `sha256_prefix`,
/// producing a SHA-256 prefix that never matched the
/// configured credential.
#[test]
fn run_doctor_hashes_literal_token_not_placeholder() {
    let mut cfg = AppConfig::default();
    let mut ws = workspace("proj");
    ws.claude = Some(jackin_config::AgentAuthConfig {
        auth_forward: AuthForwardMode::OAuthToken,
        ..Default::default()
    });
    ws.env.insert(
        "CLAUDE_CODE_OAUTH_TOKEN".into(),
        EnvValue::Plain("sk-ant-oat01-LITERAL-FOR-DOCTOR-TEST".into()),
    );
    cfg.workspaces.insert("proj".into(), ws);

    let report = run_doctor(&cfg, "proj").unwrap();

    let placeholder = "(literal slot — resolves verbatim)";
    assert_ne!(
        report.token_sha256_prefix,
        sha256_prefix(placeholder),
        "doctor must not hash the placeholder string"
    );
    assert_eq!(
        report.token_sha256_prefix,
        sha256_prefix("sk-ant-oat01-LITERAL-FOR-DOCTOR-TEST"),
        "doctor must hash the literal stored token"
    );
    assert!(
        report.op_ref.is_none(),
        "literal slot has no op:// reference"
    );
    assert_eq!(report.mode, AuthForwardMode::OAuthToken);
}

/// `run_doctor` on a workspace whose `[env]` block is missing
/// `CLAUDE_CODE_OAUTH_TOKEN` returns the actionable "run setup
/// first" error rather than a generic miss.
#[test]
fn run_doctor_missing_env_var_returns_actionable_error() {
    let mut cfg = AppConfig::default();
    let ws = workspace("proj");
    cfg.workspaces.insert("proj".into(), ws);

    let reader = FakeOpReader::ok("unused");
    let err = run_doctor_with_runner(&cfg, "proj", &reader).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("CLAUDE_CODE_OAUTH_TOKEN") && msg.contains("claude-token setup"),
        "doctor must point the operator at `claude-token setup`, got: {msg}"
    );
}

/// `run_doctor` wraps the `op read` failure in an operator-facing
/// error that names the resolved path.
#[test]
fn run_doctor_op_read_failure_wraps_error() {
    let mut cfg = AppConfig::default();
    let mut ws = workspace("proj");
    ws.env.insert(
        "CLAUDE_CODE_OAUTH_TOKEN".into(),
        EnvValue::OpRef(OpRef {
            op: "op://VID/IID/FID".into(),
            path: "Personal/Item/token".into(),
            account: None,
            on_demand: false,
        }),
    );
    cfg.workspaces.insert("proj".into(), ws);

    let reader = FakeOpReader::err("vault locked");
    let err = run_doctor_with_runner(&cfg, "proj", &reader).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Personal/Item/token"), "got: {msg}");
    assert!(msg.contains("vault locked"), "got: {msg}");
}

/// `run_revoke --delete-op-item` on a slot with a parseable op://
/// reference issues an `item_delete` with the parsed UUIDs and
/// then clears the canonical slot.
#[test]
fn run_revoke_with_runner_delete_op_item_calls_writer_with_parsed_uuids() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let mut ws = cfg.workspaces.get("proj").unwrap().clone();
    ws.claude = Some(jackin_config::AgentAuthConfig {
        auth_forward: AuthForwardMode::OAuthToken,
        ..Default::default()
    });
    ws.env.insert(
        "CLAUDE_CODE_OAUTH_TOKEN".into(),
        EnvValue::OpRef(OpRef {
            op: "op://VAULT_UUID/ITEM_UUID/FIELD_UUID".into(),
            path: "Personal/Item/token".into(),
            account: None,
            on_demand: false,
        }),
    );
    cfg.workspaces.insert("proj".into(), ws);
    std::fs::write(&paths.config_file, toml::to_string(&cfg).unwrap()).unwrap();

    let writer = FakeOpWriter::new(dummy_op_ref());
    let report = run_revoke_with_runner(&paths, &mut cfg, "proj", true, &writer).unwrap();

    assert!(report.cleared_slot);
    assert!(report.deleted_op_item);
    assert_eq!(
        *writer.deletes.borrow(),
        vec![("VAULT_UUID".to_owned(), "ITEM_UUID".to_owned())],
        "delete must be issued against the parsed vault/item UUIDs"
    );
    assert!(
        cfg.workspaces
            .get("proj")
            .and_then(|w| w.env.get("CLAUDE_CODE_OAUTH_TOKEN"))
            .is_none(),
        "slot must be cleared after a successful delete"
    );
}

/// `run_revoke --delete-op-item` on a literal-token slot is an
/// explicit error: jackin does not know where the literal came
/// from, and a silent fall-through would let the secret survive
/// in 1Password without the operator knowing.
#[test]
fn run_revoke_with_runner_delete_op_item_on_literal_slot_bails() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let mut ws = cfg.workspaces.get("proj").unwrap().clone();
    ws.claude = Some(jackin_config::AgentAuthConfig {
        auth_forward: AuthForwardMode::OAuthToken,
        ..Default::default()
    });
    ws.env.insert(
        "CLAUDE_CODE_OAUTH_TOKEN".into(),
        EnvValue::Plain("sk-ant-oat01-LITERAL".into()),
    );
    cfg.workspaces.insert("proj".into(), ws);
    std::fs::write(&paths.config_file, toml::to_string(&cfg).unwrap()).unwrap();

    let writer = FakeOpWriter::new(dummy_op_ref());
    let err = run_revoke_with_runner(&paths, &mut cfg, "proj", true, &writer).unwrap_err();

    assert!(err.to_string().contains("literal token slot"));
    assert!(
        writer.deletes.borrow().is_empty(),
        "no delete must fire on literal slots"
    );
    // Config must NOT have been mutated — caller can re-run
    // without --delete-op-item to clear the slot.
    assert!(
        cfg.workspaces
            .get("proj")
            .and_then(|w| w.env.get("CLAUDE_CODE_OAUTH_TOKEN"))
            .is_some()
    );
}

/// `run_revoke --delete-op-item` where `op item delete` fails
/// propagates the error AND leaves the workspace config
/// untouched, so a re-run of `revoke` can complete the cleanup
/// without first having to re-stamp the slot.
#[test]
fn run_revoke_with_runner_delete_op_item_failure_does_not_save_config() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let mut ws = cfg.workspaces.get("proj").unwrap().clone();
    ws.claude = Some(jackin_config::AgentAuthConfig {
        auth_forward: AuthForwardMode::OAuthToken,
        ..Default::default()
    });
    ws.env.insert(
        "CLAUDE_CODE_OAUTH_TOKEN".into(),
        EnvValue::OpRef(OpRef {
            op: "op://VID/IID/FID".into(),
            path: "Personal/Item/token".into(),
            account: None,
            on_demand: false,
        }),
    );
    cfg.workspaces.insert("proj".into(), ws);
    std::fs::write(&paths.config_file, toml::to_string(&cfg).unwrap()).unwrap();

    let writer = FakeOpWriter::new(dummy_op_ref()).with_failing_delete();
    let err = run_revoke_with_runner(&paths, &mut cfg, "proj", true, &writer).unwrap_err();

    assert!(err.to_string().contains("simulated item_delete failure"));
    // Slot must still be present on disk and in cfg — the
    // `editor.save` step is reached only after `item_delete`
    // succeeds.
    let on_disk: AppConfig =
        toml::from_str(&std::fs::read_to_string(&paths.config_file).unwrap()).unwrap();
    assert!(
        on_disk
            .workspaces
            .get("proj")
            .and_then(|w| w.env.get("CLAUDE_CODE_OAUTH_TOKEN"))
            .is_some(),
        "delete-failure must not save the cleared slot to disk"
    );
}

/// Post-write read-failure where the cleanup-delete also fails:
/// bail message must surface the manual `op item delete <id>
/// --vault <vault>` recovery hint so the operator can clean up
/// the orphan that jackin couldn't.
#[test]
fn run_setup_with_runner_post_write_failure_with_failing_delete_surfaces_manual_hint() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref()).with_failing_delete();
    let reader = FakeOpReader::err("op read failed: vault not found");
    let probe = dummy_probe();
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            vault: Some("Personal".into()),
            ..Default::default()
        },
        Some(&probe),
        || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_owned())),
        &reader,
        &writer,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("was NOT deleted"), "got: {msg}");
    assert!(
        msg.contains("op item delete IID --vault VID"),
        "manual recovery hint missing: {msg}"
    );
    // The failed delete still recorded its attempt.
    assert_eq!(writer.deletes.borrow().len(), 1);
}

/// Post-write failure where the produced op-ref does not parse:
/// orphan-cleanup skips the delete call entirely and the bail
/// message tells the operator to remove the item by hand.
#[test]
fn run_setup_with_runner_post_write_unparseable_op_ref_skips_delete_call() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let bogus_ref = OpRef {
        op: "garbage-not-an-op-uri".into(),
        path: "Personal/Item/token".into(),
        account: None,
        on_demand: false,
    };
    let writer = FakeOpWriter::new(bogus_ref);
    let reader = FakeOpReader::err("op read failed: bogus URI");
    let probe = dummy_probe();
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            vault: Some("Personal".into()),
            ..Default::default()
        },
        Some(&probe),
        || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_owned())),
        &reader,
        &writer,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("did not parse into vault/item ids"),
        "got: {msg}"
    );
    assert!(
        writer.deletes.borrow().is_empty(),
        "no delete must fire on unparseable op-ref"
    );
}

/// Edit-existing path with a post-write validation failure: the
/// field was written into the operator's PRE-EXISTING item, so a
/// mismatch must NOT delete that item (deleting it would destroy the
/// operator's other fields). The bail message must say the item was
/// kept intact.
#[test]
fn run_setup_with_runner_edit_existing_validation_failure_keeps_item() {
    let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
    let writer = FakeOpWriter::new(dummy_op_ref());
    // Reader resolves the ref to a DIFFERENT value than the captured
    // token, so the SHA-256 prefix check fails and the post-write
    // validation bails.
    let reader = FakeOpReader::ok("sk-ant-oat01-DIFFERENT");
    let probe = dummy_probe();
    let err = run_setup_with_runner(
        &paths,
        &mut cfg,
        &TokenSetupScope::Workspace("proj".into()),
        &TokenSetupArgs {
            edit_existing: Some(EditExistingTarget {
                vault_id: "VID".into(),
                item_id: "IID".into(),
                field: FieldTarget::Existing {
                    id: "fld-real-id".into(),
                    label: "token".into(),
                },
                section: None,
            }),
            ..Default::default()
        },
        Some(&probe),
        || {
            Ok(secrecy::SecretString::from(
                "sk-ant-oat01-CAPTURED".to_owned(),
            ))
        },
        &reader,
        &writer,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("post-write validation failed"), "got: {msg}");
    assert!(
        msg.contains("left intact"),
        "bail message must say the existing item was kept: {msg}"
    );
    assert!(
        writer.deletes.borrow().is_empty(),
        "edit-existing validation failure must NEVER delete the operator's item"
    );
    // The picked field's real id must reach the writer so the overwrite
    // targets that exact field rather than the first label match.
    assert_eq!(
        *writer.recorded_field_id.borrow(),
        Some(Some("fld-real-id".to_owned())),
        "edit-existing must thread the field id to item_field_set"
    );
}

/// `OrphanCleanup::Display` wording is the operator-facing
/// contract — pin it here so a refactor cannot silently drop the
/// recovery instructions.
#[test]
fn orphan_cleanup_display_each_variant() {
    assert_eq!(
        OrphanCleanup::Deleted.to_string(),
        "The just-created 1P item was deleted."
    );
    assert!(
        OrphanCleanup::UnparseableRef {
            op: "garbage".into(),
        }
        .to_string()
        .contains("did not parse into vault/item ids")
    );
    let failed = OrphanCleanup::DeleteFailed {
        err: "vault locked".into(),
        hint: "op item delete X --vault Y".into(),
    };
    let s = failed.to_string();
    assert!(s.contains("vault locked"));
    assert!(s.contains("op item delete X --vault Y"));
}
