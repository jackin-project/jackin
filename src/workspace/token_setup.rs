//! Workspace Claude token setup orchestrator.
//!
//! Imperative pipeline glueing four primitives together:
//!
//! 1. [`crate::host_claude::probe_claude_cli`] — verify the upstream
//!    `claude` CLI is on `PATH` and capture its version.
//! 2. [`crate::host_claude::capture_setup_token`] — drive
//!    `claude setup-token` under a PTY; redacted progress goes to
//!    `stderr`, the captured token lives in `secrecy::SecretString`.
//! 3. [`crate::operator_env::OpWriteRunner::item_create`] — write a
//!    new 1Password item with the token on stdin (never argv).
//! 4. Validate the round-trip via `OpRunner::read` + SHA-256 prefix
//!    comparison BEFORE persisting any on-disk config — a vault-
//!    routing surprise must never leave a wired-but-broken slot
//!    behind.
//! 5. [`crate::config::ConfigEditor`] — comment-preserving edit of
//!    the workspace's `[claude]` block (`auth_forward = "oauth_token"`)
//!    and `[env]` block (`CLAUDE_CODE_OAUTH_TOKEN = op://...`).
//!
//! Production callers use [`run_setup`]; tests inject mocks via
//! [`run_setup_with_runner`].
//!
//! Roadmap: `docs/src/content/docs/reference/roadmap/workspace-claude-token-setup.mdx`

use crate::config::{AppConfig, AuthForwardMode, ConfigEditor, EnvScope};
use crate::host_claude;
use crate::operator_env::{EnvValue, OpItemCreateParams, OpRef, OpRunner, OpWriteRunner};
use crate::paths::JackinPaths;

use secrecy::ExposeSecret;
use sha2::Digest;

/// Default `op` item title template — `{ws}` substitutes the
/// workspace name. Operators can override with
/// [`TokenSetupArgs::item_name`].
pub const DEFAULT_ITEM_TEMPLATE: &str = "jackin · {ws} · claude-token";

/// Default `op` item category for OAuth tokens (1Password's API
/// Credential category renders `token` as a concealed field).
pub const DEFAULT_ITEM_CATEGORY: &str = "API_CREDENTIAL";

/// Default field label inside the created item.
pub const DEFAULT_FIELD_LABEL: &str = "token";

/// Tags every jackin-managed item is stamped with so list / search
/// filters can find them later.
pub const JACKIN_TAG: &str = "jackin";
/// Per-workspace tag prefix (`workspace=<name>`).
pub const WORKSPACE_TAG_PREFIX: &str = "workspace=";

/// Approximate validity window of an upstream-issued OAuth token.
/// 1Password stores the absolute date; the orchestrator computes it
/// at write time so the operator can see expiry-relative timing in
/// the launch banner.
const TOKEN_LIFETIME_DAYS: i64 = 365;

/// Operator-supplied arguments to a `jackin workspace setup
/// claude-token` invocation. Optional fields fall back to workspace
/// or repo-level defaults.
#[derive(Debug, Clone, Default)]
pub struct TokenSetupArgs {
    /// 1Password vault name or UUID where the new item is created.
    /// Required on every `Capture` invocation. Auto-detection of
    /// the prior item's vault on rotate is a deferred follow-up
    /// (the orchestrator currently asks the operator to repeat
    /// `--vault`).
    pub vault: Option<String>,
    /// Override the auto-generated item title. `{ws}` is substituted.
    pub item_name: Option<String>,
    /// Pin this run to a specific 1P account; falls back to
    /// `WorkspaceConfig.op_account`, then `op`'s default.
    pub account: Option<String>,
    /// If set, skip token generation and adopt the supplied
    /// `op://...` reference verbatim. Used for setup-from-existing
    /// flows.
    pub reuse: Option<OpRef>,
}

/// Outcome of one orchestrator run.
#[derive(Debug, Clone)]
pub struct TokenSetupReport {
    pub workspace: String,
    pub claude_cli_version: String,
    pub op_ref: OpRef,
    pub op_account: Option<String>,
    pub token_sha256_prefix: String,
    pub created: bool,
    /// `YYYY-MM-DD` estimate of when the captured token will lapse.
    /// `None` for the `--reuse` path (jackin did not mint the token,
    /// so it cannot estimate the issuance date).
    pub expiry_estimate: Option<String>,
}

/// Run the orchestrator end-to-end against production runners.
///
/// Equivalent to calling [`run_setup_with_runner`] with
/// `host_claude::capture_setup_token` and a freshly constructed
/// `OpCli`. Tests inject mocks via the `_with_runner` form.
pub fn run_setup(
    paths: &JackinPaths,
    config: &mut AppConfig,
    workspace: &str,
    args: &TokenSetupArgs,
) -> anyhow::Result<TokenSetupReport> {
    let op_account =
        effective_account(config, workspace, args.account.as_deref()).map(str::to_string);
    let op_cli = crate::operator_env::OpCli::new().with_account(op_account);
    let probe = host_claude::probe_claude_cli()?;
    run_setup_with_runner(
        paths,
        config,
        workspace,
        args,
        &probe,
        host_claude::capture_setup_token,
        &op_cli,
        &op_cli,
    )
}

/// Test-injectable variant. Takes:
/// - a pre-resolved Claude CLI probe (skips re-running `claude --version`),
/// - a closure that returns the captured token (`capture_setup_token`
///   in production; a fixture in tests),
/// - an [`OpRunner`] for read-back validation, and
/// - an [`OpWriteRunner`] for the actual create.
///
/// The orchestrator's mutation path is gated behind these injection
/// seams so the unit tests in this module never spawn `op` or
/// `claude`.
#[allow(clippy::too_many_arguments)]
pub fn run_setup_with_runner<F>(
    paths: &JackinPaths,
    config: &mut AppConfig,
    workspace: &str,
    args: &TokenSetupArgs,
    probe: &host_claude::ClaudeProbe,
    capture: F,
    op_reader: &dyn OpRunner,
    op_writer: &dyn OpWriteRunner,
) -> anyhow::Result<TokenSetupReport>
where
    F: FnOnce() -> anyhow::Result<secrecy::SecretString>,
{
    if !config.workspaces.contains_key(workspace) {
        anyhow::bail!(
            "workspace {workspace:?} is not registered; \
             create it first with `jackin workspace create`"
        );
    }

    // Step 1: produce a token. Either re-use the operator's existing
    // op:// (skip generation) or capture a fresh one.
    let (op_ref, token_sha256_prefix, created) = if let Some(reuse_ref) = args.reuse.as_ref() {
        let value = op_reader.read(&reuse_ref.op).map_err(|e| {
            anyhow::anyhow!(
                "validation of --reuse reference {:?} failed: {e} \
                 (vault / item / field correct?)",
                reuse_ref.path
            )
        })?;
        let prefix = sha256_prefix(&value);
        (reuse_ref.clone(), prefix, false)
    } else {
        let secret = capture()?;
        let prefix = sha256_prefix(secret.expose_secret());
        let op_ref = create_op_item(op_writer, config, workspace, args, &secret, &prefix, probe)?;
        (op_ref, prefix, true)
    };

    // Step 2: validate the write BEFORE persisting any on-disk
    // config. A wired slot pointing at a 1P item whose value the
    // operator never saw would silently inject a mystery token at
    // the next launch — both arms below abort and best-effort
    // delete the orphan so the operator's vault stays tidy.
    let cleanup_orphan = || {
        if created && let Some((vault_id, item_id)) = parse_uuid_op_ref(&op_ref.op) {
            let _ = op_writer.item_delete(item_id, vault_id, args.account.as_deref());
        }
    };
    let resolved = op_reader.read(&op_ref.op).map_err(|e| {
        cleanup_orphan();
        anyhow::anyhow!(
            "post-write validation failed: re-reading {:?} returned: {e} \
             (no on-disk config was changed; the just-created 1P item was \
             best-effort deleted)",
            op_ref.path
        )
    })?;
    if sha256_prefix(&resolved) != token_sha256_prefix {
        cleanup_orphan();
        anyhow::bail!(
            "post-write validation failed: {:?} resolved to a value whose SHA-256 prefix \
             does not match the value just written. No on-disk config was changed; the \
             just-created 1P item was best-effort deleted. Re-run setup, or inspect the \
             1P item by hand if the deletion did not succeed.",
            op_ref.path
        );
    }

    // Step 3: persist the workspace config last so a partial failure
    // earlier in this function never leaves a wired-but-broken slot.
    let mut editor = ConfigEditor::open(paths)?;
    editor.set_workspace_auth_forward(
        workspace,
        crate::agent::Agent::Claude,
        Some(AuthForwardMode::OAuthToken),
    );
    editor.set_env_var(
        &EnvScope::Workspace(workspace.to_string()),
        "CLAUDE_CODE_OAUTH_TOKEN",
        EnvValue::OpRef(op_ref.clone()),
    )?;
    if let Some(account) = args.account.as_deref()
        && config
            .workspaces
            .get(workspace)
            .and_then(|ws| ws.op_account.as_deref())
            != Some(account)
    {
        editor.set_workspace_op_account(workspace, Some(account));
    }
    let saved = editor.save()?;
    *config = saved;

    let op_account =
        effective_account(config, workspace, args.account.as_deref()).map(str::to_string);

    // Stamp the expiry estimate into the local cache so the launch
    // diagnostic can render an `expires in N days` banner without
    // re-reading the 1P item's `notesPlain`. Only do this on the
    // `created` path — for `--reuse` we did not mint the token, so
    // the issuance date is unknown and any stamp would mislead. If
    // the cache write fails (filesystem full / permission), the
    // operator is told and the report's `expiry_estimate` is set to
    // `None` so it matches what the launch banner will see.
    let expiry_estimate = if created {
        let expiry = upstream_expiry_stamp();
        match write_expiry_stamp(paths, workspace, &expiry) {
            Ok(()) => Some(expiry),
            Err(e) => {
                eprintln!(
                    "[jackin] note: token cached in 1Password, but expiry banner cache \
                     write failed: {e} — launch banner will not show 'expires in N days' \
                     for this workspace until the next setup."
                );
                None
            }
        }
    } else {
        None
    };

    Ok(TokenSetupReport {
        workspace: workspace.to_string(),
        claude_cli_version: probe.version.clone(),
        op_ref,
        op_account,
        token_sha256_prefix,
        created,
        expiry_estimate,
    })
}

/// Revoke the workspace's token: clear the canonical slot, switch
/// `auth_forward` to `ignore`, and (optionally) delete the 1P item.
///
/// `delete_op_item` requires that the workspace's existing
/// `oauth_token` slot resolves to an `op://` reference and the item
/// id can be parsed from it. Plain literal slots are cleared without
/// any 1P-side action (jackin does not know where the literal came
/// from).
pub fn run_revoke(
    paths: &JackinPaths,
    config: &mut AppConfig,
    workspace: &str,
    delete_op_item: bool,
) -> anyhow::Result<RevokeReport> {
    let ws = config
        .workspaces
        .get(workspace)
        .ok_or_else(|| anyhow::anyhow!("workspace {workspace:?} is not registered"))?;
    let prior = ws.env.get("CLAUDE_CODE_OAUTH_TOKEN").cloned();

    let deleted_item = if delete_op_item
        && let Some(EnvValue::OpRef(r)) = &prior
        && let Some((vault_id, item_id)) = parse_uuid_op_ref(&r.op)
    {
        let account = effective_account(config, workspace, None).map(str::to_string);
        let op_cli = crate::operator_env::OpCli::new().with_account(account);
        op_cli.item_delete(item_id, vault_id, None)?;
        true
    } else {
        false
    };

    let mut editor = ConfigEditor::open(paths)?;
    editor.remove_env_var(
        &EnvScope::Workspace(workspace.to_string()),
        "CLAUDE_CODE_OAUTH_TOKEN",
    );
    editor.set_workspace_auth_forward(
        workspace,
        crate::agent::Agent::Claude,
        Some(AuthForwardMode::Ignore),
    );
    let saved = editor.save()?;
    *config = saved;

    // Drop the cached expiry stamp — the slot is gone, the banner
    // should not surface a stale countdown for the next launch.
    clear_expiry_stamp(paths, workspace);

    Ok(RevokeReport {
        workspace: workspace.to_string(),
        deleted_op_item: deleted_item,
        cleared_slot: prior.is_some(),
    })
}

#[derive(Debug, Clone)]
pub struct RevokeReport {
    pub workspace: String,
    pub deleted_op_item: bool,
    pub cleared_slot: bool,
}

/// Pick the vault for a `rotate` invocation: explicit `--vault` if
/// the operator supplied one, otherwise the vault id parsed out of
/// the prior canonical slot.
///
/// Without this, `jackin workspace claude-token rotate <ws>` (no
/// `--vault`) — the documented default rotate flow — fails inside
/// [`create_op_item`] AFTER the operator has already completed the
/// PTY token capture, because `create_op_item` hard-errors when its
/// vault arg is `None`. The prior canonical slot stores a
/// UUID-form `op://<vault_id>/<item_id>/<field_id>` URI, so the
/// vault id round-trips through `create_op_item`'s vault arg
/// without needing a separate name lookup.
///
/// Returns `None` only when the CLI passed nothing AND the prior
/// slot is absent or holds a literal (non-`op://`) value — both
/// cases that legitimately require explicit `--vault`. The caller
/// surfaces the resulting "no --vault supplied" error from
/// `create_op_item` in that case.
#[must_use]
pub fn vault_for_rotate(cli_vault: Option<String>, prior: Option<&EnvValue>) -> Option<String> {
    cli_vault.or_else(|| {
        prior.and_then(|v| match v {
            EnvValue::OpRef(r) => crate::operator_env::parse_op_reference(&r.op).map(|p| p.vault),
            EnvValue::Plain(_) => None,
        })
    })
}

/// Read the workspace's current `oauth_token` slot, resolve it via
/// `op`, and report whether the value resolves cleanly.
///
/// This is a structural / connectivity check only — it does not
/// contact Claude's API. The cheapest reliable way to confirm an
/// OAuth token is *valid* upstream is to launch a workspace and
/// observe the auth banner; doctor's job is to confirm the
/// canonical-slot config plumbing resolves without errors.
pub fn run_doctor(config: &AppConfig, workspace: &str) -> anyhow::Result<DoctorReport> {
    let ws = config
        .workspaces
        .get(workspace)
        .ok_or_else(|| anyhow::anyhow!("workspace {workspace:?} is not registered"))?;
    let mode = ws
        .claude
        .as_ref()
        .map(|c| c.auth_forward)
        .unwrap_or_default();
    let token_decl = ws.env.get("CLAUDE_CODE_OAUTH_TOKEN").ok_or_else(|| {
        anyhow::anyhow!(
            "workspace {workspace:?} has no CLAUDE_CODE_OAUTH_TOKEN in its env block — \
             run `jackin workspace claude-token setup` first"
        )
    })?;

    let account = effective_account(config, workspace, None).map(str::to_string);
    let op_cli = crate::operator_env::OpCli::new().with_account(account.clone());

    let resolution = match token_decl {
        EnvValue::Plain(t) => Ok(t.clone()),
        EnvValue::OpRef(r) => op_cli
            .read(&r.op)
            .map_err(|e| anyhow::anyhow!("op read for {:?} failed: {e}", r.path)),
    };
    let token = resolution?;
    let prefix = sha256_prefix(&token);

    Ok(DoctorReport {
        workspace: workspace.to_string(),
        mode,
        op_ref: match token_decl {
            EnvValue::OpRef(r) => Some(r.clone()),
            EnvValue::Plain(_) => None,
        },
        op_account: account,
        token_sha256_prefix: prefix,
    })
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub workspace: String,
    pub mode: AuthForwardMode,
    pub op_ref: Option<OpRef>,
    pub op_account: Option<String>,
    pub token_sha256_prefix: String,
}

fn create_op_item(
    op_writer: &dyn OpWriteRunner,
    config: &AppConfig,
    workspace: &str,
    args: &TokenSetupArgs,
    secret: &secrecy::SecretString,
    token_sha256_prefix: &str,
    probe: &host_claude::ClaudeProbe,
) -> anyhow::Result<OpRef> {
    let vault = args.vault.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "no --vault supplied; `jackin workspace claude-token setup` and \
             `jackin workspace claude-token rotate` need --vault <name-or-uuid> \
             so the new item lands somewhere explicit. Pass --reuse if you \
             already have an op:// reference to adopt instead."
        )
    })?;

    let title_template = args.item_name.as_deref().unwrap_or(DEFAULT_ITEM_TEMPLATE);
    let title = title_template.replace("{ws}", workspace);

    let workspace_tag = format!("{WORKSPACE_TAG_PREFIX}{workspace}");
    let expires = upstream_expiry_stamp();
    let notes = format!(
        "Managed by jackin\n\
         workspace = {workspace}\n\
         host_claude = {claude}\n\
         created = {now}\n\
         expires_estimate = {expires}\n\
         token_sha256_prefix = {prefix}\n\
         (Edit at your own risk — re-run \
         `jackin workspace claude-token setup` to rotate.)",
        claude = probe.version,
        now = now_utc_rfc3339(),
        prefix = token_sha256_prefix,
    );
    let tags = [JACKIN_TAG, workspace_tag.as_str()];

    let _ = config; // reserved for future cross-workspace conflict checks
    let account = args.account.as_deref();
    let params = OpItemCreateParams {
        vault_id: vault,
        account,
        title: &title,
        category: DEFAULT_ITEM_CATEGORY,
        field_label: DEFAULT_FIELD_LABEL,
        value: secret.expose_secret(),
        notes_plain: Some(&notes),
        tags: &tags,
    };
    op_writer.item_create(params)
}

/// Parse `op://VAULT/ITEM/FIELD` (or 4-segment) into `(vault, item)`
/// — the IDs needed to delete the item.
pub fn parse_uuid_op_ref(uri: &str) -> Option<(&str, &str)> {
    let body = uri.strip_prefix("op://")?;
    let mut segs = body.split('/');
    let vault = segs.next()?;
    let item = segs.next()?;
    if vault.is_empty() || item.is_empty() {
        return None;
    }
    Some((vault, item))
}

fn effective_account<'a>(
    config: &'a AppConfig,
    workspace: &str,
    explicit: Option<&'a str>,
) -> Option<&'a str> {
    explicit.or_else(|| {
        config
            .workspaces
            .get(workspace)
            .and_then(|ws| ws.op_account.as_deref())
    })
}

fn sha256_prefix(value: &str) -> String {
    let digest = sha2::Sha256::digest(value.as_bytes());
    digest
        .iter()
        .take(6)
        .fold(String::with_capacity(12), |mut acc, byte| {
            use std::fmt::Write;
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

fn upstream_expiry_stamp() -> String {
    let now = chrono::Utc::now();
    let expiry = now + chrono::Duration::days(TOKEN_LIFETIME_DAYS);
    expiry.format("%Y-%m-%d").to_string()
}

fn now_utc_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Local cache file path holding the workspace's token expiry stamp
/// (YYYY-MM-DD). Written by the orchestrator on successful setup /
/// rotate, read by the launch diagnostic for the expiry banner.
///
/// One file per workspace under
/// `<cache_dir>/claude-token-expiry/<workspace>`. Removed on revoke.
pub fn expiry_cache_path(paths: &JackinPaths, workspace: &str) -> std::path::PathBuf {
    paths.cache_dir.join("claude-token-expiry").join(workspace)
}

/// Write the workspace's expiry stamp.
///
/// Returns `Err` so callers can reflect the cache state in the
/// report they show the operator — see `run_setup_with_runner`,
/// which sets `TokenSetupReport.expiry_estimate = None` on failure
/// so the banner-state shown to the operator matches what the
/// launch path will read back.
pub fn write_expiry_stamp(
    paths: &JackinPaths,
    workspace: &str,
    expiry: &str,
) -> std::io::Result<()> {
    let path = expiry_cache_path(paths, workspace);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, format!("{expiry}\n"))
}

/// Remove the cached expiry stamp for a workspace.
///
/// `NotFound` is fine (idempotent revoke); other errors surface as
/// a warning so a `PermissionDenied` / `IsADirectory` cache
/// collision does not silently leave a stale banner countdown in
/// place for the next launch.
pub fn clear_expiry_stamp(paths: &JackinPaths, workspace: &str) {
    let path = expiry_cache_path(paths, workspace);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            eprintln!(
                "[jackin] could not remove token-expiry cache {}: {e} \
                 (next launch may show a stale expiry banner — delete by hand if needed)",
                path.display()
            );
        }
    }
}

/// Launch-time accessor for the on-disk expiry stamp.
///
/// Returns `Ok(Some(days))` for a present, well-formed stamp,
/// `Ok(None)` when the file is absent (the legitimate "no stamp
/// yet" case), or `Err(_)` when the file is present but unreadable
/// or malformed. Collapsing the four failure modes (missing / IO
/// error / empty / parse failure) into a single
/// `Result<Option<i64>>` lets the launch site distinguish "no
/// stamp" (silent) from "broken stamp" (warn once).
pub fn expiry_days_for_launch(
    paths: &JackinPaths,
    workspace: &str,
) -> std::io::Result<Option<i64>> {
    let path = expiry_cache_path(paths, workspace);
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("token-expiry cache {} is empty", path.display()),
        ));
    }
    days_until_expiry(trimmed).map_or_else(
        || {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "token-expiry cache {} contains malformed date {trimmed:?} \
                     (expected YYYY-MM-DD)",
                    path.display()
                ),
            ))
        },
        |d| Ok(Some(d)),
    )
}

/// Days remaining until `expiry` (YYYY-MM-DD), or `None` when the
/// stamp cannot be parsed. Negative values mean expired.
pub fn days_until_expiry(expiry: &str) -> Option<i64> {
    let parsed = chrono::NaiveDate::parse_from_str(expiry, "%Y-%m-%d").ok()?;
    let today = chrono::Utc::now().date_naive();
    Some((parsed - today).num_days())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator_env::OpRef;
    use crate::workspace::WorkspaceConfig;
    use std::cell::RefCell;
    use tempfile::tempdir;

    struct FakeOpWriter {
        last_create: RefCell<Option<(String, String, String)>>, // (vault, title, field)
        produced_ref: OpRef,
        recorded_value: RefCell<Option<String>>,
        /// When `true`, `item_create` returns Err instead of recording.
        fail_create: bool,
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
                fail_create: false,
                deletes: RefCell::new(Vec::new()),
            }
        }
        fn failing() -> Self {
            Self {
                last_create: RefCell::new(None),
                produced_ref: OpRef {
                    op: "op://_/_/_".into(),
                    path: "_/_/_".into(),
                },
                recorded_value: RefCell::new(None),
                fail_create: true,
                deletes: RefCell::new(Vec::new()),
            }
        }
    }

    impl OpWriteRunner for FakeOpWriter {
        fn item_create(&self, params: OpItemCreateParams<'_>) -> anyhow::Result<OpRef> {
            if self.fail_create {
                anyhow::bail!("simulated item_create failure");
            }
            *self.last_create.borrow_mut() = Some((
                params.vault_id.to_string(),
                params.title.to_string(),
                params.field_label.to_string(),
            ));
            *self.recorded_value.borrow_mut() = Some(params.value.to_string());
            Ok(self.produced_ref.clone())
        }
        fn item_delete(
            &self,
            item_id: &str,
            vault_id: &str,
            _: Option<&str>,
        ) -> anyhow::Result<()> {
            self.deletes
                .borrow_mut()
                .push((vault_id.to_string(), item_id.to_string()));
            Ok(())
        }
    }

    struct FakeOpReader {
        /// Per-call queue. Each call pops one. When empty, `read`
        /// reuses the last value indefinitely so single-call tests
        /// can keep using `Self { values: vec![token] }`.
        values: RefCell<Vec<anyhow::Result<String>>>,
        last_ref: RefCell<Vec<String>>,
    }
    impl FakeOpReader {
        fn ok(value: &str) -> Self {
            Self {
                values: RefCell::new(vec![Ok(value.into())]),
                last_ref: RefCell::new(Vec::new()),
            }
        }
        fn err(msg: &'static str) -> Self {
            Self {
                values: RefCell::new(vec![Err(anyhow::anyhow!(msg))]),
                last_ref: RefCell::new(Vec::new()),
            }
        }
    }
    impl OpRunner for FakeOpReader {
        fn read(&self, reference: &str) -> anyhow::Result<String> {
            self.last_ref.borrow_mut().push(reference.to_string());
            let mut q = self.values.borrow_mut();
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
    fn parse_uuid_op_ref_handles_3_and_4_segment_uris() {
        assert_eq!(parse_uuid_op_ref("op://Va/It/Fl"), Some(("Va", "It")));
        assert_eq!(parse_uuid_op_ref("op://Va/It/Sec/Fl"), Some(("Va", "It")));
        assert_eq!(parse_uuid_op_ref("not-an-op-ref"), None);
        assert_eq!(parse_uuid_op_ref("op:///It/Fl"), None);
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
        }
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
            "proj",
            &TokenSetupArgs {
                vault: Some("Personal".into()),
                ..Default::default()
            },
            &probe,
            || Ok(secrecy::SecretString::from(token.to_string())),
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
        assert_eq!(report.claude_cli_version, "2.1.4");
        assert_eq!(report.token_sha256_prefix, sha256_prefix(token));
        assert!(report.created);
        assert!(report.expiry_estimate.is_some());

        // Expiry stamp landed on disk and round-trips.
        let stamp_path = expiry_cache_path(&paths, "proj");
        assert!(stamp_path.exists(), "expiry stamp must be written");
        let read_back = expiry_days_for_launch(&paths, "proj").unwrap();
        assert!(read_back.is_some(), "stamp must parse to a day count");

        // Post-write read used the canonical UUID URI, not the path.
        assert_eq!(reader.last_ref.borrow().last().unwrap(), "op://VID/IID/FID");
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
            "ghost",
            &TokenSetupArgs::default(),
            &probe,
            || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string())),
            &reader,
            &writer,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not registered"));
        assert!(writer.last_create.borrow().is_none());
    }

    #[test]
    fn run_setup_with_runner_aborts_when_vault_missing_and_no_reuse() {
        let (_t, paths, mut cfg) = seed_paths_with_workspace("proj");
        let writer = FakeOpWriter::new(dummy_op_ref());
        let reader = FakeOpReader::ok("ignored");
        let probe = dummy_probe();
        let err = run_setup_with_runner(
            &paths,
            &mut cfg,
            "proj",
            &TokenSetupArgs::default(),
            &probe,
            || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string())),
            &reader,
            &writer,
        )
        .unwrap_err();
        assert!(err.to_string().contains("--vault"));
        assert!(writer.last_create.borrow().is_none());
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
            "proj",
            &TokenSetupArgs {
                vault: Some("Personal".into()),
                ..Default::default()
            },
            &probe,
            || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string())),
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
            "proj",
            &TokenSetupArgs {
                vault: Some("Personal".into()),
                ..Default::default()
            },
            &probe,
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
            "proj",
            &TokenSetupArgs {
                vault: Some("Personal".into()),
                ..Default::default()
            },
            &probe,
            || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string())),
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
        assert_eq!(deletes[0], ("VID".to_string(), "IID".to_string()));
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
            "proj",
            &TokenSetupArgs {
                vault: Some("Personal".into()),
                ..Default::default()
            },
            &probe,
            || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string())),
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
            "proj",
            &TokenSetupArgs {
                reuse: Some(OpRef {
                    op: "op://Other/Item/Field".into(),
                    path: "Other/Item/Field".into(),
                }),
                ..Default::default()
            },
            &probe,
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
            "proj",
            &TokenSetupArgs {
                reuse: Some(OpRef {
                    op: "op://VID/IID/FID".into(),
                    path: "Personal/Existing/token".into(),
                }),
                ..Default::default()
            },
            &probe,
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
        assert_eq!(report.op_ref.path, "Personal/Existing/token");
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
        assert!(days >= 6 && days <= 7, "days = {days}");

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
        ws.claude = Some(crate::config::AgentAuthConfig {
            auth_forward: AuthForwardMode::OAuthToken,
        });
        ws.env.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            EnvValue::OpRef(OpRef {
                op: "op://VID/IID/FID".into(),
                path: "Personal/Item/token".into(),
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
        });
        assert_eq!(
            vault_for_rotate(None, Some(&prior)),
            Some("VAULT_UUID".to_string()),
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
        });
        assert_eq!(
            vault_for_rotate(Some("NewVault".into()), Some(&prior)),
            Some("NewVault".to_string()),
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
        ws.claude = Some(crate::config::AgentAuthConfig {
            auth_forward: AuthForwardMode::OAuthToken,
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
}
