//! Workspace Claude token setup orchestrator.
//!
//! Imperative pipeline glueing five primitives together:
//!
//! 1. [`crate::host_claude::probe_claude_cli`] — verify the upstream
//!    `claude` CLI is on `PATH` and capture its version.
//! 2. [`crate::host_claude::capture_setup_token`] — drive
//!    `claude setup-token` under a PTY; redacted progress goes to
//!    `stderr`, the captured token lives in `secrecy::SecretString`.
//! 3. [`crate::op_struct::OpWriteRunner::item_create`] — write a
//!    new 1Password item with the token on stdin (never argv).
//! 4. Validate the round-trip via `OpRunner::read` + SHA-256 prefix
//!    comparison BEFORE persisting any on-disk config — a vault-
//!    routing surprise must never leave a wired-but-broken slot
//!    behind.
//! 5. [`jackin_config::ConfigEditor`] — comment-preserving edit of
//!    the workspace's `[claude]` block (`auth_forward = "oauth_token"`)
//!    and `[env]` block (`CLAUDE_CODE_OAUTH_TOKEN = op://...`).
//!
//! Production callers use [`run_setup`]; tests inject mocks via
//! [`run_setup_with_runner`].
//!
//! Roadmap: `docs/src/content/docs/reference/roadmap/workspace-claude-token-setup.mdx`

use crate::host_claude;
use crate::op_cli::OpCli;
use crate::op_runner::OpRunner;
use crate::op_struct::OpWriteRunner;
use crate::resolve::CLAUDE_OAUTH_TOKEN_ENV;
use jackin_config::{AppConfig, AuthForwardMode, ConfigEditor, EnvScope};
use jackin_core::{Agent, EnvValue, FieldTarget, JackinPaths, OpRef, WorkspaceName};

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
pub(crate) const DEFAULT_ITEM_CATEGORY: &str = "API_CREDENTIAL";

/// Default field label inside the created item.
pub const DEFAULT_FIELD_LABEL: &str = "oauth-token";

/// Tags every jackin-managed item is stamped with so list / search
/// filters can find them later.
pub const JACKIN_TAG: &str = "jackin";
/// Per-workspace tag prefix (`workspace=<name>`).
pub(crate) const WORKSPACE_TAG_PREFIX: &str = "workspace=";

/// True when an item's tags mark it as jackin-created (and therefore safe
/// for rotate to delete). Keeps the [`JACKIN_TAG`] ownership rule in one
/// place so callers don't re-derive the predicate.
#[must_use]
pub fn tags_indicate_jackin_owned(tags: &[String]) -> bool {
    tags.iter().any(|t| t == JACKIN_TAG)
}

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
#[derive(Debug, Clone)]
pub struct EditExistingTarget {
    pub vault_id: String,
    pub item_id: String,
    /// Which field to write: an exact existing field id (overwrite,
    /// placement preserved) or a new field by label (see [`FieldTarget`]).
    pub field: FieldTarget,
    /// Optional 1Password section label for a newly appended field.
    /// Ignored when overwriting an existing field (its placement is
    /// preserved). `None` leaves an appended field unsectioned.
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
    let op_cli = op_cli_for_scope(
        config,
        scope,
        args.account.as_deref(),
        OpTimeoutBudget::Interactive,
    );
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
    let op_cli = op_cli_for_scope(
        config,
        scope,
        args.account.as_deref(),
        OpTimeoutBudget::Interactive,
    );
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
        let wn = WorkspaceName::parse(workspace).map_err(anyhow::Error::from)?;
        config.require_workspace(&wn)?;
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
                    &target.field,
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
        WiredValue::Plain(secret) => EnvValue::Plain(secret.expose_secret().to_owned()),
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
    if let (true, Some(workspace)) = (created, scope.workspace())
        && let Ok(wn) = WorkspaceName::parse(workspace)
    {
        let expiry = upstream_expiry_stamp();
        if let Err(e) = write_expiry_stamp(paths, &wn, &expiry) {
            crate::output::stderr_line(format_args!(
                "[jackin] note: token stored, but expiry banner cache \
                 write failed: {e} — launch banner will not show 'expires in N days' \
                 for this workspace until the next setup."
            ));
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
pub(crate) fn run_setup_with_runner<F>(
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
            let ws = WorkspaceName::parse(workspace).map_err(anyhow::Error::from)?;
            editor.set_workspace_auth_forward(
                &ws,
                Agent::Claude,
                Some(AuthForwardMode::OAuthToken),
            );
            editor.set_env_var(
                &EnvScope::Workspace(workspace.clone()),
                CLAUDE_OAUTH_TOKEN_ENV,
                env_value,
            )?;
        }
        TokenSetupScope::WorkspaceRole { workspace, role } => {
            let ws = WorkspaceName::parse(workspace).map_err(anyhow::Error::from)?;
            editor.set_workspace_role_auth_forward(
                &ws,
                role,
                Agent::Claude,
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
            editor.set_global_auth_forward(Agent::Claude, AuthForwardMode::OAuthToken);
            editor.set_env_var(&EnvScope::Global, CLAUDE_OAUTH_TOKEN_ENV, env_value)?;
        }
    }
    let saved = editor.save()?;
    *config = saved;

    let op_account = effective_account(config, scope, args.account.as_deref()).map(str::to_owned);

    // The expiry stamp landed inside `mint_token_value_with_runner`;
    // re-derive the report's estimate from the on-disk cache so a
    // write failure (which the mint path already warned about) is
    // reflected as `None` here too, matching what the launch banner
    // will read. Only the `created` + workspace-scoped path has a
    // stamp to read back.
    let expiry_estimate = match (created, scope.workspace()) {
        (true, Some(workspace)) => WorkspaceName::parse(workspace)
            .ok()
            .and_then(|wn| std::fs::read_to_string(expiry_cache_path(paths, &wn)).ok())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        _ => None,
    };

    Ok(TokenSetupReport {
        workspace: scope.label().to_owned(),
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
    workspace: &WorkspaceName,
    delete_op_item: bool,
) -> anyhow::Result<RevokeReport> {
    let op_cli = op_cli_for_scope(
        config,
        &TokenSetupScope::Workspace(workspace.as_str().to_owned()),
        None,
        OpTimeoutBudget::Quick,
    );
    run_revoke_with_runner(paths, config, workspace, delete_op_item, &op_cli)
}

/// Test-injectable variant of [`run_revoke`].
pub(crate) fn run_revoke_with_runner(
    paths: &JackinPaths,
    config: &mut AppConfig,
    workspace: &WorkspaceName,
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
                let parts =
                    jackin_core::op_reference::parse_op_reference(&r.op).ok_or_else(|| {
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
            Some(EnvValue::Plain(_) | EnvValue::Extended(_)) => {
                anyhow::bail!(
                    "--delete-op-item requested but workspace {workspace} has a literal \
                     token slot (not an op:// reference); jackin does not know where the \
                     literal came from. Re-run without --delete-op-item to clear the slot."
                );
            }
            None => {
                anyhow::bail!(
                    "--delete-op-item requested but workspace {workspace} has no \
                     CLAUDE_CODE_OAUTH_TOKEN slot to delete from."
                );
            }
        }
    } else {
        false
    };

    let mut editor = ConfigEditor::open(paths)?;
    editor.remove_env_var(
        &EnvScope::Workspace(workspace.as_str().to_owned()),
        CLAUDE_OAUTH_TOKEN_ENV,
    );
    editor.set_workspace_auth_forward(workspace, Agent::Claude, Some(AuthForwardMode::Ignore));
    let saved = editor.save()?;
    *config = saved;

    // Drop the cached expiry stamp — the slot is gone, the banner
    // should not surface a stale countdown for the next launch.
    clear_expiry_stamp(paths, workspace);

    Ok(RevokeReport {
        workspace: workspace.as_str().to_owned(),
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
            EnvValue::OpRef(r) => {
                jackin_core::op_reference::parse_op_reference(&r.op).map(|p| p.vault)
            }
            EnvValue::Plain(_) | EnvValue::Extended(_) => None,
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
pub fn run_doctor(config: &AppConfig, workspace: &WorkspaceName) -> anyhow::Result<DoctorReport> {
    let op_cli = op_cli_for_scope(
        config,
        &TokenSetupScope::Workspace(workspace.as_str().to_owned()),
        None,
        OpTimeoutBudget::Quick,
    );
    run_doctor_with_runner(config, workspace, &op_cli)
}

/// Test-injectable variant of [`run_doctor`].
pub(crate) fn run_doctor_with_runner(
    config: &AppConfig,
    workspace: &WorkspaceName,
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
            "workspace {workspace} has no CLAUDE_CODE_OAUTH_TOKEN in its env block — \
             run `jackin workspace claude-token setup` first"
        )
    })?;
    let account = effective_account(
        config,
        &TokenSetupScope::Workspace(workspace.as_str().to_owned()),
        None,
    )
    .map(str::to_owned);
    let token = match token_decl {
        EnvValue::Plain(t) => t.clone(),
        EnvValue::Extended(e) => e.value.clone(),
        EnvValue::OpRef(r) => op_reader
            .read_with_account(&r.op, r.account.as_deref())
            .map_err(|e| anyhow::anyhow!("op read for {:?} failed: {e}", r.path))?,
    };
    let prefix = sha256_prefix(&token);

    Ok(DoctorReport {
        workspace: workspace.as_str().to_owned(),
        mode,
        op_ref: match token_decl {
            EnvValue::OpRef(r) => Some(r.clone()),
            EnvValue::Plain(_) | EnvValue::Extended(_) => None,
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
    use crate::op_struct::OpItemCreateParams;

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
    match scope_token_slot(config, scope) {
        Some(EnvValue::OpRef(r)) => r.account.as_deref(),
        _ => None,
    }
}

/// The canonical Claude-token env slot for a scope: the workspace-level
/// slot for `Workspace`, the role-level slot for `WorkspaceRole`, the
/// global slot for `Global`. Single lookup shared by account resolution
/// and the rotate prior-slot read so both agree on where a scope's token
/// lives.
fn scope_token_slot<'a>(config: &'a AppConfig, scope: &TokenSetupScope) -> Option<&'a EnvValue> {
    match scope {
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
    }
}

/// The prior Claude-token value at a scope, cloned for the rotate flow so
/// it can derive the prior item's vault and delete it after the new mint.
#[must_use]
pub fn prior_token_slot(config: &AppConfig, scope: &TokenSetupScope) -> Option<EnvValue> {
    scope_token_slot(config, scope).cloned()
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
    budget: OpTimeoutBudget,
) -> OpCli {
    let account = effective_account(config, scope, explicit).map(str::to_owned);
    let cli = match budget {
        OpTimeoutBudget::Interactive => OpCli::new_interactive(),
        OpTimeoutBudget::Quick => OpCli::new(),
    };
    cli.with_account(account)
}

/// Timeout ceiling for an [`OpCli`] built by [`op_cli_for_scope`].
#[derive(Clone, Copy)]
enum OpTimeoutBudget {
    /// 5-minute budget for the write paths (`run_setup`, `mint_token_value`,
    /// rotate): an `op item create`/`edit` may block on a biometric or SSO
    /// round-trip the operator completes in a browser, which the default
    /// ceiling would time out.
    Interactive,
    /// Default 30s ceiling for read-only `doctor` and `revoke` so a locked
    /// or stalled `op` fails fast instead of hanging a quick check.
    Quick,
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
        let Some(parts) = jackin_core::op_reference::parse_op_reference(&op_ref.op) else {
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
            let _unused = write!(acc, "{byte:02x}");
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
pub(crate) fn expiry_cache_path(
    paths: &JackinPaths,
    workspace: &WorkspaceName,
) -> std::path::PathBuf {
    paths
        .cache_dir
        .join("claude-token-expiry")
        .join(workspace.as_str())
}

/// Write the workspace's expiry stamp.
///
/// Returns `Err` so callers can reflect the cache state in the
/// report they show the operator — see `run_setup_with_runner`,
/// which sets `TokenSetupReport.expiry_estimate = None` on failure
/// so the banner-state shown to the operator matches what the
/// launch path will read back.
pub(crate) fn write_expiry_stamp(
    paths: &JackinPaths,
    workspace: &WorkspaceName,
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
pub(crate) fn clear_expiry_stamp(paths: &JackinPaths, workspace: &WorkspaceName) {
    let path = expiry_cache_path(paths, workspace);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            crate::output::stderr_line(format_args!(
                "[jackin] could not remove token-expiry cache {}: {e} \
                 (next launch may show a stale expiry banner — delete by hand if needed)",
                path.display()
            ));
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
    workspace: &WorkspaceName,
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
pub(crate) fn days_until_expiry(expiry: &str) -> Option<i64> {
    let parsed = chrono::NaiveDate::parse_from_str(expiry, "%Y-%m-%d").ok()?;
    let today = chrono::Utc::now().date_naive();
    Some((parsed - today).num_days())
}

#[cfg(test)]
mod tests;
