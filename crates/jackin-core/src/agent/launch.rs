// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Exhaustive launch policy shared by the image's generated shell dispatch.

use super::Agent;

impl Agent {
    /// Native CLI executable and fixed launch arguments. New enum variants must
    /// supply a launch policy; the shell has no independently maintained list.
    pub const fn launch_argv(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &[
                "claude",
                "--settings",
                "{\"skipDangerousModePermissionPrompt\":true}",
                "--dangerously-skip-permissions",
                "--verbose",
            ],
            Self::Codex => &[
                "codex",
                "--enable",
                "goals",
                "--dangerously-bypass-approvals-and-sandbox",
            ],
            Self::Amp => &["amp", "--dangerously-allow-all"],
            Self::Kimi => &["kimi", "--yolo"],
            Self::Opencode => &["opencode"],
            Self::Grok => &["grok", "--always-approve"],
            Self::Antigravity => &["agy"],
            Self::Gemini => &["gemini"],
            Self::Cursor => &["cursor-agent"],
            Self::Muse => &["muse"],
            Self::Omp => &["omp"],
            Self::Hermes => &["hermes"],
        }
    }

    /// Render final argv in the entrypoint shell, before hooks mutate its env.
    /// Amp retains its existing argument policy. Other agents receive caller
    /// arguments without evaluation or splitting.
    pub fn launch_dispatch_shell() -> String {
        let mut shell = String::from("case \"${JACKIN_AGENT:?JACKIN_AGENT must be set}\" in\n");
        for agent in Self::ALL {
            shell.push_str(&format!("  {})\n", agent.slug()));
            if *agent == Self::Opencode {
                shell.push_str("    export OPENCODE_CONFIG_CONTENT='{\"permission\":\"allow\"}'\n");
            }
            shell.push_str("    LAUNCH=(");
            shell.push_str(
                &agent
                    .launch_argv()
                    .iter()
                    .map(|arg| shell_word(arg))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            shell.push_str(")\n");
            if *agent == Self::Claude {
                shell.push_str("    if [ -n \"${JACKIN_EXEC_SYSTEM_PROMPT:-}\" ]; then\n        LAUNCH+=(--system-prompt \"${JACKIN_EXEC_SYSTEM_PROMPT}\")\n    fi\n");
            }
            if *agent != Self::Amp {
                shell.push_str("    if [ \"$#\" -gt 0 ]; then\n        LAUNCH+=(\"$@\")\n    fi\n");
            }
            shell.push_str("    ;;\n");
        }
        shell.push_str("  *)\n    echo \"[entrypoint] unknown JACKIN_AGENT: $JACKIN_AGENT\" >&2\n    exit 2\n    ;;\nesac");
        shell
    }
}

fn shell_word(word: &str) -> String {
    if word
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"_-./".contains(&byte))
    {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}
