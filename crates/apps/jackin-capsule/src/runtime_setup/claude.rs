// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    AuthMaterialization, AuthMode, ForwardedCredential, apply_forwarded_credential,
    claude_account_path, claude_config_dir, claude_credentials_path, copy_file_with_mode,
    env_is_one, forwarded_file, remove_file_if_exists, run_optional_command,
    seed_agent_home_from_enum,
};

use std::fs;

use anyhow::Result;

use jackin_core::container_paths;

pub(crate) fn setup_claude(mode: AuthMode) -> Result<AuthMaterialization> {
    let home = claude_config_dir();
    let first_seed = seed_agent_home_from_enum(jackin_core::Agent::Claude, &home)?.is_first_seed();
    let credentials_path = claude_credentials_path();
    let forwarded = forwarded_file(container_paths::CLAUDE_CREDENTIALS);
    let materialization = apply_forwarded_credential(
        first_seed,
        mode,
        &ForwardedCredential {
            label: "claude",
            forwarded: &forwarded,
            target: &credentials_path,
            api_key_envs: &[
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_AUTH_TOKEN",
                "CLAUDE_CODE_OAUTH_TOKEN",
            ],
        },
    )?;
    if matches!(mode, AuthMode::Sync) {
        seed_claude_account_json(first_seed)?;
    } else {
        remove_file_if_exists(claude_account_path())?;
    }

    if env_is_one("JACKIN_DISABLE_TIRITH") {
        crate::output::stdout_line(format_args!(
            "[entrypoint] tirith disabled (JACKIN_DISABLE_TIRITH=1)"
        ));
    } else {
        run_optional_command(
            "claude",
            &["mcp", "add", "tirith", "--", "tirith", "mcp-server"],
        );
    }
    if env_is_one("JACKIN_DISABLE_SHELLFIRM") {
        crate::output::stdout_line(format_args!(
            "[entrypoint] shellfirm disabled (JACKIN_DISABLE_SHELLFIRM=1)"
        ));
    } else {
        run_optional_command(
            "claude",
            &["mcp", "add", "shellfirm", "--", "shellfirm", "mcp"],
        );
    }
    if std::env::var_os("JACKIN_EXEC_BINDINGS").is_some_and(|v| !v.is_empty()) {
        run_optional_command(
            "claude",
            &[
                "mcp",
                "add",
                "jackin-exec",
                "--",
                "jackin-capsule",
                "mcp-server",
            ],
        );
    }
    setup_claude_plugins();
    Ok(materialization)
}

pub(crate) fn seed_claude_account_json(first_seed: bool) -> Result<()> {
    let forwarded_account = forwarded_file(container_paths::CLAUDE_ACCOUNT);
    if !forwarded_account.is_file() {
        return Ok(());
    }
    let account_path = claude_account_path();
    let needs_seed = first_seed
        || fs::read_to_string(&account_path).map_or(true, |contents| contents.trim() == "{}");
    if needs_seed {
        copy_file_with_mode(&forwarded_account, &account_path, 0o600)?;
    }
    Ok(())
}

pub(crate) fn setup_claude_plugins() {
    let Some(config) = crate::config::load_optional() else {
        return;
    };
    if config.claude_marketplaces.is_empty() && config.claude_plugins.is_empty() {
        return;
    }
    let config_dir = claude_config_dir();
    let marker = config_dir.join(".jackin-plugins.done");
    let fingerprint = claude_plugin_fingerprint(&config);
    if fs::read_to_string(&marker).is_ok_and(|s| s == fingerprint) {
        return;
    }
    run_optional_command(
        "claude",
        &[
            "plugin",
            "marketplace",
            "add",
            "anthropics/claude-plugins-official",
        ],
    );
    let mut all_ok = true;
    for marketplace in &config.claude_marketplaces {
        let mut args = vec!["plugin", "marketplace", "add", marketplace.source.as_str()];
        if !marketplace.sparse.is_empty() {
            args.push("--sparse");
            args.extend(marketplace.sparse.iter().map(String::as_str));
        }
        all_ok &= run_optional_command("claude", &args);
    }
    for plugin in &config.claude_plugins {
        all_ok &= run_optional_command("claude", &["plugin", "install", plugin.as_str()]);
    }
    if !all_ok {
        crate::output::stderr_line(format_args!(
            "[entrypoint] claude plugins: one or more installs failed; marker not written, will retry on next launch"
        ));
        return;
    }
    if let Err(e) = fs::create_dir_all(&config_dir) {
        crate::output::stderr_line(format_args!(
            "[entrypoint] claude plugins: failed to create marker dir {}: {e} (plugins will re-run next launch)",
            config_dir.display()
        ));
        return;
    }
    if let Err(e) = fs::write(&marker, &fingerprint) {
        crate::output::stderr_line(format_args!(
            "[entrypoint] claude plugins: failed to write install marker (plugins will re-run next launch): {e}"
        ));
    }
}

pub(crate) fn claude_plugin_fingerprint(config: &jackin_protocol::CapsuleConfig) -> String {
    let mut out = String::new();
    for marketplace in &config.claude_marketplaces {
        out.push_str("m:");
        out.push_str(&marketplace.source);
        for path in &marketplace.sparse {
            out.push(' ');
            out.push_str(path);
        }
        out.push('\n');
    }
    for plugin in &config.claude_plugins {
        out.push_str("p:");
        out.push_str(plugin);
        out.push('\n');
    }
    out
}
