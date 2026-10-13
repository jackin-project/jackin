// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn load_args_parses_agent_flag() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith", "--agent", "codex"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            agent: Some(jackin_core::Agent::Codex),
            ..
        }))
    ));
}

#[test]
fn load_args_parses_amp_agent_flag() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith", "--agent", "amp"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            agent: Some(jackin_core::Agent::Amp),
            ..
        }))
    ));
}

#[test]
fn load_args_rejects_unknown_agent() {
    let res = Cli::try_parse_from(["jackin", "load", "agent-smith", "--agent", "foo"]);
    res.unwrap_err();
}

#[test]
fn load_args_agent_optional() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            agent: None,
            model: None,
            effort: None,
            ..
        }))
    ));
}

#[test]
fn load_args_parses_task_scoped_model_and_effort() {
    let cli = Cli::try_parse_from([
        "jackin",
        "load",
        "agent-smith",
        "--agent",
        "codex",
        "--model",
        "gpt-6-luna",
        "--effort",
        "max",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            model: Some(ref model),
            effort: Some(jackin_core::ReasoningEffort::Max),
            ..
        })) if model == "gpt-6-luna"
    ));
}

#[test]
fn load_args_rejects_unknown_effort() {
    let error =
        Cli::try_parse_from(["jackin", "load", "agent-smith", "--effort", "maximum"]).unwrap_err();
    let message = strip_ansi(&error.to_string());
    assert!(message.contains("low, medium, high, max"), "{message}");
}

#[test]
fn load_args_parses_model_and_effort_overrides() {
    let cli = Cli::try_parse_from([
        "jackin",
        "load",
        "agent-smith",
        "--agent",
        "codex",
        "--model",
        "  provider/model-id  ",
        "--effort",
        "high",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            model: Some(ref model),
            effort: Some(jackin_core::ReasoningEffort::High),
            ..
        })) if model == "provider/model-id"
    ));
}

#[test]
fn load_args_rejects_empty_model_and_unknown_effort() {
    for args in [
        vec!["jackin", "load", "agent-smith", "--model", "  "],
        vec!["jackin", "load", "agent-smith", "--effort", "extreme"],
    ] {
        Cli::try_parse_from(args).expect_err("invalid launch override should be rejected");
    }
}

#[test]
fn load_args_parses_branch_flag() {
    let cli = Cli::try_parse_from([
        "jackin",
        "load",
        "the-architect",
        "--role-branch",
        "feat/my-pr",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            role_branch: Some(ref b),
            ..
        })) if b == "feat/my-pr"
    ));
}

#[test]
fn load_args_branch_optional() {
    let cli = Cli::try_parse_from(["jackin", "load", "the-architect"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            role_branch: None,
            ..
        }))
    ));
}

#[test]
fn parses_load_command() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith"]).unwrap();
    // `debug` is omitted from the pattern: it is env-backed
    // (`JACKIN_DEBUG`), so its default depends on the runner's env.
    // `tests/cli_debug_env.rs` covers the env-driven behavior.
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            selector: Some(ref s),
            target: None,
            ..
        })) if s == "agent-smith"
    ));
}

#[test]
fn parses_load_without_selector() {
    let cli = Cli::try_parse_from(["jackin", "load"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            selector: None,
            target: None,
            ..
        }))
    ));
}

#[test]
fn parses_load_rebuild_without_selector() {
    let cli = Cli::try_parse_from(["jackin", "load", "--rebuild"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            selector: None,
            rebuild: true,
            ..
        }))
    ));
}

#[test]
fn parses_load_with_target_path() {
    let cli = Cli::try_parse_from(["jackin", "load", "agent-smith", "~/Projects/my-app"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            target: Some(ref t),
            ..
        })) if t == "~/Projects/my-app"
    ));
}

#[test]
fn parses_load_with_target_and_mount() {
    let cli = Cli::try_parse_from([
        "jackin",
        "load",
        "agent-smith",
        "big-monorepo",
        "--mount",
        "/tmp/cache:/workspace/cache:ro",
    ])
    .unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            target: Some(ref t),
            ref mounts,
            ..
        })) if t == "big-monorepo" && mounts.len() == 1
    ));
}

#[test]
fn parses_load_with_mount_only() {
    let cli = Cli::try_parse_from([
        "jackin",
        "load",
        "agent-smith",
        "--mount",
        "/tmp/project:/workspace/project",
        "--mount",
        "/tmp/cache:/workspace/cache:ro",
    ])
    .unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Load(LoadArgs {
            target: None,
            ref mounts,
            ..
        })) if mounts.len() == 2
    ));
}

#[test]
fn parses_console_command() {
    let cli = Cli::try_parse_from(["jackin", "console"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Console(ConsoleArgs { .. }))
    ));
}

#[test]
fn parses_console_with_debug() {
    let cli = Cli::try_parse_from(["jackin", "console", "--debug"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Console(ConsoleArgs { .. }))
    ));
    // --debug is global on Cli, not on ConsoleArgs.
    assert!(cli.debug);
}

#[test]
fn console_rejects_removed_flags() {
    // The console is always the full experience; the old --no-rain /
    // --no-tui / --intro / --outro toggles no longer exist.
    for flag in ["--no-rain", "--no-tui", "--intro", "--outro"] {
        Cli::try_parse_from(["jackin", "console", flag])
            .expect_err(&format!("console should reject {flag}"));
    }
}

#[test]
fn load_rejects_removed_surface_flags() {
    for flag in ["--no-rain", "--no-tui", "--no-intro"] {
        Cli::try_parse_from(["jackin", "load", flag])
            .expect_err(&format!("load should reject {flag}"));
    }
}

#[test]
fn parses_bare_jackin_as_no_subcommand() {
    let cli = Cli::try_parse_from(["jackin"]).unwrap();
    assert!(cli.command.is_none());
}

#[test]
fn parses_bare_jackin_with_top_level_debug() {
    let cli = Cli::try_parse_from(["jackin", "--debug"]).unwrap();
    assert!(cli.command.is_none());
    // CLI flag wins over env, so this assertion holds even when
    // `JACKIN_DEBUG=0` is set in the runner's env.
    assert!(cli.debug);
}

#[test]
fn load_help_shows_description_and_examples() {
    let help = help_text(&["jackin", "load", "--help"]);
    assert!(help.contains("Jack a role into an isolated container"));
    assert!(help.contains("Examples:"));
    assert!(help.contains("jackin load agent-smith"));
    assert!(help.contains("jackin load agent-smith big-monorepo"));
    assert!(help.contains("--model"));
    assert!(help.contains("--effort"));
    assert!(help.contains("low, medium"));
    assert!(help.contains("max"));
}

#[test]
fn load_help_shows_mount_format() {
    let help = help_text(&["jackin", "load", "--help"]);
    assert!(
        help.contains("path[:ro]") && help.contains("src:dst[:ro]"),
        "mount format missing"
    );
}

#[test]
fn load_help_documents_model_and_effort_overrides() {
    let help = help_text(&["jackin", "load", "--help"]);
    assert!(
        help.contains("--model <MODEL>"),
        "model option missing: {help}"
    );
    assert!(
        help.contains("--effort <EFFORT>"),
        "effort option missing: {help}"
    );
    assert!(help.contains("(low, medium, high, or max)"));
}

#[test]
fn load_help_lists_every_agent_slug() {
    let help = help_text(&["jackin", "load", "--help"]);
    for agent in jackin_core::Agent::ALL {
        assert!(
            help.contains(agent.slug()),
            "load help should list `{}` from Agent::ALL: {help}",
            agent.slug()
        );
    }
}

#[test]
fn hardline_help_shows_examples() {
    let help = help_text(&["jackin", "hardline", "--help"]);
    assert!(help.contains("Reattach to a running role"));
    assert!(help.contains("jackin hardline agent-smith"));
    assert!(
        help.contains("jackin hardline ") && help.contains("auto-detect workspace"),
        "missing no-arg usage in hardline help: {help}"
    );
}

#[test]
fn hardline_help_lists_every_agent_slug() {
    let help = help_text(&["jackin", "hardline", "--help"]);
    for agent in jackin_core::Agent::ALL {
        assert!(
            help.contains(agent.slug()),
            "hardline help should list `{}` from Agent::ALL: {help}",
            agent.slug()
        );
    }
}

#[test]
fn parses_hardline_without_selector() {
    let cli = Cli::try_parse_from(["jackin", "hardline"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(HardlineArgs {
            selector: None,
            inspect: false,
            new: false,
            agent: None,
            shell: false,
        }))
    ));
}

#[test]
fn parses_hardline_with_selector() {
    let cli = Cli::try_parse_from(["jackin", "hardline", "agent-smith"]).unwrap();
    assert!(matches!(
        cli.command,
        Some(Command::Hardline(HardlineArgs {
            selector: Some(ref s),
            inspect: false,
            new: false,
            agent: None,
            shell: false,
        })) if s == "agent-smith"
    ));
}
