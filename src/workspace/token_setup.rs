//! Workspace Claude token setup orchestrator.
//!
//! Imperative pipeline glueing five primitives together:
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
use crate::operator_env::{
    CLAUDE_OAUTH_TOKEN_ENV, EnvValue, OpItemCreateParams, OpRef, OpRunner, OpWriteRunner,
};
use crate::paths::JackinPaths;

use secrecy::ExposeSecret;
use sha2::Digest;

/// Default `op` item title (the literal `Claude`).
///
/// Operators can override with [`TokenSetupArgs::item_name`]; a custom
/// title may still contain `{ws}`, which substitutes the scope label
/// (workspace name, or `global`). The default has no placeholder.
pub const DEFAULT_ITEM_TEMPLATE: &str = "Claude";

/// Default `op` item category for OAuth tokens (1Password's API
/// Credential category renders `token` as a concealed field).
pub const DEFAULT_ITEM_CATEGORY: &str = "API_CREDENTIAL";

/// Default field label inside the created item.
pub const DEFAULT_FIELD_LABEL: &str = "oauth-token";

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
    /// Pin this run to a specific 1P account; falls back to the account
    /// the workspace's stored ref was created under, then `op`'s default.
    pub account: Option<String>,
    /// If set, skip token generation and adopt the supplied
    /// `op://...` reference verbatim. Used for setup-from-existing
    /// flows.
    pub reuse: Option<OpRef>,
    /// Override the field label inside the created item. Falls back to
    /// [`DEFAULT_FIELD_LABEL`] when `None`.
    pub field_label: Option<String>,
    /// When set, overwrite (or add) a field in an existing 1Password
    /// item instead of creating a new one. Mutually exclusive with
    /// `vault`, `item_name`, and `reuse`.
    pub edit_existing: Option<EditExistingTarget>,
    /// Optional 1Password section label for the field on the new-item
    /// path. `None` leaves the field unsectioned.
    pub section: Option<String>,
    /// Mint the token and store it as a literal value in config
    /// (cleartext `CLAUDE_CODE_OAUTH_TOKEN`) instead of writing a
    /// 1Password item. Mutually exclusive with `reuse` and
    /// `edit_existing` (both are op-only). No vault is required.
    pub plain_text: bool,
}

/// Where a generated token's config wiring lands.
#[derive(Debug, Clone)]
pub enum TokenSetupScope {
    /// Wire `[workspaces.<name>]` claude auth + the workspace env slot.
    Workspace(String),
    /// Wire `[workspaces.<name>.roles.<role>]` claude auth + that role's
    /// env slot — a per-role override inside the workspace.
    WorkspaceRole { workspace: String, role: String },
    /// Wire the global `[claude]` auth + the global env slot.
    Global,
}

impl TokenSetupScope {
    /// Workspace name for the workspace-scoped variants, used to stamp
    /// the op item title, expiry cache, op account, and report line;
    /// `None` for `Global`.
    fn workspace(&self) -> Option<&str> {
        match self {
            Self::Workspace(name)
            | Self::WorkspaceRole {
                workspace: name, ..
            } => Some(name),
            Self::Global => None,
        }
    }

    /// Item-title / op-tag label: the workspace name, or `"global"`.
    fn label(&self) -> &str {
        self.workspace().unwrap_or("global")
    }
}

/// Identifies an existing 1Password item and field to update in-place
/// during the interactive `--interactive` token-setup path.
#[derive(Debug, Clone, Default)]
pub struct EditExistingTarget {
    pub vault_id: String,
    pub item_id: String,
    /// Field label to overwrite, or name of a new field to append.
    pub field_label: String,
    /// Optional 1Password section label for the field on the
    /// edit/new-field path. `None` leaves the field unsectioned.
    pub section: Option<String>,
}

/// Outcome of one orchestrator run.
#[derive(Debug, Clone)]
pub struct TokenSetupReport {
    pub workspace: String,
    /// Probed `claude` CLI version. `None` on `--reuse` because the
    /// orchestrator does not invoke `claude` on that path.
    pub claude_cli_version: Option<String>,
    /// `Some` for op-backed wiring (the canonical UUID-form reference);
    /// `None` for the plain-text path, where the token is stored as a
    /// literal in config and there is no op item to reference.
    pub op_ref: Option<OpRef>,
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
    scope: &TokenSetupScope,
    args: &TokenSetupArgs,
) -> anyhow::Result<TokenSetupReport> {
    let op_cli = op_cli_for_scope(config, scope, args.account.as_deref());
    // Probe `claude` only when we will actually mint a fresh token.
    // `--reuse` adopts an existing `op://` reference and never invokes
    // claude, so requiring it on PATH would block a legitimate flow.
    let probe = if args.reuse.is_some() {
        None
    } else {
        Some(host_claude::probe_claude_cli()?)
    };
    run_setup_with_runner(
        paths,
        config,
        scope,
        args,
        probe.as_ref(),
        host_claude::capture_setup_token,
        &op_cli,
        &op_cli,
    )
}

/// Production entry the TUI calls to mint a Claude OAuth token (or
/// adopt a reuse reference) and return the wired [`EnvValue`] WITHOUT
/// writing any jackin config.
///
/// The op item create + post-write validation still run, and the
/// expiry stamp is still written; only the `[claude]` / `[env]` config
/// edit is skipped. The TUI stages the wiring into the open auth form
/// and persists it when the operator Saves, mirroring the provide
/// path. The CLI keeps using [`run_setup`] (full mint + persist)
/// because it has no form to return to.
pub fn mint_token_value(
    paths: &JackinPaths,
    config: &AppConfig,
    scope: &TokenSetupScope,
    args: &TokenSetupArgs,
) -> anyhow::Result<EnvValue> {
    let op_cli = op_cli_for_scope(config, scope, args.account.as_deref());
    let probe = if args.reuse.is_some() {
        None
    } else {
        Some(host_claude::probe_claude_cli()?)
    };
    let outcome = mint_token_value_with_runner(
        paths,
        config,
        scope,
        args,
        probe.as_ref(),
        host_claude::capture_setup_token,
        &op_cli,
        &op_cli,
    )?;
    Ok(outcome.env_value)
}

/// The mint-only result: the wired [`EnvValue`] plus the metadata
/// [`run_setup_with_runner`] folds into a [`TokenSetupReport`].
struct MintOutcome {
    wired: WiredValue,
    env_value: EnvValue,
    token_sha256_prefix: String,
    created: bool,
}

/// Mint + validate (and stamp expiry) WITHOUT persisting jackin config.
///
/// Everything-up-to-validation logic split out of
/// [`run_setup_with_runner`] so the TUI can mint and obtain the wired
/// [`EnvValue`] (an `OpRef` for the op paths, a `Plain` literal for
/// `--plain`) without writing config — the form Save handles
/// persistence on the TUI generate path. [`run_setup_with_runner`]
/// calls this then does the config edit, so there is no duplication.
///
/// The op-item create + post-write read-back validation live here (that
/// safety is not tied to the config persist): a vault-routing surprise
/// must never leave a wired-but-broken slot. The expiry stamp is
/// written here too (the token was minted; issuance is known) — a
/// harmless cache even if the operator later cancels the Save.
///
/// Takes the same injection seams as [`run_setup_with_runner`]: a
/// pre-resolved Claude probe (`None` on `--reuse`), a capture closure,
/// an [`OpRunner`] for read-back, and an [`OpWriteRunner`] for create.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn mint_token_value_with_runner<F>(
    paths: &JackinPaths,
    config: &AppConfig,
    scope: &TokenSetupScope,
    args: &TokenSetupArgs,
    probe: Option<&host_claude::ClaudeProbe>,
    capture: F,
    op_reader: &dyn OpRunner,
    op_writer: &dyn OpWriteRunner,
) -> anyhow::Result<MintOutcome>
where
    F: FnOnce() -> anyhow::Result<secrecy::SecretString>,
{
    if let Some(workspace) = scope.workspace() {
        config.require_workspace(workspace)?;
    }

    // plain_text is op-incompatible: `reuse` adopts an existing op
    // reference and `edit_existing` writes into an existing op item, so
    // neither has a literal value to store. Bail before any capture.
    if args.plain_text && (args.reuse.is_some() || args.edit_existing.is_some()) {
        anyhow::bail!(
            "--plain is mutually exclusive with --reuse and the edit-existing path: \
             both wire a 1Password reference, while --plain stores the minted token \
             as a literal in config. Pick one."
        );
    }

    // `created_new_item` is true ONLY when jackin minted a brand-new op
    // item it owns (the `create_op_item` path). It gates orphan deletion:
    // the `edit_existing` path writes one field into the operator's
    // PRE-EXISTING item, so a validation failure must never delete that
    // item (it would take the operator's other fields with it).
    let (wired, token_sha256_prefix, created, created_new_item) =
        if let Some(reuse_ref) = args.reuse.as_ref() {
            let value = op_reader
                .read_with_account(&reuse_ref.op, reuse_ref.account.as_deref())
                .map_err(|e| {
                    anyhow::anyhow!(
                        "validation of --reuse reference {:?} failed: {e} \
                     (vault / item / field correct?)",
                        reuse_ref.path
                    )
                })?;
            let prefix = sha256_prefix(&value);
            (WiredValue::Op(reuse_ref.clone()), prefix, false, false)
        } else if args.plain_text {
            // Mint, then store the literal in config — no op item, so no
            // vault requirement and no post-write read-back to validate.
            let probe = probe.ok_or_else(|| {
                anyhow::anyhow!("internal error: claude probe missing on capture path")
            })?;
            let _ = probe;
            let secret = capture()?;
            let prefix = sha256_prefix(secret.expose_secret());
            (WiredValue::Plain(secret), prefix, true, false)
        } else {
            // Validate that a destination is specified before launching the
            // OAuth flow so the operator is not prompted to complete
            // authentication only to hit a required-arg error afterward.
            if args.vault.is_none() && args.reuse.is_none() && args.edit_existing.is_none() {
                anyhow::bail!(
                    "no --vault supplied; `jackin workspace claude-token setup` and \
                 `jackin workspace claude-token rotate` need --vault <name-or-uuid> \
                 so the new item lands somewhere explicit. Pass --reuse if you \
                 already have an op:// reference to adopt instead, or --plain to \
                 store the minted token as a literal in config."
                );
            }
            let probe = probe.ok_or_else(|| {
                anyhow::anyhow!("internal error: claude probe missing on capture path")
            })?;
            let secret = capture()?;
            let prefix = sha256_prefix(secret.expose_secret());
            let (op_ref, created_new_item) = if let Some(target) = args.edit_existing.as_ref() {
                // Writing a field into the operator's existing item: not a
                // deletable orphan on validation failure.
                let op_ref = op_writer.item_field_set(
                    &target.item_id,
                    &target.vault_id,
                    &target.field_label,
                    secret.expose_secret(),
                    target.section.as_deref(),
                )?;
                (op_ref, false)
            } else {
                // jackin minted a brand-new item it owns: safe to delete if
                // validation fails.
                (
                    create_op_item(op_writer, scope, args, &secret, &prefix, probe)?,
                    true,
                )
            };
            (WiredValue::Op(op_ref), prefix, true, created_new_item)
        };

    // Validate the write BEFORE persisting any on-disk config: a wired
    // slot pointing at an item whose value the operator never saw
    // would silently inject a mystery token at the next launch. Skip
    // on `--reuse` (both reads target the same pre-existing item, so
    // the comparison is meaningless) and on the plain-text path (the
    // literal lands directly in config — there is no op item to read
    // back).
    if let (true, WiredValue::Op(op_ref)) = (created, &wired) {
        // On failure, only the new-item path may delete: it created an
        // item jackin owns. The edit-existing path wrote one field into
        // the operator's pre-existing item — deleting it would destroy
        // their other fields — so it reports the failure without cleanup.
        let cleanup = |reason: PostWriteCleanup| {
            if created_new_item {
                PostWriteCleanup::Orphan(OrphanCleanup::run(
                    op_writer,
                    op_ref,
                    args.account.as_deref(),
                ))
            } else {
                reason
            }
        };
        let resolved = op_reader
            .read_with_account(&op_ref.op, op_ref.account.as_deref())
            .map_err(|e| {
                let outcome = cleanup(PostWriteCleanup::EditedExistingKept);
                anyhow::anyhow!(
                    "post-write validation failed: re-reading {:?} returned: {e} \
                     (no on-disk config was changed). {outcome}",
                    op_ref.path
                )
            })?;
        let resolved_prefix = sha256_prefix(&resolved);
        if resolved_prefix != token_sha256_prefix {
            let outcome = cleanup(PostWriteCleanup::EditedExistingKept);
            anyhow::bail!(
                "post-write validation failed: {:?} resolved to a value whose SHA-256 prefix \
                 ({resolved_prefix}) does not match the captured value ({token_sha256_prefix}). \
                 No on-disk config was changed. {outcome}",
                op_ref.path,
            );
        }
    }

    // Single derivation of the env value wired into every scope arm:
    // op reference for the op paths, literal token for plain-text.
    let env_value = match &wired {
        WiredValue::Op(op_ref) => EnvValue::OpRef(op_ref.clone()),
        WiredValue::Plain(secret) => EnvValue::Plain(secret.expose_secret().to_string()),
    };

    // Stamp the expiry estimate into the local cache so the launch
    // diagnostic can render an `expires in N days` banner without
    // re-reading the 1P item's `notesPlain`. Only do this on the
    // `created` path — for `--reuse` we did not mint the token, so
    // the issuance date is unknown and any stamp would mislead. If
    // the cache write fails (filesystem full / permission), the
    // operator is told.
    //
    // The expiry stamp is keyed per workspace; the launch banner that
    // reads it is per-workspace too, so the `Global` scope has no
    // banner to feed and skips the stamp entirely.
    if let (true, Some(workspace)) = (created, scope.workspace()) {
        let expiry = upstream_expiry_stamp();
        if let Err(e) = write_expiry_stamp(paths, workspace, &expiry) {
            eprintln!(
                "[jackin] note: token stored, but expiry banner cache \
                 write failed: {e} — launch banner will not show 'expires in N days' \
                 for this workspace until the next setup."
            );
        }
    }

    Ok(MintOutcome {
        wired,
        env_value,
        token_sha256_prefix,
        created,
    })
}

/// Test-injectable variant of the full CLI path: mint + validate (via
/// [`mint_token_value_with_runner`]) then persist the `[claude]` /
/// `[env]` config edit. Takes:
/// - a pre-resolved Claude CLI probe (skips re-running `claude --version`);
///   `None` on the `--reuse` path because no token capture happens,
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
    scope: &TokenSetupScope,
    args: &TokenSetupArgs,
    probe: Option<&host_claude::ClaudeProbe>,
    capture: F,
    op_reader: &dyn OpRunner,
    op_writer: &dyn OpWriteRunner,
) -> anyhow::Result<TokenSetupReport>
where
    F: FnOnce() -> anyhow::Result<secrecy::SecretString>,
{
    let MintOutcome {
        wired,
        env_value,
        token_sha256_prefix,
        created,
    } = mint_token_value_with_runner(
        paths, config, scope, args, probe, capture, op_reader, op_writer,
    )?;

    // Persist the config last: a partial failure earlier must never
    // leave a wired-but-broken slot.
    let mut editor = ConfigEditor::open(paths)?;
    match scope {
        TokenSetupScope::Workspace(workspace) => {
            editor.set_workspace_auth_forward(
                workspace,
                crate::agent::Agent::Claude,
                Some(AuthForwardMode::OAuthToken),
            );
            editor.set_env_var(
                &EnvScope::Workspace(workspace.clone()),
                CLAUDE_OAUTH_TOKEN_ENV,
                env_value,
            )?;
        }
        TokenSetupScope::WorkspaceRole { workspace, role } => {
            editor.set_workspace_role_auth_forward(
                workspace,
                role,
                crate::agent::Agent::Claude,
                Some(AuthForwardMode::OAuthToken),
            );
            editor.set_env_var(
                &EnvScope::WorkspaceRole {
                    workspace: workspace.clone(),
                    role: role.clone(),
                },
                CLAUDE_OAUTH_TOKEN_ENV,
                env_value,
            )?;
        }
        TokenSetupScope::Global => {
            editor
                .set_global_auth_forward(crate::agent::Agent::Claude, AuthForwardMode::OAuthToken);
            editor.set_env_var(&EnvScope::Global, CLAUDE_OAUTH_TOKEN_ENV, env_value)?;
        }
    }
    let saved = editor.save()?;
    *config = saved;

    let op_account = effective_account(config, scope, args.account.as_deref()).map(str::to_string);

    // The expiry stamp landed inside `mint_token_value_with_runner`;
    // re-derive the report's estimate from the on-disk cache so a
    // write failure (which the mint path already warned about) is
    // reflected as `None` here too, matching what the launch banner
    // will read. Only the `created` + workspace-scoped path has a
    // stamp to read back.
    let expiry_estimate = match (created, scope.workspace()) {
        (true, Some(workspace)) => std::fs::read_to_string(expiry_cache_path(paths, workspace))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        _ => None,
    };

    Ok(TokenSetupReport {
        workspace: scope.label().to_string(),
        claude_cli_version: probe.map(|p| p.version.clone()),
        op_ref: match wired {
            WiredValue::Op(op_ref) => Some(op_ref),
            WiredValue::Plain(_) => None,
        },
        op_account,
        token_sha256_prefix,
        created,
        expiry_estimate,
    })
}

/// The value the orchestrator wires into the canonical
/// `CLAUDE_CODE_OAUTH_TOKEN` slot: a 1Password reference (reuse / edit /
/// create paths) or the minted token as a literal (plain-text path).
///
/// `Plain` holds the secret in [`secrecy::SecretString`] so it never
/// reaches argv or a log line; the only place the cleartext leaves it is
/// the `EnvValue::Plain` written to config.
enum WiredValue {
    Op(OpRef),
    Plain(secrecy::SecretString),
}

/// Revoke the workspace's token: clear the canonical slot, switch
/// `auth_forward` to `ignore`, and (optionally) delete the 1P item.
///
/// `delete_op_item` requires that the workspace's existing
/// `oauth_token` slot resolves to an `op://` reference. If the
/// operator passes `--delete-op-item` but the prior slot is a
/// literal token or an unparseable URI, this returns `Err` rather
/// than silently clearing the slot only — the operator explicitly
/// asked for a 1P-side delete, and a no-op exit-zero would let the
/// secret survive in the vault without any feedback.
pub fn run_revoke(
    paths: &JackinPaths,
    config: &mut AppConfig,
    workspace: &str,
    delete_op_item: bool,
) -> anyhow::Result<RevokeReport> {
    let op_cli = op_cli_for_scope(
        config,
        &TokenSetupScope::Workspace(workspace.to_string()),
        None,
    );
    run_revoke_with_runner(paths, config, workspace, delete_op_item, &op_cli)
}

/// Test-injectable variant of [`run_revoke`].
pub fn run_revoke_with_runner(
    paths: &JackinPaths,
    config: &mut AppConfig,
    workspace: &str,
    delete_op_item: bool,
    op_writer: &dyn OpWriteRunner,
) -> anyhow::Result<RevokeReport> {
    let prior = config
        .require_workspace(workspace)?
        .env
        .get(CLAUDE_OAUTH_TOKEN_ENV)
        .cloned();

    let deleted_item = if delete_op_item {
        match prior.as_ref() {
            Some(EnvValue::OpRef(r)) => {
                let parts = crate::operator_env::parse_op_reference(&r.op).ok_or_else(|| {
                    anyhow::anyhow!(
                        "--delete-op-item requested but slot {:?} did not parse into a \
                         vault/item op-ref; clear the workspace via plain `revoke` and \
                         delete the item by hand from 1Password.",
                        r.op
                    )
                })?;
                op_writer.item_delete(&parts.item, &parts.vault, None)?;
                true
            }
            Some(EnvValue::Plain(_)) => {
                anyhow::bail!(
                    "--delete-op-item requested but workspace {workspace:?} has a literal \
                     token slot (not an op:// reference); jackin does not know where the \
                     literal came from. Re-run without --delete-op-item to clear the slot."
                );
            }
            None => {
                anyhow::bail!(
                    "--delete-op-item requested but workspace {workspace:?} has no \
                     CLAUDE_CODE_OAUTH_TOKEN slot to delete from."
                );
            }
        }
    } else {
        false
    };

    let mut editor = ConfigEditor::open(paths)?;
    editor.remove_env_var(
        &EnvScope::Workspace(workspace.to_string()),
        CLAUDE_OAUTH_TOKEN_ENV,
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
    let op_cli = op_cli_for_scope(
        config,
        &TokenSetupScope::Workspace(workspace.to_string()),
        None,
    );
    run_doctor_with_runner(config, workspace, &op_cli)
}

/// Test-injectable variant of [`run_doctor`].
pub fn run_doctor_with_runner(
    config: &AppConfig,
    workspace: &str,
    op_reader: &dyn OpRunner,
) -> anyhow::Result<DoctorReport> {
    let ws = config.require_workspace(workspace)?;
    let mode = ws
        .claude
        .as_ref()
        .map(|c| c.auth_forward)
        .unwrap_or_default();
    let token_decl = ws.env.get(CLAUDE_OAUTH_TOKEN_ENV).ok_or_else(|| {
        anyhow::anyhow!(
            "workspace {workspace:?} has no CLAUDE_CODE_OAUTH_TOKEN in its env block — \
             run `jackin workspace claude-token setup` first"
        )
    })?;
    let account = effective_account(
        config,
        &TokenSetupScope::Workspace(workspace.to_string()),
        None,
    )
    .map(str::to_string);
    let token = match token_decl {
        EnvValue::Plain(t) => t.clone(),
        EnvValue::OpRef(r) => op_reader
            .read_with_account(&r.op, r.account.as_deref())
            .map_err(|e| anyhow::anyhow!("op read for {:?} failed: {e}", r.path))?,
    };
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
    scope: &TokenSetupScope,
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

    // `{ws}` in the title template substitutes the scope label: the
    // workspace name, or the literal `global` for the global scope.
    let label = scope.label();
    let title_template = args.item_name.as_deref().unwrap_or(DEFAULT_ITEM_TEMPLATE);
    let title = title_template.replace("{ws}", label);

    // Single deterministic scoping tag: `workspace=<name>` for a
    // workspace, `workspace=global` for the global scope. Reusing the
    // existing prefix keeps list/search filters uniform across scopes.
    let workspace_tag = format!("{WORKSPACE_TAG_PREFIX}{label}");
    let expires = upstream_expiry_stamp();
    let notes = format!(
        "Managed by jackin\n\
         workspace = {label}\n\
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

    let effective_field_label = args.field_label.as_deref().unwrap_or(DEFAULT_FIELD_LABEL);
    let params = OpItemCreateParams {
        vault_id: vault,
        title: &title,
        category: DEFAULT_ITEM_CATEGORY,
        field_label: effective_field_label,
        value: secret.expose_secret(),
        notes_plain: Some(&notes),
        tags: &tags,
        section: args.section.as_deref(),
    };
    op_writer.item_create(params)
}

/// The 1Password account the scope's stored `CLAUDE_CODE_OAUTH_TOKEN`
/// ref was created under, recovered from `OpRef::account`. `revoke` /
/// `doctor` / `setup` pin `op` to this so the slot resolves against the
/// same account that minted it.
///
/// Reads the env slot that matches the scope — the workspace-level slot
/// for `Workspace`, the role-level slot for `WorkspaceRole`, the global
/// slot for `Global` — so a per-role override created under account A is
/// not resolved against the workspace slot's account B.
fn stored_op_account<'a>(config: &'a AppConfig, scope: &TokenSetupScope) -> Option<&'a str> {
    let slot = match scope {
        TokenSetupScope::Workspace(workspace) => config
            .workspaces
            .get(workspace)?
            .env
            .get(CLAUDE_OAUTH_TOKEN_ENV),
        TokenSetupScope::WorkspaceRole { workspace, role } => config
            .workspaces
            .get(workspace)?
            .roles
            .get(role)?
            .env
            .get(CLAUDE_OAUTH_TOKEN_ENV),
        TokenSetupScope::Global => config.env.get(CLAUDE_OAUTH_TOKEN_ENV),
    };
    match slot {
        Some(EnvValue::OpRef(r)) => r.account.as_deref(),
        _ => None,
    }
}

fn effective_account<'a>(
    config: &'a AppConfig,
    scope: &TokenSetupScope,
    explicit: Option<&'a str>,
) -> Option<&'a str> {
    explicit.or_else(|| stored_op_account(config, scope))
}

/// Single seam for the `effective_account` → `OpCli::with_account`
/// prelude shared by `run_setup` / `run_revoke` / `run_doctor`. Prefers
/// the explicit `--account`, else the account the scope's stored ref was
/// created under (see [`stored_op_account`] for the per-scope slot).
fn op_cli_for_scope(
    config: &AppConfig,
    scope: &TokenSetupScope,
    explicit: Option<&str>,
) -> crate::operator_env::OpCli {
    let account = effective_account(config, scope, explicit).map(str::to_string);
    crate::operator_env::OpCli::new().with_account(account)
}

/// Outcome of the post-write orphan-cleanup attempt that runs when
/// validation fails inside `run_setup_with_runner`. Surfaced in the
/// final bail message so the operator can tell whether the
/// freshly-created 1P item still needs hand-removal.
enum OrphanCleanup {
    Deleted,
    UnparseableRef { op: String },
    DeleteFailed { err: String, hint: String },
}

impl OrphanCleanup {
    fn run(op_writer: &dyn OpWriteRunner, op_ref: &OpRef, account: Option<&str>) -> Self {
        let Some(parts) = crate::operator_env::parse_op_reference(&op_ref.op) else {
            return Self::UnparseableRef {
                op: op_ref.op.clone(),
            };
        };
        match op_writer.item_delete(&parts.item, &parts.vault, account) {
            Ok(()) => Self::Deleted,
            Err(e) => Self::DeleteFailed {
                err: e.to_string(),
                hint: parts.manual_delete_hint().to_string(),
            },
        }
    }
}

impl std::fmt::Display for OrphanCleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deleted => f.write_str("The just-created 1P item was deleted."),
            Self::UnparseableRef { op } => write!(
                f,
                "Orphan was NOT deleted: op-ref {op:?} did not parse into vault/item ids; \
                 remove the freshly-created item by hand from 1Password."
            ),
            Self::DeleteFailed { err, hint } => write!(
                f,
                "The just-created 1P item was NOT deleted ({err}); remove by hand: `{hint}`."
            ),
        }
    }
}

/// What the post-write validation failure did about the written item.
/// The new-item path may delete the orphan it just created; the
/// edit-existing path must keep the operator's pre-existing item intact.
enum PostWriteCleanup {
    Orphan(OrphanCleanup),
    EditedExistingKept,
}

impl std::fmt::Display for PostWriteCleanup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Orphan(o) => o.fmt(f),
            Self::EditedExistingKept => f.write_str(
                "The field was written into your existing 1Password item, which was left \
                 intact — verify or fix that field by hand.",
            ),
        }
    }
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
                },
                recorded_value: RefCell::new(None),
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
            if self.fail_delete {
                anyhow::bail!("simulated item_delete failure");
            }
            Ok(())
        }
        fn item_field_set(
            &self,
            _item_id: &str,
            _vault_id: &str,
            field_label: &str,
            value: &str,
            _section: Option<&str>,
        ) -> anyhow::Result<OpRef> {
            if self.fail_create {
                anyhow::bail!("simulated item_field_set failure");
            }
            *self.last_create.borrow_mut() = Some((
                "existing-vault".to_string(),
                "existing-item".to_string(),
                field_label.to_string(),
            ));
            *self.recorded_value.borrow_mut() = Some(value.to_string());
            Ok(self.produced_ref.clone())
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
            &TokenSetupScope::Workspace("proj".into()),
            &TokenSetupArgs {
                vault: Some("Personal".into()),
                ..Default::default()
            },
            Some(&probe),
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
        assert_eq!(reader.last_ref.borrow().last().unwrap(), "op://VID/IID/FID");
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
            || Ok(secrecy::SecretString::from(token.to_string())),
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
            || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string())),
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
                Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string()))
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
            &TokenSetupScope::Workspace("proj".into()),
            &TokenSetupArgs {
                vault: Some("Personal".into()),
                ..Default::default()
            },
            Some(&probe),
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
            &TokenSetupScope::Workspace("proj".into()),
            &TokenSetupArgs {
                reuse: Some(OpRef {
                    op: "op://Other/Item/Field".into(),
                    path: "Other/Item/Field".into(),
                    account: None,
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
            || Ok(secrecy::SecretString::from(token.to_string())),
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
            reader.last_ref.borrow().is_empty(),
            "plain-text path must not read back from op"
        );

        // Token wired as a literal, not an op ref.
        let env_val = cfg
            .workspaces
            .get("proj")
            .and_then(|w| w.env.get("CLAUDE_CODE_OAUTH_TOKEN"));
        assert_eq!(env_val, Some(&EnvValue::Plain(token.to_string())));

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
                }),
                ..Default::default()
            },
            Some(&probe),
            || {
                capture_called.set(true);
                Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string()))
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
        ws.claude = Some(crate::config::AgentAuthConfig {
            auth_forward: AuthForwardMode::OAuthToken,
        });
        ws.env.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            EnvValue::OpRef(OpRef {
                op: "op://VID/IID/FID".into(),
                path: "Personal/Item/token".into(),
                account: None,
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
            account: None,
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
        ws.claude = Some(crate::config::AgentAuthConfig {
            auth_forward: AuthForwardMode::OAuthToken,
        });
        ws.env.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            EnvValue::OpRef(OpRef {
                op: "op://VAULT_UUID/ITEM_UUID/FIELD_UUID".into(),
                path: "Personal/Item/token".into(),
                account: None,
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
            vec![("VAULT_UUID".to_string(), "ITEM_UUID".to_string())],
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
        ws.claude = Some(crate::config::AgentAuthConfig {
            auth_forward: AuthForwardMode::OAuthToken,
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
        ws.claude = Some(crate::config::AgentAuthConfig {
            auth_forward: AuthForwardMode::OAuthToken,
        });
        ws.env.insert(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            EnvValue::OpRef(OpRef {
                op: "op://VID/IID/FID".into(),
                path: "Personal/Item/token".into(),
                account: None,
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
            || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string())),
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
            || Ok(secrecy::SecretString::from("sk-ant-oat01-X".to_string())),
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
                    field_label: "token".into(),
                    section: None,
                }),
                ..Default::default()
            },
            Some(&probe),
            || {
                Ok(secrecy::SecretString::from(
                    "sk-ant-oat01-CAPTURED".to_string(),
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
}
