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

        let mut config: Self = match contents_opt {
            Some(c) => toml::from_str(&c)?,
            None => Self::default(),
        };

        // Run the reserved-name validation against the on-disk shape
        // BEFORE the builtin-sync editor save below. `ConfigEditor::save`
        // also runs this check as a safety net, but doing it here first
        // means the operator gets the canonical
        // `validate_reserved_names` error directly — without the
        // "rejecting candidate config" wrapper that save() adds when it
        // rolls back a write.
        crate::operator_env::validate_reserved_names(&config)?;

        let builtins_changed = config.sync_builtin_agents();

        if builtins_changed {
            // Bootstrap only when the file doesn't exist yet. Without this
            // gate, ConfigEditor::open would call load_or_init for the
            // missing file and recurse. When the file DOES exist (the
            // builtins-drifted upgrade path), we must NOT rewrite it
            // through the lossy serde path first — that would destroy
            // every user comment before the editor could preserve them.
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
            for &(name, git) in crate::config::roles::BUILTIN_ROLES {
                editor.upsert_builtin_agent(name, git);
            }
            // editor.save() returns an AppConfig parsed from the on-disk file,
            // which has [roles.X.env] preserved (upsert_builtin_agent doesn't
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
    fn load_rejects_deprecated_copy_alias() {
        // Pre-release stance: no compatibility shims. `auth_forward = "copy"`
        // must hard-fail at load instead of being silently migrated.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[claude]
auth_forward = "copy"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
        )
        .unwrap();

        let err = AppConfig::load_or_init(&paths).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unknown variant `copy`") || msg.contains("invalid auth_forward mode"),
            "expected parse error rejecting `copy`, got: {msg}"
        );
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

[roles.agent-smith]
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
    fn load_is_idempotent_when_builtins_already_synced() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        // Bootstrap once so builtins are synced and file stabilizes.
        AppConfig::load_or_init(&paths).unwrap();
        let mtime_before = std::fs::metadata(&paths.config_file)
            .unwrap()
            .modified()
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));

        // Second load on a stable file must not rewrite.
        AppConfig::load_or_init(&paths).unwrap();
        let mtime_after = std::fs::metadata(&paths.config_file)
            .unwrap()
            .modified()
            .unwrap();

        assert_eq!(mtime_before, mtime_after);
    }
}
