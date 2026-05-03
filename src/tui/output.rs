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
}
