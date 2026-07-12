//! Env layer resolution, merge, and launch diagnostics.

use crate::env_layer::EnvLayer;
use crate::op_cli::OpCli;
use crate::op_runner::{OpRunner, resolve_env_value};
use crate::op_struct::OpStructRunner;
use crate::parse_helpers::parse_host_ref;
use jackin_config::AppConfig;
use jackin_core::op_reference::parse_op_reference;
use jackin_core::op_types::OpItem;
use jackin_core::{EnvValue, OpRef, WorkspaceName};

/// Typed failures for operator-env validation and `op://` URI resolution.
///
/// Constructed at the resolve boundary and either returned directly (pure
/// validation paths) or attached as an `anyhow` source so classification
/// survives `?` and is recovered by `downcast_ref` (mixed port-error paths).
#[derive(Debug, thiserror::Error)]
pub enum OperatorEnvError {
    /// Operator env declares reserved runtime names that cannot be overridden.
    #[error(
        "operator env map contains {count} reserved runtime name(s):\n{details}\n\
         These names are fixed by jackin and cannot be overridden. Remove them \
         from your config.toml."
    )]
    ReservedNames {
        /// Number of reserved-name offenses.
        count: usize,
        /// Multi-line detail listing each reserved key and layer.
        details: String,
    },
    /// Value is not an `op://` reference.
    #[error("not an op:// reference: {value}")]
    NotOpRef {
        /// The rejected input value.
        value: String,
    },
    /// Shell variable substitution inside an `op://` URI is unsupported.
    #[error(
        "jackin does not support shell variable substitution inside `op://` URIs \
         (`{value}`). Use a plain string value, or substitute before passing."
    )]
    ShellVarInRef {
        /// The rejected URI containing shell substitution.
        value: String,
    },
    /// `op://` URI path segment count is not 3 or 4.
    #[error("malformed op:// URI (expected 3 or 4 path segments): {value}")]
    MalformedRef {
        /// The malformed URI.
        value: String,
    },
    /// Named or id vault could not be found.
    #[error("vault not found: {vault:?}")]
    VaultNotFound {
        /// Vault name or id that was not found.
        vault: String,
    },
    /// Item name/id missing from the given vault.
    #[error("item {item:?} not found in vault {vault:?}")]
    ItemNotFound {
        /// Item name or id that was not found.
        item: String,
        /// Vault that was searched.
        vault: String,
    },
    /// Multiple items share the same name; disambiguation required.
    #[error("{count} items named {item:?} in vault {vault:?}. Disambiguate with:\n{suggestions}")]
    AmbiguousItem {
        /// Number of matching items.
        count: usize,
        /// Ambiguous item name.
        item: String,
        /// Vault containing the matches.
        vault: String,
        /// Human-readable disambiguation hints.
        suggestions: String,
    },
    /// Field label/id missing from the item.
    #[error("field {field:?} not found in item {item:?}")]
    FieldNotFound {
        /// Field that was not found.
        field: String,
        /// Item that was searched.
        item: String,
    },
    /// Resolution aborted due to an underlying port/runner error.
    #[error("operator env resolution aborted: {source}")]
    Aborted {
        /// Underlying error that caused the abort.
        #[source]
        source: anyhow::Error,
    },
    /// Multiple env keys failed resolution; details are aggregated.
    #[error("operator env resolution failed for {count} var(s):\n{summary}")]
    Aggregated {
        /// Number of failed variables.
        count: usize,
        /// Multi-line summary of failures.
        summary: String,
    },
}

/// Reject operator env maps that declare any reserved runtime name.
/// Runs at config-load time so misconfigurations fail before launch.
/// Conflicts across every layer are aggregated into one error.
pub fn validate_reserved_names(config: &AppConfig) -> Result<(), OperatorEnvError> {
    let mut offenses: Vec<String> = Vec::new();
    let mut record = |layer: EnvLayer, env: &std::collections::BTreeMap<String, EnvValue>| {
        for key in env.keys() {
            if jackin_core::env_model::is_reserved(key) {
                offenses.push(format!(
                    "  - {key:?} is reserved by the jackin runtime; declared in {layer}"
                ));
            }
        }
    };

    record(EnvLayer::Global, &config.env);
    for (role_name, role_source) in &config.roles {
        record(EnvLayer::Role(role_name.clone()), &role_source.env);
    }
    for (ws_name, ws) in &config.workspaces {
        record(EnvLayer::Workspace(ws_name.clone()), &ws.env);
        for (role_name, override_) in &ws.roles {
            record(
                EnvLayer::WorkspaceRole {
                    workspace: ws_name.clone(),
                    role: role_name.clone(),
                },
                &override_.env,
            );
        }
    }

    if offenses.is_empty() {
        return Ok(());
    }

    Err(OperatorEnvError::ReservedNames {
        count: offenses.len(),
        details: offenses.join("\n"),
    })
}

/// Resolve a user-supplied `op://...` URI into a canonical [`OpRef`].
///
/// Accepts all official 1Password URI forms: names, UUIDs, mixed, with
/// optional subtitle filter `Item[subtitle]`, optional 4th section segment,
/// and optional query suffix (`?attribute=otp` etc.). Errors on ambiguity,
/// missing items or fields, or unsupported `${VAR}` substitution syntax.
///
/// The caller must probe `op` CLI availability before calling this
/// (e.g. via [`OpRunner::probe`]).
///
/// `account` pins every underlying `op` query (`vault list`, `item
/// list`, `item get`) to a specific 1Password account. Required when
/// the operator runs more than one signed-in account: a name-based
/// `op://...` reference can otherwise resolve a coincidentally-named
/// item from the default account instead of the intended one. Pass
/// `None` when the call has no account context (e.g. ambient
/// `op://...` resolution where the operator has not pinned an
/// account).
#[expect(
    clippy::too_many_lines,
    reason = "URI parsing + multi-protocol op:// resolution; split would obscure the single algorithm"
)]
pub fn resolve_op_uri_to_ref(
    input: &str,
    op: &dyn OpStructRunner,
    account: Option<&str>,
) -> anyhow::Result<OpRef> {
    // Port errors from OpStructRunner stay as raw anyhow; validation failures
    // are typed sources so pickers can downcast without substring matching.
    if !input.starts_with("op://") {
        return Err(anyhow::Error::new(OperatorEnvError::NotOpRef {
            value: input.to_owned(),
        }));
    }
    if input.contains("${") {
        return Err(anyhow::Error::new(OperatorEnvError::ShellVarInRef {
            value: input.to_owned(),
        }));
    }

    // Peel off optional `?attribute=...` / `?attr=...` / `?ssh-format=...` suffix.
    let (path_part, query) = input
        .find('?')
        .map_or((input, None), |i| (&input[..i], Some(&input[i..])));
    let Some(body) = path_part.strip_prefix("op://") else {
        return Err(anyhow::Error::new(OperatorEnvError::NotOpRef {
            value: input.to_owned(),
        }));
    };
    let segs: Vec<&str> = body.split('/').collect();
    let (vault_seg, item_seg, section_seg, field_seg) = match segs.as_slice() {
        [v, i, f] => (*v, *i, None::<&str>, *f),
        [v, i, s, f] => (*v, *i, Some(*s), *f),
        _ => {
            return Err(anyhow::Error::new(OperatorEnvError::MalformedRef {
                value: input.to_owned(),
            }));
        }
    };

    // Item segment may carry [subtitle] filter — a display extension from jackin❯.
    // Nested condition makes map_or awkward; allow the if-let pattern here.
    #[allow(clippy::option_if_let_else, reason = "documented residual allow; prefer expect when site is lint-true")]
    let (item_name, subtitle_filter): (&str, Option<&str>) = if let Some(open) = item_seg.rfind('[')
    {
        if item_seg.ends_with(']') && open < item_seg.len() - 1 {
            (
                &item_seg[..open],
                Some(&item_seg[open + 1..item_seg.len() - 1]),
            )
        } else {
            (item_seg, None)
        }
    } else {
        (item_seg, None)
    };

    // Resolve vault by name (case-insensitive) or UUID.
    let vaults = op.vault_list(account)?;
    let vault = vaults
        .iter()
        .find(|v| v.name.eq_ignore_ascii_case(vault_seg) || v.id == vault_seg)
        .ok_or_else(|| {
            anyhow::Error::new(OperatorEnvError::VaultNotFound {
                vault: vault_seg.to_owned(),
            })
        })?;

    // Resolve items in this vault, then filter by name (case-insensitive) or
    // UUID, and by subtitle filter when present.
    let items = op.item_list(&vault.id, account)?;
    let mut matches: Vec<&OpItem> = items
        .iter()
        .filter(|i| {
            let name_match = i.name.eq_ignore_ascii_case(item_name) || i.id == item_name;
            let subtitle_match = match subtitle_filter {
                None => true,
                // `#<prefix>` → match against item ID prefix (from disambig suggestion).
                Some(s) if s.starts_with('#') => i.id.starts_with(&s[1..]),
                Some(s) => i.subtitle.eq_ignore_ascii_case(s),
            };
            name_match && subtitle_match
        })
        .collect();

    if matches.is_empty() {
        let suffix = subtitle_filter
            .map(|s| format!("[{s}]"))
            .unwrap_or_default();
        return Err(anyhow::Error::new(OperatorEnvError::ItemNotFound {
            item: format!("{item_name}{suffix}"),
            vault: vault.name.clone(),
        }));
    }
    if matches.len() > 1 {
        let suggestions: Vec<String> = matches
            .iter()
            .map(|i| {
                let label = if i.subtitle.is_empty() {
                    let id_prefix: String = i.id.chars().take(8).collect();
                    format!("{}[#{}]", i.name, id_prefix)
                } else {
                    format!("{}[{}]", i.name, i.subtitle)
                };
                let section_part = section_seg.map(|s| format!("/{s}")).unwrap_or_default();
                let q = query.unwrap_or("");
                format!("  op://{}/{label}{section_part}/{field_seg}{q}", vault.name)
            })
            .collect();
        return Err(anyhow::Error::new(OperatorEnvError::AmbiguousItem {
            count: matches.len(),
            item: item_name.to_owned(),
            vault: vault.name.clone(),
            suggestions: suggestions.join("\n"),
        }));
    }
    let Some(item) = matches.pop() else {
        return Err(anyhow::Error::new(OperatorEnvError::ItemNotFound {
            item: item_name.to_owned(),
            vault: vault.name.clone(),
        }));
    };

    // Resolve field by label (case-insensitive) or UUID.
    let fields = op.item_get(&item.id, &vault.id, account)?;
    let field = fields
        .iter()
        .find(|f| f.label.eq_ignore_ascii_case(field_seg) || f.id == field_seg)
        .ok_or_else(|| {
            anyhow::Error::new(OperatorEnvError::FieldNotFound {
                field: field_seg.to_owned(),
                item: item.name.clone(),
            })
        })?;

    // Compute ambiguity for path snapshot (same rule as picker).
    let item_name_collides = items.iter().any(|i| i.id != item.id && i.name == item.name);
    let safe_to_embed = !item.name.contains('[') && !item.name.contains(']');
    let item_segment = if item_name_collides && safe_to_embed && !item.subtitle.is_empty() {
        format!("{}[{}]", item.name, item.subtitle)
    } else {
        item.name.clone()
    };

    // Use field.reference (1Password's canonical emission) as the authoritative
    // source for the section segment, mirroring build_op_ref_on_commit.
    let section_from_field = parse_op_reference(&field.reference).and_then(|p| p.section);

    let canonical_section = match (section_seg, section_from_field) {
        // field.reference has a section: use canonical (1Password) form
        // regardless of whether the user also typed a section. This covers:
        //   - (Some(_), Some(s)): both present → prefer field.reference's form.
        //   - (None, Some(s)): 3-segment input but field lives in a section;
        //     pick it up so the result matches the picker's output.
        (_, Some(s)) => Some(s),
        // User typed a section but the field's reference has none — should not
        // happen for sectioned fields; trust the user input as a fallback.
        (Some(user_s), None) => Some(user_s.to_owned()),
        // No section anywhere: 3-segment URI.
        (None, None) => None,
    };

    // Mirror picker's empty-label fallback: use field.id when label is empty.
    let field_label = if field.label.is_empty() {
        field.id.as_str()
    } else {
        field.label.as_str()
    };

    let q_suffix = query.unwrap_or("");
    let (op_uri, display_path) = canonical_section.as_deref().map_or_else(
        || {
            (
                format!("op://{}/{}/{}{q_suffix}", vault.id, item.id, field.id),
                format!("{}/{}/{}{q_suffix}", vault.name, item_segment, field_label),
            )
        },
        |s| {
            (
                format!("op://{}/{}/{}/{}{q_suffix}", vault.id, item.id, s, field.id),
                format!(
                    "{}/{}/{}/{}{q_suffix}",
                    vault.name, item_segment, s, field_label
                ),
            )
        },
    );

    Ok(OpRef {
        op: op_uri,
        path: display_path,
        account: account.map(str::to_owned),
        on_demand: false,
    })
}

fn record_layer(
    attributed: &mut std::collections::BTreeMap<String, (EnvLayer, EnvValue)>,
    layer: &EnvLayer,
    env: &std::collections::BTreeMap<String, EnvValue>,
) {
    for (k, v) in env {
        attributed.insert(k.clone(), (layer.clone(), v.clone()));
    }
}

/// Build the (key → (layer, value)) attribution map by walking the
/// four config layers in precedence order — global, role, workspace,
/// workspace-role — for the given `(role, workspace)` selection.
/// Later layers overwrite earlier ones, so the final layer attached
/// to each key is the one that wins resolution.
fn build_attributed_layers(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
) -> std::collections::BTreeMap<String, (EnvLayer, EnvValue)> {
    let mut attributed: std::collections::BTreeMap<String, (EnvLayer, EnvValue)> =
        std::collections::BTreeMap::new();

    record_layer(&mut attributed, &EnvLayer::Global, &config.env);
    if let Some(role_name) = role_selector
        && let Some(a) = config.roles.get(role_name)
    {
        record_layer(
            &mut attributed,
            &EnvLayer::Role(role_name.to_owned()),
            &a.env,
        );
    }
    if let Some(ws_name) = workspace_name
        && let Some(ws) = config.workspaces.get(ws_name.as_str())
    {
        record_layer(
            &mut attributed,
            &EnvLayer::Workspace(ws_name.as_str().to_owned()),
            &ws.env,
        );
        if let Some(role_name) = role_selector
            && let Some(ov) = ws.roles.get(role_name)
        {
            let ws_role_layer = EnvLayer::WorkspaceRole {
                workspace: ws_name.as_str().to_owned(),
                role: role_name.to_owned(),
            };
            record_layer(&mut attributed, &ws_role_layer, &ov.env);
        }
    }

    attributed
}

/// Return whether any operator-env declaration applies to the given
/// `(role, workspace)` pair, without resolving the values.
pub fn has_operator_env(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
) -> bool {
    !build_attributed_layers(config, role_selector, workspace_name).is_empty()
}

/// Return whether any operator-env declaration that matches `include_key`
/// applies to the given `(role, workspace)` pair, without resolving values.
pub fn has_operator_env_matching<F>(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
    include_key: F,
) -> bool
where
    F: Fn(&str) -> bool,
{
    build_attributed_layers(config, role_selector, workspace_name)
        .keys()
        .any(|key| include_key(key))
}

/// Look up the raw (unresolved) declaration value for `key` in the
/// operator env config layers, using the same precedence as
/// `resolve_operator_env` (global < role < workspace < workspace-role).
pub fn lookup_operator_env_raw(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
    key: &str,
) -> Option<String> {
    build_attributed_layers(config, role_selector, workspace_name)
        .remove(key)
        .map(|(_, value)| value.as_display_str().to_owned())
}

/// Env var Claude Code reads for the long-lived OAuth token.
///
/// Centralised so token-setup, launch diagnostics, and agent
/// `required_env_var` stay in sync. See
/// <https://code.claude.com/docs/en/iam> for upstream precedence
/// semantics.
pub const CLAUDE_OAUTH_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// Walk the env layers for the given `(role, workspace)` pair and
/// resolve every value. Resolution failures across layers are
/// aggregated into one error.
pub fn resolve_operator_env(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    // Each `op://` ref pins its own account at read time
    // (`OpRef::account`), so the runner carries no instance-level account.
    let runner = OpCli::new_launch_env();
    resolve_operator_env_with(config, role_selector, workspace_name, &runner, |name| {
        std::env::var(name)
    })
}

/// Walk the env layers for the given `(role, workspace)` pair and resolve only
/// keys accepted by `include_key`. Resolution failures across included keys are
/// aggregated into one error.
pub fn resolve_operator_env_matching<F>(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
    include_key: F,
) -> anyhow::Result<std::collections::BTreeMap<String, String>>
where
    F: Fn(&str) -> bool,
{
    let runner = OpCli::new_launch_env();
    resolve_operator_env_with_matching(
        config,
        role_selector,
        workspace_name,
        &runner,
        |name| std::env::var(name),
        include_key,
    )
}

/// Collect the on-demand credential bindings for a `(role, workspace)`
/// selection — every env entry flagged `on_demand`, with the `(name, kind,
/// source)` triple the host credential resolver needs. `kind` is `"op"`
/// (resolve via `op read <source>`), `"env"` (read host env named by the
/// `$VAR` source), or `"literal"` (return the source verbatim).
///
/// These are exactly the values [`resolve_operator_env_with_matching`] drops
/// from launch-time injection: they are resolved later, at `jackin-exec` time,
/// after the operator approves them in the picker. Returned sorted by name.
#[must_use]
pub fn collect_on_demand_bindings(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
) -> Vec<jackin_protocol::ExecBinding> {
    // BTreeMap iteration is already ordered by key, so the result is sorted.
    build_attributed_layers(config, role_selector, workspace_name)
        .into_iter()
        .filter(|(_, (_, v))| v.is_on_demand())
        .map(|(name, (_, value))| {
            use jackin_protocol::ExecKind;
            let (kind, source) = match value {
                EnvValue::OpRef(r) => (ExecKind::Op, r.op),
                EnvValue::Extended(e) => {
                    if parse_host_ref(&e.value).is_some() {
                        (ExecKind::Env, e.value)
                    } else {
                        (ExecKind::Literal, e.value)
                    }
                }
                // Plain is never on_demand, so the filter excludes it; map
                // defensively as a literal rather than panicking.
                EnvValue::Plain(s) => (ExecKind::Literal, s),
            };
            jackin_protocol::ExecBinding { name, kind, source }
        })
        .collect()
}

/// `?Sized` so callers can pass `&dyn OpRunner` (used by
/// `LoadOptions::op_runner` in `src/runtime/launch.rs`).
pub fn resolve_operator_env_with<R, H>(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
    op_runner: &R,
    host_env: H,
) -> anyhow::Result<std::collections::BTreeMap<String, String>>
where
    R: OpRunner + ?Sized,
    H: Fn(&str) -> Result<String, std::env::VarError> + Send + Sync,
{
    resolve_operator_env_with_matching(
        config,
        role_selector,
        workspace_name,
        op_runner,
        host_env,
        |_| true,
    )
}

/// `?Sized` so callers can pass `&dyn OpRunner` (used by
/// `LoadOptions::op_runner` in `src/runtime/launch.rs`).
pub fn resolve_operator_env_with_matching<R, H, F>(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
    op_runner: &R,
    host_env: H,
    include_key: F,
) -> anyhow::Result<std::collections::BTreeMap<String, String>>
where
    R: OpRunner + ?Sized,
    H: Fn(&str) -> Result<String, std::env::VarError> + Send + Sync,
    F: Fn(&str) -> bool,
{
    let mut attributed = build_attributed_layers(config, role_selector, workspace_name);
    attributed.retain(|key, _| include_key(key));
    // On-demand credentials are never resolved at launch — that would run
    // `op read` (and a Touch ID prompt) for a value the agent should only get
    // through the operator picker at `jackin-exec` time. Drop them here so they
    // are not injected into the container env; the names are surfaced separately
    // via `collect_on_demand_bindings` for the host resolver.
    attributed.retain(|_, (_, v)| !v.is_on_demand());

    let mut resolved = std::collections::BTreeMap::new();
    let mut errors: Vec<String> = Vec::new();

    // Probe op CLI once up front when any value is an OpRef, so a
    // missing op surfaces as one install-link error not N.
    let uses_op = attributed
        .values()
        .any(|(_, v)| matches!(v, EnvValue::OpRef(_)));
    if uses_op && let Err(e) = op_runner.probe() {
        return Err(anyhow::Error::new(OperatorEnvError::Aborted { source: e }));
    }

    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(attributed.len());
        for (key, (layer, value)) in &attributed {
            let host_env = &host_env;
            handles.push(scope.spawn(move || {
                let layer_label = format!("{layer}");
                let timing_name = format!("operator_env:{key}");
                let value_kind = ValueKind::of_env_value(value).as_timing_detail();
                jackin_diagnostics::active_timing_started(
                    "credentials",
                    &timing_name,
                    Some(value_kind),
                );
                let result =
                    resolve_env_value(&layer_label, key, value, op_runner, |name| host_env(name));
                match result {
                    Ok(value) => {
                        jackin_diagnostics::active_timing_done(
                            "credentials",
                            &timing_name,
                            Some(value_kind),
                        );
                        (key.clone(), Ok(value))
                    }
                    Err(error) => {
                        jackin_diagnostics::active_timing_done(
                            "credentials",
                            &timing_name,
                            Some("error"),
                        );
                        (key.clone(), Err(error))
                    }
                }
            }));
        }

        for handle in handles {
            match handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            {
                (key, Ok(value)) => {
                    resolved.insert(key, value);
                }
                (_, Err(error)) => errors.push(format!("  - {error}")),
            }
        }
    });

    if errors.is_empty() {
        return Ok(resolved);
    }

    Err(anyhow::Error::new(OperatorEnvError::Aggregated {
        count: errors.len(),
        summary: errors.join("\n"),
    }))
}

/// Print a launch diagnostic to stderr. Values are NEVER printed —
/// normal mode is counts only, debug mode is reference strings or the
/// `literal` placeholder; the layer that supplied each key is shown.
pub fn print_launch_diagnostic(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) {
    let mut out = Vec::new();
    let _unused = write_launch_diagnostic(
        &mut out,
        config,
        role_selector,
        workspace_name,
        resolved,
        debug,
    );
    emit_launch_diagnostic(
        std::str::from_utf8(&out).unwrap_or(""),
        debug,
        &mut std::io::stderr(),
    );
}

pub(crate) fn emit_launch_diagnostic<W: std::io::Write>(
    rendered: &str,
    debug: bool,
    stderr: &mut W,
) {
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact("operator_env", rendered.trim_end());
    }
    if debug || jackin_diagnostics::rich_terminal_owned() {
        return;
    }
    drop(stderr.write_all(rendered.as_bytes()));
}

#[cfg(test)]
#[expect(
    dead_code,
    reason = "diagnostic formatter is used by selected test builds"
)]
pub(crate) fn format_launch_diagnostic_for_test(
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) -> String {
    let mut out = Vec::new();
    write_launch_diagnostic(
        &mut out,
        config,
        role_selector,
        workspace_name,
        resolved,
        debug,
    )
    .unwrap();
    String::from_utf8(out).unwrap()
}

fn write_launch_diagnostic<W: std::io::Write>(
    w: &mut W,
    config: &AppConfig,
    role_selector: Option<&str>,
    workspace_name: Option<&WorkspaceName>,
    resolved: &std::collections::BTreeMap<String, String>,
    debug: bool,
) -> std::io::Result<()> {
    let mut attributed = build_attributed_layers(config, role_selector, workspace_name);
    // Drop keys not in `resolved` — those failed to dispatch.
    attributed.retain(|k, _| resolved.contains_key(k));

    if debug {
        writeln!(w, "[jackin] operator env:")?;
        let key_width = attributed
            .keys()
            .map(String::len)
            .max()
            .unwrap_or(0)
            .min(40);
        let raw_width = attributed
            .values()
            .map(|(_, v)| classify_env_value(v).len())
            .max()
            .unwrap_or(0)
            .min(40);
        for (key, (layer, value)) in &attributed {
            let kind = classify_env_value(value);
            writeln!(w, "  {key:key_width$}  {kind:raw_width$}  ({layer})")?;
        }
        return Ok(());
    }

    let (mut op_count, mut host_count, mut literal_count) = (0u32, 0u32, 0u32);
    for (_, value) in attributed.values() {
        match ValueKind::of_env_value(value) {
            ValueKind::Op => op_count += 1,
            ValueKind::Host => host_count += 1,
            ValueKind::Literal => literal_count += 1,
        }
    }
    writeln!(
        w,
        "[jackin] operator env: {} resolved ({} op://, {} host ref, {} literal)",
        attributed.len(),
        op_count,
        host_count,
        literal_count
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum ValueKind {
    Op,
    Host,
    Literal,
}

impl ValueKind {
    fn of_env_value(value: &EnvValue) -> Self {
        match value {
            EnvValue::OpRef(_) => Self::Op,
            // An Extended value carries a literal or `$VAR` string, same as
            // Plain — classify it by whether it is a host ref.
            EnvValue::Plain(s) => Self::of_str(s),
            EnvValue::Extended(e) => Self::of_str(&e.value),
        }
    }

    fn of_str(s: &str) -> Self {
        if parse_host_ref(s).is_some() {
            Self::Host
        } else {
            Self::Literal
        }
    }

    const fn as_timing_detail(self) -> &'static str {
        match self {
            Self::Op => "op",
            Self::Host => "host",
            Self::Literal => "literal",
        }
    }
}

/// Value-free label: `OpRef` emits the canonical `op://` URI; `$NAME`
/// host refs are returned verbatim; literals collapse to `"literal"` so
/// the value never reaches stderr.
fn classify_env_value(value: &EnvValue) -> String {
    match value {
        EnvValue::OpRef(r) => r.op.clone(),
        EnvValue::Plain(s) => classify_str(s),
        EnvValue::Extended(e) => classify_str(&e.value),
    }
}

/// `$NAME` host refs are returned verbatim; literals collapse to `"literal"`
/// so the value never reaches stderr.
fn classify_str(s: &str) -> String {
    if parse_host_ref(s).is_some() {
        s.to_owned()
    } else {
        "literal".to_owned()
    }
}

#[cfg(test)]
mod tests;
