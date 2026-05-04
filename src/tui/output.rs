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

/// One-line yellow deprecation warning to stderr. Used for soft-migration
/// notices like "config field X is deprecated — migrated to Y".
pub fn deprecation_warning(msg: &str) {
    const AMBER: (u8, u8, u8) = (230, 180, 80);
    eprintln!(
        "  {} {}",
        "warning:".color(rgb(AMBER)).bold(),
        msg.color(rgb(AMBER)),
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
/// Shapes:
///   claude auth: host session (sync)
///   claude auth: none (ignore — /login required inside the container)
///   claude auth: OAuth token (`CLAUDE_CODE_OAUTH_TOKEN` ← <source-reference>)
///
/// `source_reference` is consulted only by the `token` arm; pass the
/// resolver's source description for the `CLAUDE_CODE_OAUTH_TOKEN`
/// entry (e.g. `"op://vault/claude/token"` or
/// `"$CLAUDE_CODE_OAUTH_TOKEN"`). Other modes pass `None`.
pub fn auth_mode_notice(mode: &str, source_reference: Option<&str>) {
    eprintln!(
        "  {}",
        format_auth_mode_notice_for_test(mode, source_reference)
    );
}

/// Pure formatter extracted for unit-testing the exact output text.
/// Returns the rendered line with ANSI color codes included.
fn format_auth_mode_notice_for_test(mode: &str, source_reference: Option<&str>) -> String {
    let label = "claude auth:".color(rgb(PHOSPHOR_GREEN)).bold().to_string();
    let body = match mode {
        "token" => {
            let src = source_reference.unwrap_or("CLAUDE_CODE_OAUTH_TOKEN");
            format!("OAuth token ({src})")
        }
        "sync" => "host session (sync)".to_string(),
        "ignore" => "none (ignore — /login required inside the container)".to_string(),
        other => other.to_string(),
    };
    format!("{label} {}", body.color(rgb(PHOSPHOR_DIM)))
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
            (M::Sync | M::Token, O::Skipped) => unreachable!(
                "AuthProvisionOutcome::Skipped should only be returned for AuthForwardMode::Ignore"
            ),
        }
    }
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
    fn auth_mode_notice_token_mentions_source_reference() {
        let line = format_auth_mode_notice_for_test(
            "token",
            Some("CLAUDE_CODE_OAUTH_TOKEN ← op://vault/claude/token"),
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
    fn auth_mode_notice_sync_has_one_liner() {
        let clean = strip_ansi(&format_auth_mode_notice_for_test("sync", None));
        assert!(clean.contains("claude auth:"));
        assert!(clean.contains("host session"));
        assert!(clean.contains("sync"));
    }

    #[test]
    fn auth_mode_notice_ignore_has_one_liner() {
        let clean = strip_ansi(&format_auth_mode_notice_for_test("ignore", None));
        assert!(clean.contains("claude auth:"));
        assert!(clean.contains("none"));
        assert!(clean.contains("ignore"));
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
            (M::Token, O::TokenMode, CodexSyncState::TokenMode),
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
