use jackin_core::WorkspaceName;
use jackin_config::AppConfig;

/// What we found in a single env layer when looking up the credential
/// var required by an `auth_forward` mode.
///
/// Carried inside `LaunchError::AuthCredentialMissing` so both CLI text
/// rendering and TUI structured rendering can reuse the same trace
/// without re-deriving it from the resolved env map.
///
/// All three variants are constructed today: `Unset` by both
/// `verify_credential_env_present`'s tests and `build_env_layer_states`
/// when a layer is silent; `ResolvedLiteral` / `ResolvedOpRef` by
/// `build_env_layer_states` when a layer declares the var.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnvLayerState {
    /// Layer does not declare the var at all.
    Unset,
    /// Layer declares the var with a literal (or `$VAR`) value that
    /// resolved to a non-empty string.
    ResolvedLiteral,
    /// Layer declares the var with an `op://...` reference that
    /// resolved to a non-empty string.
    ResolvedOpRef,
}

impl std::fmt::Display for EnvLayerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unset => write!(f, "unset"),
            Self::ResolvedLiteral => write!(f, "resolved (literal)"),
            Self::ResolvedOpRef => write!(f, "resolved (op://...)"),
        }
    }
}

/// Errors produced by launch-time validation that benefit from
/// structured fields (e.g. TUI rendering, multi-line CLI output) rather
/// than the stringy `anyhow::bail!` shape used elsewhere in this file.
///
/// Today this enum carries a single variant — the auth-credential
/// pre-flight failure — but it's defined as an enum so that future
/// launch-time validators (`DinD` readiness, image build preconditions,
/// etc.) can grow structured variants alongside it without churning the
/// type at every call site.
//
// Constructed by Task 13's `verify_credential_env_present` and bubbled
// through Task 14's `load_role_with` integration.
#[derive(Debug, thiserror::Error)]
pub(crate) enum LaunchError {
    /// `auth_forward` mode requires a credential env var to resolve to
    /// a non-empty value, but the resolved operator env doesn't carry
    /// it. Carries enough structure for both CLI rendering (multi-line
    /// text via the `Display` impl) and TUI rendering (structured
    /// panel) to reuse the same data without re-deriving it.
    #[error("{}", render_auth_credential_missing(
        *.agent,
        *.mode,
        .env_var,
        .workspace,
        .role,
        .mode_resolution,
        .env_layers,
    ))]
    AuthCredentialMissing {
        /// Agent the launch was for (drives the var name and remediation copy).
        agent: jackin_core::agent::Agent,
        /// Resolved `auth_forward` mode that requires the credential.
        mode: jackin_config::AuthForwardMode,
        /// Well-known credential env var (e.g. `ANTHROPIC_API_KEY`,
        /// `CLAUDE_CODE_OAUTH_TOKEN`, `OPENAI_API_KEY`, `AMP_API_KEY`) that must
        /// resolve to a non-empty value for `mode`.
        env_var: &'static str,
        /// Workspace name the launch targets (for messaging).
        workspace: String,
        /// Role selector key the launch targets (for messaging).
        role: String,
        /// Trace of the 3-layer mode resolution: each entry pairs a
        /// human-readable layer label (e.g. `"workspace × role × claude"`)
        /// with the mode value declared at that layer (`None` = layer
        /// is silent). Layers are ordered most-specific first.
        mode_resolution: Vec<(String, Option<jackin_config::AuthForwardMode>)>,
        /// Trace of the env-layer resolution for `env_var`: each entry
        /// pairs a TOML-table label (e.g. `"[workspaces.proj.env]"`)
        /// with what we found in that layer. Layers are ordered
        /// lowest-to-highest priority so the rendered output reads
        /// chronologically the same way operators read TOML.
        env_layers: Vec<(String, EnvLayerState)>,
    },
}

/// Constant gutter between the layer-label column and the `->` arrow
/// in `render_auth_credential_missing` output. Sized so even the longest
/// label has visible whitespace before the arrow (matches the spec test
/// fixture `workspace × role × claude    -> api_key`).
const RENDER_LABEL_GUTTER: usize = 4;

/// Cap on the layer-label column width. Keeps a pathologically-long
/// label (60+ chars) from blowing up line width while still
/// comfortably fitting any realistic env-table path.
const RENDER_LABEL_WIDTH_CAP: usize = 60;

/// Compute the padded column width used for the layer-label column in
/// `render_auth_credential_missing`. Pulled out so both the
/// mode-resolution and env-layer sections share the same arithmetic
/// without repeating the gutter / cap constants inline.
fn render_label_width<T>(rows: &[(String, T)]) -> usize {
    rows.iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0)
        .saturating_add(RENDER_LABEL_GUTTER)
        .min(RENDER_LABEL_WIDTH_CAP)
}

/// Render the structured multi-line `AuthCredentialMissing` message
/// for CLI display. The TUI panel consumes the structured fields
/// directly and ignores this rendering — they intentionally share the
/// data, not the formatting.
fn render_auth_credential_missing(
    agent: jackin_core::agent::Agent,
    mode: jackin_config::AuthForwardMode,
    env_var: &str,
    workspace: &str,
    role: &str,
    mode_resolution: &[(String, Option<jackin_config::AuthForwardMode>)],
    env_layers: &[(String, EnvLayerState)],
) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();

    let _unused = writeln!(
        out,
        "cannot launch {agent} in workspace '{workspace}' role '{role}'"
    );
    let _unused = writeln!(
        out,
        "       \u{2014} auth_forward is '{mode}', which requires {env_var}"
    );
    let _unused = writeln!(
        out,
        "         to resolve to a non-empty value, but it is unset."
    );

    if !mode_resolution.is_empty() {
        let _unused = writeln!(out);
        let _unused = writeln!(out, "  Effective auth resolution:");
        let label_width = render_label_width(mode_resolution);
        for (idx, (label, value)) in mode_resolution.iter().enumerate() {
            let value_str = value
                .as_ref()
                .map_or_else(|| "(none)".to_owned(), ToString::to_string);
            let suffix = if idx == 0 { "  (most-specific)" } else { "" };
            let _unused = writeln!(out, "    {label:<label_width$}-> {value_str}{suffix}");
        }
    }

    if !env_layers.is_empty() {
        let _unused = writeln!(out);
        let _unused = writeln!(
            out,
            "  Env layer resolution for {env_var} (lowest -> highest):"
        );
        let label_width = render_label_width(env_layers);
        for (label, state) in env_layers {
            let _unused = writeln!(out, "    {label:<label_width$}-> {state}");
        }
    }

    let agent_title = agent.runtime().label();

    let _unused = writeln!(out);
    let _unused = writeln!(out, "  Fix one of:");
    let _unused = writeln!(
        out,
        "    - Open the Auth panel:  jackin tui workspaces  \u{2192} '{workspace}' \u{2192} Auth \u{2192} {role} / {agent_title}"
    );
    // `jackin config env set` does not yet support `--workspace`; show
    // the role-scoped form (the closest existing remediation) so we
    // don't print a flag the operator can't actually use today.
    let _unused = writeln!(
        out,
        "    - Or by hand:           jackin config env set {env_var} <value> --role {role}"
    );
    let _unused = writeln!(
        out,
        "    - Or change the mode:   set auth_forward = 'sync' at one of the layers above"
    );

    // Trim the trailing newline left by the final `writeln!` so callers
    // composing this into larger errors don't get an awkward extra blank
    // line.
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Build the 3-layer mode-resolution trace (most-specific first) that
/// `LaunchError::AuthCredentialMissing` carries for rendering. Walks
/// the same layers as [`jackin_config::resolve_mode`] but records each
/// layer's value (or `None` when silent) so the operator can see at a
/// glance which TOML layer wins.
pub(super) fn build_mode_resolution(
    cfg: &AppConfig,
    agent: jackin_core::agent::Agent,
    workspace: &str,
    role: &str,
) -> Vec<(String, Option<jackin_config::AuthForwardMode>)> {
    jackin_config::resolve_mode_with_trace(cfg, agent, workspace, role).1
}

/// Build the 4-layer env-layer trace (lowest precedence first) for the
/// credential var. Layers mirror `operator_env::build_attributed_layers`:
/// `[env]` < `[roles.<role>.env]` < `[workspaces.<ws>.env]` <
/// `[workspaces.<ws>.roles.<role>.env]`. Each entry records whether the
/// layer declared the var as a literal, an `op://...` reference, or
/// not at all.
pub(super) fn build_env_layer_states(
    cfg: &AppConfig,
    workspace: &str,
    role: &str,
    env_var: &str,
) -> Vec<(String, EnvLayerState)> {
    const fn classify(value: &jackin_core::EnvValue) -> EnvLayerState {
        match value {
            jackin_core::EnvValue::Plain(_) => EnvLayerState::ResolvedLiteral,
            jackin_core::EnvValue::Extended(_) => EnvLayerState::ResolvedLiteral,
            jackin_core::EnvValue::OpRef(_) => EnvLayerState::ResolvedOpRef,
        }
    }
    let global = cfg.env.get(env_var).map_or(EnvLayerState::Unset, classify);
    let role_global = cfg
        .roles
        .get(role)
        .and_then(|r| r.env.get(env_var))
        .map_or(EnvLayerState::Unset, classify);
    let workspace_global = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.env.get(env_var))
        .map_or(EnvLayerState::Unset, classify);
    let workspace_role = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.roles.get(role))
        .and_then(|ro| ro.env.get(env_var))
        .map_or(EnvLayerState::Unset, classify);
    vec![
        ("[env]".to_owned(), global),
        (format!("[roles.{role}.env]"), role_global),
        (format!("[workspaces.{workspace}.env]"), workspace_global),
        (
            format!("[workspaces.{workspace}.roles.{role}.env]"),
            workspace_role,
        ),
    ]
}

/// Append `KEY=value` to `env_strings` when `value` is `Some` and
/// non-empty. Centralizes the "skip the env push when the value is
/// missing or blank" check used by every optional env injection.
pub(super) fn push_env_if_present(env_strings: &mut Vec<String>, key: &str, value: Option<&str>) {
    if let Some(v) = value
        && !v.is_empty()
    {
        env_strings.push(format!("{key}={v}"));
    }
}

/// Canonical CLI proxy env vars `curl`, `wget`, and Go's HTTP client read.
/// `FTP_PROXY` / `RSYNC_PROXY` are intentionally out of scope: they don't
/// reach `DinD`'s daemon socket, so adding them here would only widen the
/// detection surface without changing bypass behavior.
pub(super) const PROXY_VAR_NAMES: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];
pub(super) const NO_PROXY_UPPER: &str = "NO_PROXY";
pub(super) const NO_PROXY_LOWER: &str = "no_proxy";

pub(super) fn is_proxy_env_name(key: &str) -> bool {
    PROXY_VAR_NAMES.contains(&key)
}

pub(crate) fn append_no_proxy_host(value: &str, host: &str) -> String {
    if value
        .split(',')
        .map(str::trim)
        .any(|entry| entry.eq_ignore_ascii_case(host))
    {
        return value.to_owned();
    }

    if value.trim().is_empty() {
        host.to_owned()
    } else {
        format!("{value},{host}")
    }
}

/// Printable source reference for the credential env var `env_var` (e.g.
/// `"CLAUDE_CODE_OAUTH_TOKEN"`, `"ANTHROPIC_API_KEY"`) given the raw
/// (unresolved) declaration value from the operator env config (e.g.
/// `"Private/Claude/security/auth token"` or `"$CLAUDE_CODE_OAUTH_TOKEN"`).
/// Produces the `"KEY ← value"` form; falls back to the bare env-var name
/// when `raw` is `None` or empty.
pub(super) fn auth_token_source_reference(env_var: &str, raw: Option<&str>) -> String {
    match raw {
        None | Some("") => env_var.to_owned(),
        Some(value) => format!("{env_var} \u{2190} {value}"),
    }
}
