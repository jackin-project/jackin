use super::AppConfig;
use crate::paths::JackinPaths;

impl AppConfig {
    pub fn load_or_init(paths: &JackinPaths) -> anyhow::Result<Self> {
        paths.ensure_base_dirs()?;

        let contents_opt = match std::fs::read_to_string(&paths.config_file) {
            Ok(c) => Some(c),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };

        let deprecated_copy_seen = match &contents_opt {
            Some(c) => contains_deprecated_copy_auth_forward(c)?,
            None => false,
        };

        let mut config: Self = match contents_opt {
            Some(c) => toml::from_str(&c)?,
            None => Self::default(),
        };

        let builtins_changed = config.sync_builtin_agents();

        if deprecated_copy_seen {
            crate::tui::deprecation_warning(&format!(
                "migrated auth_forward \"copy\" → \"sync\" in {} (copy is deprecated)",
                paths.config_file.display()
            ));
        }

        if builtins_changed || deprecated_copy_seen {
            // Bootstrap only when the file doesn't exist yet. Without this
            // gate, ConfigEditor::open would call load_or_init for the
            // missing file and recurse. When the file DOES exist (the
            // builtins-drifted or deprecated-copy upgrade path), we must
            // NOT rewrite it through the lossy serde path first — that
            // would destroy every user comment before the editor could
            // preserve them, defeating the whole point of this migration.
            if !paths.config_file.exists() {
                // Inline of the removed AppConfig::save. Atomic write:
                // serialize → .tmp (0o600 on unix, fsync) → rename.
                let contents = toml::to_string_pretty(&config)?;
                let tmp = paths.config_file.with_extension("tmp");

                #[cfg(unix)]
                {
                    use std::io::Write;
                    use std::os::unix::fs::OpenOptionsExt;
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .mode(0o600)
                        .open(&tmp)?;
                    file.write_all(contents.as_bytes())?;
                    file.sync_all()?;
                }

                #[cfg(not(unix))]
                std::fs::write(&tmp, &contents)?;

                std::fs::rename(&tmp, &paths.config_file)?;
            }
            let mut editor = crate::config::ConfigEditor::open(paths)?;
            if builtins_changed {
                for &(name, git) in crate::config::agents::BUILTIN_AGENTS {
                    editor.upsert_builtin_agent(name, git);
                }
            }
            if deprecated_copy_seen {
                editor.normalize_deprecated_copy();
            }
            // editor.save() returns an AppConfig parsed from the on-disk file,
            // which has [agents.X.env] preserved (upsert_builtin_agent doesn't
            // touch env). The in-memory `config` from sync_builtin_agents has
            // env cleared. Replace the in-memory config with the preserved one.
            config = editor.save()?;
        }

        // Reject operator env maps that declare reserved runtime names.
        // Runs at load, before validate_workspaces, so misconfigurations
        // fail fast regardless of which subcommand is about to execute.
        crate::operator_env::validate_reserved_names(&config)?;

        config.validate_workspaces()?;
        Ok(config)
    }
}

/// Detect the literal deprecated `auth_forward = "copy"` at either of the
/// two known config paths: the global `[claude]` table or any
/// `[agents.*.claude]` table. Returns `true` if any occurrence is found.
///
/// Uses `toml::Value` (cheap — we parse the same string into `AppConfig`
/// right after) instead of a regex, so quoted keys with odd whitespace
/// are handled correctly.
fn contains_deprecated_copy_auth_forward(raw: &str) -> anyhow::Result<bool> {
    let value: toml::Value = toml::from_str(raw)?;

    // Global [claude] auth_forward
    if let Some(s) = value
        .get("claude")
        .and_then(|c| c.get("auth_forward"))
        .and_then(|v| v.as_str())
        && s == "copy"
    {
        return Ok(true);
    }

    // Per-agent [agents.<name>.claude] auth_forward
    if let Some(agents) = value.get("agents").and_then(|a| a.as_table()) {
        for agent in agents.values() {
            if let Some(s) = agent
                .get("claude")
                .and_then(|c| c.get("auth_forward"))
                .and_then(|v| v.as_str())
                && s == "copy"
            {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use tempfile::tempdir;

    #[test]
    fn sync_does_not_rewrite_config_when_already_current() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        // First load creates the file
        AppConfig::load_or_init(&paths).unwrap();
        let mtime_before = std::fs::metadata(&paths.config_file)
            .unwrap()
            .modified()
            .unwrap();

        // Small delay so mtime would differ if rewritten
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Second load should not rewrite
        AppConfig::load_or_init(&paths).unwrap();
        let mtime_after = std::fs::metadata(&paths.config_file)
            .unwrap()
            .modified()
            .unwrap();

        assert_eq!(mtime_before, mtime_after);
    }

    #[test]
    fn load_migrates_global_copy_to_sync_and_rewrites_config() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[claude]
auth_forward = "copy"

[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
        )
        .unwrap();

        let config = AppConfig::load_or_init(&paths).unwrap();

        // In memory, Copy normalized to Sync
        assert_eq!(
            config.claude.auth_forward,
            crate::config::AuthForwardMode::Sync
        );

        // On disk, "copy" no longer present
        let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(
            !persisted.contains("auth_forward = \"copy\""),
            "expected on-disk config to be migrated; got:\n{persisted}"
        );
        assert!(
            persisted.contains("auth_forward = \"sync\""),
            "expected migrated config to contain sync; got:\n{persisted}"
        );
    }

    #[test]
    fn load_migrates_per_agent_copy_to_sync() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[agents.agent-smith.claude]
auth_forward = "copy"
"#,
        )
        .unwrap();

        let config = AppConfig::load_or_init(&paths).unwrap();

        assert_eq!(
            config.resolve_auth_forward_mode("agent-smith"),
            crate::config::AuthForwardMode::Sync
        );

        let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(!persisted.contains("auth_forward = \"copy\""));
    }

    #[test]
    fn load_migration_preserves_user_comments() {
        // Regression test for the persist.rs migration path: the copy→sync
        // and builtin-sync branches must NOT pre-flush through the lossy
        // serde writer, or every user comment gets destroyed before the
        // editor can preserve them.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        let original = r#"# Top-of-file note — keep this
[claude]
auth_forward = "copy"

# Builtin agent, operator-configured
[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

# Keep this comment too — it explains why we trust
[agents.agent-smith.claude]
auth_forward = "copy"
"#;
        std::fs::write(&paths.config_file, original).unwrap();

        let _config = AppConfig::load_or_init(&paths).unwrap();

        let after = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(
            after.contains("# Top-of-file note — keep this"),
            "top-of-file comment lost: {after}"
        );
        assert!(
            after.contains("# Builtin agent, operator-configured"),
            "agent-section comment lost: {after}"
        );
        assert!(
            after.contains("# Keep this comment too — it explains why we trust"),
            "claude-section comment lost: {after}"
        );
        assert!(!after.contains("\"copy\""), "copy not migrated: {after}");
        assert!(after.contains("auth_forward = \"sync\""), "{after}");
    }

    #[test]
    fn load_or_init_rejects_reserved_env_name_in_global_layer() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[env]
DOCKER_HOST = "override-attempt"

[agents.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
        )
        .unwrap();

        let err = AppConfig::load_or_init(&paths).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("DOCKER_HOST"), "{msg}");
        assert!(msg.contains("reserved"), "{msg}");
        assert!(msg.contains("global"), "{msg}");
    }

    #[test]
    fn load_does_not_rewrite_when_no_copy_present() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        // Bootstrap once so builtins are synced and file stabilizes.
        AppConfig::load_or_init(&paths).unwrap();
        let mtime_before = std::fs::metadata(&paths.config_file)
            .unwrap()
            .modified()
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));

        // Second load with no "copy" anywhere — must not rewrite.
        AppConfig::load_or_init(&paths).unwrap();
        let mtime_after = std::fs::metadata(&paths.config_file)
            .unwrap()
            .modified()
            .unwrap();

        assert_eq!(mtime_before, mtime_after);
    }
}
