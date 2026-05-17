use owo_colors::OwoColorize;
use std::io::{self, Write};
use std::sync::atomic::Ordering;

use super::{DEBUG_MODE, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, rgb};

// ── Color palette ────────────────────────────────────────────────────────

const ROSE: (u8, u8, u8) = (210, 100, 100);

// ── Config table ─────────────────────────────────────────────────────────

pub fn print_config_table(rows: &[(String, String)]) {
    let label_w = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    let value_w = rows.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
    let inner_w = label_w + 3 + value_w;

    let dim = rgb(PHOSPHOR_DARK);
    let gold = rgb(PHOSPHOR_GREEN);
    let powder = rgb(PHOSPHOR_DIM);

    eprintln!(
        "  {}{}{}",
        "\u{250c}".color(dim),
        "\u{2500}".repeat(inner_w + 2).color(dim),
        "\u{2510}".color(dim),
    );

    for (label, value) in rows {
        let pad_l = label_w - label.len();
        let pad_r = value_w - value.len();
        eprintln!(
            "  {} {}{} {} {}{}{}",
            "\u{2502}".color(dim),
            " ".repeat(pad_l),
            label.color(gold),
            "\u{2502}".color(dim),
            value.color(powder),
            " ".repeat(pad_r),
            " \u{2502}".to_string().color(dim),
        );
    }

    eprintln!(
        "  {}{}{}",
        "\u{2514}".color(dim),
        "\u{2500}".repeat(inner_w + 2).color(dim),
        "\u{2518}".color(dim),
    );
}

// ── Step shimmer ─────────────────────────────────────────────────────────

pub fn step_shimmer(n: u32, text: &str) {
    let prefix = format!("  {n:>2}.  ");
    let chars: Vec<char> = text.chars().collect();
    let frames = chars.len() + 6;

    let mg = rgb(PHOSPHOR_GREEN);

    for frame in 0..frames {
        eprint!("\r");
        eprint!("{}", prefix.color(mg).bold());
        for (i, ch) in chars.iter().enumerate() {
            let dist = (frame as i32 - i as i32).abs();
            let color = if dist == 0 {
                WHITE
            } else if dist == 1 {
                (150, 255, 170)
            } else if dist == 2 {
                PHOSPHOR_GREEN
            } else {
                PHOSPHOR_DIM
            };
            eprint!("{}", ch.color(rgb(color)).bold());
        }
        let _ = io::stderr().flush();
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    eprint!("\r");
    eprint!("{}", prefix.color(mg).bold());
    eprintln!("{}", text.color(rgb(PHOSPHOR_DIM)).bold());
}

/// Minimal step message without animation (used in `--no-intro` mode).
pub fn step_quiet(n: u32, text: &str) {
    let prefix = format!("  {n:>2}.  ");
    let mg = rgb(PHOSPHOR_GREEN);
    eprintln!(
        "{}{}",
        prefix.color(mg).bold(),
        text.color(rgb(PHOSPHOR_DIM)).bold()
    );
}

pub fn step_fail(msg: &str) {
    eprintln!("       {}", msg.color(rgb(ROSE)));
}

// ── Deploying message ────────────────────────────────────────────────────

pub fn print_deploying(role_name: &str) {
    eprintln!();
    eprintln!(
        "  {}",
        format!("Deploying {role_name} into an isolated container...")
            .color(rgb(PHOSPHOR_GREEN))
            .bold()
    );
    eprintln!();

    std::thread::sleep(std::time::Duration::from_millis(1500));
    clear_screen();
}

// ── Logo ─────────────────────────────────────────────────────────────

pub fn print_logo(logo_path: &std::path::Path) {
    let contents = match std::fs::read_to_string(logo_path) {
        Ok(c) if !c.trim().is_empty() => c,
        _ => return,
    };

    eprintln!();
    for line in contents.lines() {
        eprintln!("  {}", line.color(rgb(PHOSPHOR_GREEN)));
    }
    eprintln!();
}

// ── Utility ──────────────────────────────────────────────────────────────

pub fn fatal(msg: &str) {
    eprintln!();
    eprintln!(
        "  {} {}",
        "error:".color(rgb(ROSE)).bold(),
        msg.color(rgb(ROSE)),
    );
}

pub fn set_terminal_title(title: &str) {
    eprint!("\x1b]0;jackin' \u{00b7} {title}\x07");
    let _ = io::stderr().flush();
}

pub fn clear_screen() {
    if DEBUG_MODE.load(Ordering::Relaxed) {
        return;
    }
    eprint!("\x1b[2J\x1b[H");
    let _ = io::stderr().flush();
}

/// Replace the user's home directory prefix with `~/` for shorter display paths.
pub fn shorten_home(path: &str) -> String {
    if let Some(home) = directories::BaseDirs::new().map(|b| b.home_dir().display().to_string()) {
        if path == home {
            return "~".to_string();
        }
        if let Some(rest) = path.strip_prefix(&home)
            && rest.starts_with('/')
        {
            return format!("~{rest}");
        }
    }
    path.to_string()
}

/// Print a hint line with a highlighted command.
pub fn hint(prefix: &str, command: &str, suffix: &str) {
    println!(
        "{prefix}{}{suffix}",
        command.color(rgb(PHOSPHOR_GREEN)).bold(),
    );
}

/// Render the one-line launch diagnostic for the active auth mode.
///
/// Shapes (label varies by agent — `<agent> auth:`):
///   claude auth: host session (sync)
///   claude auth: none (ignore — /login required inside the container)
///   claude auth: OAuth token (`CLAUDE_CODE_OAUTH_TOKEN` — expires in 12 days)
///   amp auth: API key (`AMP_API_KEY` ← <source-reference>)
///
/// `source_reference` is consulted only by the env-driven token/api-key
/// arms; pass the resolver's source description for the relevant env
/// entry (e.g. `"op://vault/claude/token"` or
/// `"$CLAUDE_CODE_OAUTH_TOKEN"`). Other modes pass `None`.
///
/// `expiry_days` is consulted only by the `oauth_token` arm. Negative
/// values render as `expired`, `<= 7` as `expires in N days` (red),
/// `<= 30` as yellow, otherwise dimmed. Pass `None` when no expiry is
/// known (e.g. `--reuse` path or cache miss).
pub fn auth_mode_notice(
    agent: crate::agent::Agent,
    mode: &str,
    source_reference: Option<&str>,
    expiry_days: Option<i64>,
) {
    eprintln!(
        "  {}",
        format_auth_mode_notice_for_test(agent, mode, source_reference, expiry_days)
    );
}

/// Pure formatter extracted for unit-testing the exact output text.
/// Returns the rendered line with ANSI color codes included.
fn format_auth_mode_notice_for_test(
    agent: crate::agent::Agent,
    mode: &str,
    source_reference: Option<&str>,
    expiry_days: Option<i64>,
) -> String {
    use crate::config::AuthForwardMode;
    let label_text = format!("{} auth:", agent.slug());
    let label = label_text.color(rgb(PHOSPHOR_GREEN)).bold().to_string();
    let env_default_for = |m: AuthForwardMode| agent.required_env_var(m).unwrap_or("");
    let body = match mode {
        "oauth_token" => {
            let src =
                source_reference.unwrap_or_else(|| env_default_for(AuthForwardMode::OAuthToken));
            let suffix = expiry_days.map(format_expiry_suffix).unwrap_or_default();
            format!("OAuth token ({src}{suffix})")
        }
        "api_key" => {
            let src = source_reference.unwrap_or_else(|| env_default_for(AuthForwardMode::ApiKey));
            format!("API key ({src})")
        }
        "sync" => "host session (sync)".to_string(),
        "ignore" => "none (ignore — /login required inside the container)".to_string(),
        other => other.to_string(),
    };
    format!("{label} {}", body.color(rgb(PHOSPHOR_DIM)))
}

fn format_expiry_suffix(days: i64) -> String {
    use owo_colors::{AnsiColors, OwoColorize};
    let (text, color) = match days {
        d if d < 0 => (
            format!("expired {} day(s) ago", d.unsigned_abs()),
            AnsiColors::Red,
        ),
        0 => ("expires today".to_string(), AnsiColors::Red),
        d if d <= 7 => (format!("expires in {d} day(s)"), AnsiColors::Red),
        d if d <= 30 => (format!("expires in {d} day(s)"), AnsiColors::Yellow),
        d => (format!("expires in {d} day(s)"), AnsiColors::Default),
    };
    if matches!(color, AnsiColors::Default) {
        format!(" — {}", text.dimmed())
    } else {
        format!(" — {}", text.color(color))
    }
}

/// Verbose outcome eprintln line that follows [`auth_mode_notice`] for
/// the file-driven agents (Claude, Amp). Codex uses [`codex_auth_notice`]
/// instead.
pub fn agent_outcome_notice(
    agent: crate::agent::Agent,
    auth_mode: crate::config::AuthForwardMode,
    auth_outcome: crate::instance::AuthProvisionOutcome,
) {
    use crate::agent::Agent as A;
    use crate::config::AuthForwardMode as M;
    use crate::instance::AuthProvisionOutcome as O;

    let display = match agent {
        A::Claude => "Claude Code",
        A::Codex => "Codex",
        A::Amp => "Amp",
        A::Kimi => "Kimi",
        A::Opencode => "OpenCode",
    };
    match auth_outcome {
        O::Synced => {
            eprintln!(
                "[jackin] Synced host {display} authentication into role state \
                 (auth_forward=sync)."
            );
        }
        O::TokenMode => {
            if let Some(env_var) = agent.required_env_var(auth_mode) {
                eprintln!(
                    "[jackin] auth_forward={auth_mode} — role will use \
                     {env_var} from the resolved env."
                );
            }
        }
        O::HostMissing => {
            if matches!(auth_mode, M::Sync) {
                let host_file = match agent {
                    A::Claude => "credentials",
                    A::Codex => "auth.json",
                    A::Amp => "secrets.json",
                    A::Kimi => "credentials",
                    A::Opencode => "config.json",
                };
                eprintln!(
                    "[jackin] auth_forward=sync but no host {display} {host_file} found; \
                     preserving existing container auth if present."
                );
            }
        }
        O::Skipped => {
            // Only Amp emits a Skipped breadcrumb today; Claude/Codex
            // are silent. The asymmetry is intentional — Amp's wipe
            // path is the only one where the operator's next launch
            // surfaces interactive-login prompts.
            if matches!(agent, A::Amp) {
                eprintln!(
                    "[jackin] auth_forward=ignore — wiped any prior synced \
                     secrets.json; agent will require interactive login."
                );
            }
        }
    }
}

/// Sync-state half of the Codex auth notice.
///
/// Derived from `(AuthForwardMode, AuthProvisionOutcome)` via [`From`]
/// so a future outcome variant forces a compile error here instead of
/// a silent fallback to a misleading notice.
#[derive(Debug, Clone, Copy)]
pub enum CodexSyncState {
    Synced,
    HostMissing,
    TokenMode,
    Ignored,
}

impl
    From<(
        crate::config::AuthForwardMode,
        crate::instance::AuthProvisionOutcome,
    )> for CodexSyncState
{
    fn from(
        (mode, outcome): (
            crate::config::AuthForwardMode,
            crate::instance::AuthProvisionOutcome,
        ),
    ) -> Self {
        use crate::config::AuthForwardMode as M;
        use crate::instance::AuthProvisionOutcome as O;
        match (mode, outcome) {
            (_, O::Synced) => Self::Synced,
            (_, O::HostMissing) => Self::HostMissing,
            (_, O::TokenMode) => Self::TokenMode,
            (M::Ignore, O::Skipped) => Self::Ignored,
            // `Skipped` is only emitted by the Ignore arm of
            // `provision_codex_auth`; the Sync/Token arms always return
            // Synced/HostMissing/TokenMode. If a future outcome variant
            // is added, these arms become reachable and the `unreachable!`
            // will fire in tests/debug — preferable to silently routing
            // a new outcome to the wrong notice.
            (M::Sync | M::OAuthToken | M::ApiKey, O::Skipped) => unreachable!(
                "AuthProvisionOutcome::Skipped should only be returned for AuthForwardMode::Ignore"
            ),
        }
    }
}

/// One-line GitHub CLI auth state diagnostic.
///
/// Body is derived from `outcome`: each variant carries its own
/// attribution — `Synced` names the source (gh CLI vs `hosts.yml`),
/// `HostMissing` names the reason (logged out vs gh failed vs file
/// malformed), and `TokenMode` shows the operator-env breadcrumb.
pub fn github_auth_notice(
    outcome: &crate::instance::GithubProvisionOutcome,
    token_breadcrumb: Option<&str>,
) {
    eprintln!(
        "  {}",
        format_github_auth_notice_for_test(outcome, token_breadcrumb)
    );
}

fn format_github_auth_notice_for_test(
    outcome: &crate::instance::GithubProvisionOutcome,
    token_breadcrumb: Option<&str>,
) -> String {
    use crate::instance::{GithubProvisionOutcome as O, GithubTokenSource, HostMissingReason};
    let label = "gh auth:".color(rgb(PHOSPHOR_GREEN)).bold().to_string();
    let body = match outcome {
        O::Synced { source, .. } => {
            let via = match source {
                GithubTokenSource::GhCli => "via gh CLI",
                GithubTokenSource::HostsFile => "via ~/.config/gh/hosts.yml",
            };
            format!("forwarded host token (sync, {via})")
        }
        O::HostMissing { reason } => match reason {
            HostMissingReason::NoGhAndNoHostsFile => {
                "none — host has no gh login, container preserves prior login".to_string()
            }
            HostMissingReason::GhCliFailed { stderr } => {
                let trimmed = stderr.lines().next().unwrap_or("").trim();
                if trimmed.is_empty() {
                    "none — gh auth token failed (run with --debug for stderr); \
                     container preserves prior login"
                        .to_string()
                } else {
                    format!(
                        "none — gh auth token failed: {trimmed}; \
                         container preserves prior login"
                    )
                }
            }
            HostMissingReason::GhCliEmpty => {
                "none — gh auth token returned empty (broken host login); \
                 container preserves prior login"
                    .to_string()
            }
            HostMissingReason::HostsFileMalformed => {
                "none — ~/.config/gh/hosts.yml present but unparseable; \
                 container preserves prior login"
                    .to_string()
            }
        },
        O::TokenMode { .. } => {
            let src = token_breadcrumb.unwrap_or(crate::env_model::GH_TOKEN_ENV_NAME);
            format!("scoped token from {src} (token)")
        }
        O::Skipped => "disabled (ignore)".to_string(),
    };
    format!("{label} {}", body.color(rgb(PHOSPHOR_DIM)))
}

/// Render the one-line launch diagnostic for Codex auth state.
///
/// `api_key_source` wins display when present — Codex CLI prefers
/// `OPENAI_API_KEY` over `auth.json` (verified against the Codex CLI
/// source as of 2026-05; if that precedence ever changes upstream the
/// `codex_auth_notice_api_key_wins_over_sync_state` test will need to
/// be revisited rather than silently rotting in prose).
pub fn codex_auth_notice(api_key_source: Option<&str>, sync_state: CodexSyncState) {
    eprintln!(
        "  {}",
        format_codex_auth_notice_for_test(api_key_source, sync_state)
    );
}

fn format_codex_auth_notice_for_test(
    api_key_source: Option<&str>,
    sync_state: CodexSyncState,
) -> String {
    let label = "codex auth:".color(rgb(PHOSPHOR_GREEN)).bold().to_string();
    let body = api_key_source.map_or_else(
        || match sync_state {
            CodexSyncState::Synced => "synced (~/.codex/auth.json)".to_string(),
            CodexSyncState::HostMissing => {
                "none — Codex CLI will prompt for ChatGPT login inside the container".to_string()
            }
            CodexSyncState::TokenMode => {
                "none (token mode is a no-op for Codex — set OPENAI_API_KEY or auth_forward = \"sync\")".to_string()
            }
            CodexSyncState::Ignored => {
                "none (ignore — login required inside the container)".to_string()
            }
        },
        |src| format!("OPENAI_API_KEY ({src})"),
    );
    format!("{label} {}", body.color(rgb(PHOSPHOR_DIM)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strip ANSI escape sequences so assertions match plain text.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                for inner in chars.by_ref() {
                    if inner.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                result.push(ch);
            }
        }
        result
    }

    #[test]
    fn auth_mode_notice_oauth_token_mentions_source_reference() {
        let line = format_auth_mode_notice_for_test(
            crate::agent::Agent::Claude,
            "oauth_token",
            Some("CLAUDE_CODE_OAUTH_TOKEN ← op://vault/claude/token"),
            None::<i64>,
        );
        let clean = strip_ansi(&line);
        assert!(clean.contains("claude auth:"), "got: {clean}");
        assert!(clean.contains("OAuth token"), "got: {clean}");
        assert!(
            clean.contains("CLAUDE_CODE_OAUTH_TOKEN ← op://vault/claude/token"),
            "got: {clean}"
        );
    }

    #[test]
    fn auth_mode_notice_oauth_token_appends_expiry_days_when_present() {
        let line = format_auth_mode_notice_for_test(
            crate::agent::Agent::Claude,
            "oauth_token",
            Some("CLAUDE_CODE_OAUTH_TOKEN ← op://Personal/Claude/token"),
            Some(12),
        );
        let clean = strip_ansi(&line);
        assert!(clean.contains("expires in 12 day(s)"), "got: {clean}");
    }

    #[test]
    fn auth_mode_notice_oauth_token_renders_expired_when_negative() {
        let line = format_auth_mode_notice_for_test(
            crate::agent::Agent::Claude,
            "oauth_token",
            None,
            Some(-3),
        );
        let clean = strip_ansi(&line);
        assert!(clean.contains("expired 3 day(s) ago"), "got: {clean}");
    }

    #[test]
    fn auth_mode_notice_oauth_token_renders_expires_today_at_zero() {
        let line = format_auth_mode_notice_for_test(
            crate::agent::Agent::Claude,
            "oauth_token",
            None,
            Some(0),
        );
        let clean = strip_ansi(&line);
        assert!(clean.contains("expires today"), "got: {clean}");
    }

    #[test]
    fn auth_mode_notice_sync_has_one_liner() {
        let clean = strip_ansi(&format_auth_mode_notice_for_test(
            crate::agent::Agent::Claude,
            "sync",
            None,
            None,
        ));
        assert!(clean.contains("claude auth:"));
        assert!(clean.contains("host session"));
        assert!(clean.contains("sync"));
    }

    #[test]
    fn auth_mode_notice_ignore_has_one_liner() {
        let clean = strip_ansi(&format_auth_mode_notice_for_test(
            crate::agent::Agent::Claude,
            "ignore",
            None,
            None,
        ));
        assert!(clean.contains("claude auth:"));
        assert!(clean.contains("none"));
        assert!(clean.contains("ignore"));
    }

    #[test]
    fn auth_mode_notice_uses_agent_specific_label_and_env_var_for_amp() {
        let line =
            format_auth_mode_notice_for_test(crate::agent::Agent::Amp, "api_key", None, None);
        let clean = strip_ansi(&line);
        assert!(clean.contains("amp auth:"), "got: {clean}");
        assert!(clean.contains("API key"), "got: {clean}");
        assert!(clean.contains("AMP_API_KEY"), "got: {clean}");
        assert!(!clean.contains("ANTHROPIC_API_KEY"), "got: {clean}");
    }

    #[test]
    fn codex_auth_notice_api_key_wins_over_sync_state() {
        let clean = strip_ansi(&format_codex_auth_notice_for_test(
            Some("op://Work/OpenAI/default"),
            CodexSyncState::Synced,
        ));
        assert!(clean.contains("codex auth:"), "got: {clean}");
        assert!(clean.contains("OPENAI_API_KEY"), "got: {clean}");
        assert!(clean.contains("op://Work/OpenAI/default"), "got: {clean}");
        assert!(!clean.contains("synced"), "api key should win: {clean}");
    }

    #[test]
    fn codex_auth_notice_synced_mentions_auth_json_source() {
        let clean = strip_ansi(&format_codex_auth_notice_for_test(
            None,
            CodexSyncState::Synced,
        ));
        assert!(clean.contains("codex auth:"));
        assert!(clean.contains("synced"));
        assert!(clean.contains("~/.codex/auth.json"));
    }

    #[test]
    fn codex_auth_notice_host_missing_describes_chatgpt_login_fallback() {
        let clean = strip_ansi(&format_codex_auth_notice_for_test(
            None,
            CodexSyncState::HostMissing,
        ));
        assert!(clean.contains("codex auth:"));
        assert!(clean.contains("none"));
        assert!(clean.contains("ChatGPT login"));
    }

    #[test]
    fn codex_auth_notice_token_mode_steers_operator_to_sync_or_env() {
        let clean = strip_ansi(&format_codex_auth_notice_for_test(
            None,
            CodexSyncState::TokenMode,
        ));
        assert!(clean.contains("token mode"));
        assert!(clean.contains("no-op"));
        assert!(clean.contains("OPENAI_API_KEY"));
        assert!(clean.contains("sync"));
    }

    #[test]
    fn codex_auth_notice_ignored_says_login_required() {
        let clean = strip_ansi(&format_codex_auth_notice_for_test(
            None,
            CodexSyncState::Ignored,
        ));
        assert!(clean.contains("ignore"));
        assert!(clean.contains("login required"));
    }

    // ── github_auth_notice formatter ─────────────────────────────

    #[test]
    fn github_auth_notice_synced_via_gh_cli_mentions_source() {
        use crate::instance::{GithubProvisionOutcome, GithubTokenSource};
        let outcome = GithubProvisionOutcome::Synced {
            token: "ghp_test".into(),
            source: GithubTokenSource::GhCli,
        };
        let clean = strip_ansi(&format_github_auth_notice_for_test(&outcome, None));
        assert!(clean.contains("gh auth:"), "got: {clean}");
        assert!(clean.contains("forwarded host token"), "got: {clean}");
        assert!(clean.contains("via gh CLI"), "got: {clean}");
        // Token must not appear verbatim — formatter never echoes the
        // resolved value into the notice.
        assert!(!clean.contains("ghp_test"), "token leaked: {clean}");
    }

    #[test]
    fn github_auth_notice_synced_via_hosts_file_mentions_source() {
        use crate::instance::{GithubProvisionOutcome, GithubTokenSource};
        let outcome = GithubProvisionOutcome::Synced {
            token: "ghp_test".into(),
            source: GithubTokenSource::HostsFile,
        };
        let clean = strip_ansi(&format_github_auth_notice_for_test(&outcome, None));
        assert!(clean.contains("via ~/.config/gh/hosts.yml"), "got: {clean}");
    }

    #[test]
    fn github_auth_notice_token_mode_uses_breadcrumb() {
        use crate::instance::GithubProvisionOutcome;
        let outcome = GithubProvisionOutcome::TokenMode {
            token: "ghp_secret".into(),
        };
        let clean = strip_ansi(&format_github_auth_notice_for_test(
            &outcome,
            Some("GH_TOKEN ← op://Work/ACME/gh-pat"),
        ));
        assert!(clean.contains("scoped token"), "got: {clean}");
        assert!(
            clean.contains("op://Work/ACME/gh-pat"),
            "breadcrumb missing: {clean}"
        );
        assert!(clean.contains("(token)"), "got: {clean}");
        assert!(!clean.contains("ghp_secret"), "token leaked: {clean}");
    }

    #[test]
    fn github_auth_notice_token_mode_falls_back_to_bare_env_var_name() {
        use crate::instance::GithubProvisionOutcome;
        let outcome = GithubProvisionOutcome::TokenMode {
            token: "ghp_secret".into(),
        };
        let clean = strip_ansi(&format_github_auth_notice_for_test(&outcome, None));
        // Without a breadcrumb, formatter renders the bare env-var
        // name so the operator at least knows the source key.
        assert!(clean.contains("GH_TOKEN"), "got: {clean}");
    }

    #[test]
    fn github_auth_notice_host_missing_renders_each_typed_reason() {
        use crate::instance::{GithubProvisionOutcome, HostMissingReason};

        let cases: &[(HostMissingReason, &[&str])] = &[
            (
                HostMissingReason::NoGhAndNoHostsFile,
                &["host has no gh login", "preserves prior login"],
            ),
            (
                HostMissingReason::GhCliFailed {
                    stderr: "you must run `gh auth login`".into(),
                },
                &["gh auth token failed", "gh auth login"],
            ),
            (HostMissingReason::GhCliEmpty, &["returned empty"]),
            (HostMissingReason::HostsFileMalformed, &["unparseable"]),
        ];
        for (reason, needles) in cases {
            let outcome = GithubProvisionOutcome::HostMissing {
                reason: reason.clone(),
            };
            let clean = strip_ansi(&format_github_auth_notice_for_test(&outcome, None));
            for needle in *needles {
                assert!(
                    clean.contains(needle),
                    "reason {reason:?}: missing needle {needle:?} in {clean}",
                );
            }
        }
    }

    #[test]
    fn github_auth_notice_skipped_renders_disabled() {
        use crate::instance::GithubProvisionOutcome;
        let clean = strip_ansi(&format_github_auth_notice_for_test(
            &GithubProvisionOutcome::Skipped,
            None,
        ));
        assert!(clean.contains("disabled"), "got: {clean}");
        assert!(clean.contains("(ignore)"), "got: {clean}");
    }

    /// Pin the (`AuthForwardMode`, `AuthProvisionOutcome`) → `CodexSyncState`
    /// table so a future outcome variant or mode addition has to be wired
    /// here explicitly instead of silently falling through to a misleading
    /// notice. Mirrors the unreachable arm in the `From` impl.
    #[test]
    fn codex_sync_state_from_mode_outcome_table() {
        use crate::config::AuthForwardMode as M;
        use crate::instance::AuthProvisionOutcome as O;

        let cases = [
            (M::Sync, O::Synced, CodexSyncState::Synced),
            (M::Sync, O::HostMissing, CodexSyncState::HostMissing),
            (M::OAuthToken, O::TokenMode, CodexSyncState::TokenMode),
            (M::ApiKey, O::TokenMode, CodexSyncState::TokenMode),
            (M::Ignore, O::Skipped, CodexSyncState::Ignored),
        ];
        for (mode, outcome, expected) in cases {
            let got: CodexSyncState = (mode, outcome).into();
            assert!(
                std::mem::discriminant(&got) == std::mem::discriminant(&expected),
                "({mode:?}, {outcome:?}) → {got:?}, expected {expected:?}"
            );
        }
    }
}
