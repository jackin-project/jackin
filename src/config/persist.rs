use super::AppConfig;
use crate::paths::JackinPaths;
use anyhow::Context;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

pub fn validate_workspace_file_stem(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("workspace name cannot be empty");
    }
    if name == "." || name == ".." {
        anyhow::bail!("workspace name {name:?} is reserved");
    }
    if name.contains('/') || name.contains('\\') {
        anyhow::bail!("workspace name {name:?} cannot contain path separators");
    }
    #[cfg(windows)]
    {
        const RESERVED: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];
        if RESERVED
            .iter()
            .any(|reserved| name.eq_ignore_ascii_case(reserved))
        {
            anyhow::bail!("workspace name {name:?} is reserved on Windows");
        }
        if name.ends_with('.') || name.ends_with(' ') {
            anyhow::bail!("workspace name {name:?} cannot end with a dot or space on Windows");
        }
    }
    Ok(())
}

pub fn workspace_file_path(paths: &JackinPaths, name: &str) -> PathBuf {
    paths.workspaces_dir.join(format!("{name}.toml"))
}

pub fn load_split_config(
    paths: &JackinPaths,
    contents_opt: Option<String>,
) -> anyhow::Result<AppConfig> {
    let mut config: AppConfig = match contents_opt {
        Some(c) => toml::from_str(&c)?,
        None => AppConfig::default(),
    };

    let legacy_workspaces = std::mem::take(&mut config.workspaces);
    if !legacy_workspaces.is_empty() {
        migrate_legacy_workspaces(paths, &config, &legacy_workspaces)?;
        eprintln!(
            "jackin migrated saved workspaces into {}",
            paths.workspaces_dir.display()
        );
    }

    config.workspaces = load_workspace_files(&paths.workspaces_dir)?;
    Ok(config)
}

pub fn load_workspace_files(
    workspaces_dir: &Path,
) -> anyhow::Result<BTreeMap<String, crate::workspace::WorkspaceConfig>> {
    let mut workspaces = BTreeMap::new();
    let entries = match std::fs::read_dir(workspaces_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(workspaces),
        Err(e) => return Err(e.into()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid workspace filename {}", path.display()))?;
        validate_workspace_file_stem(stem)
            .with_context(|| format!("invalid workspace filename {}", path.display()))?;
        crate::config::migrations::migrate_workspace_file_if_needed(&path)?;
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading workspace config {}", path.display()))?;
        let workspace = toml::from_str(&raw)
            .with_context(|| format!("parsing workspace config {}", path.display()))?;
        workspaces.insert(stem.to_string(), workspace);
    }
    Ok(workspaces)
}

fn migrate_legacy_workspaces(
    paths: &JackinPaths,
    global_config: &AppConfig,
    workspaces: &BTreeMap<String, crate::workspace::WorkspaceConfig>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.workspaces_dir)?;
    for (name, workspace) in workspaces {
        validate_workspace_file_stem(name)?;
        let path = workspace_file_path(paths, name);
        if path.exists() {
            let existing: crate::workspace::WorkspaceConfig = toml::from_str(
                &std::fs::read_to_string(&path)
                    .with_context(|| format!("reading existing workspace {}", path.display()))?,
            )
            .with_context(|| format!("parsing existing workspace {}", path.display()))?;
            if &existing == workspace {
                continue;
            }
            anyhow::bail!(
                "cannot migrate workspace {name:?}: {} already exists with different contents",
                path.display()
            );
        }
        let contents = toml::to_string_pretty(workspace)
            .with_context(|| format!("serializing workspace {name:?}"))?;
        atomic_write(&path, &contents)?;
    }

    let global_contents = toml::to_string_pretty(global_config)?;
    atomic_write(&paths.config_file, &global_contents)?;
    Ok(())
}

pub fn atomic_write(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");

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
    std::fs::write(&tmp, contents)?;

    std::fs::rename(&tmp, path)?;
    Ok(())
}

impl AppConfig {
    pub fn load_or_init(paths: &JackinPaths) -> anyhow::Result<Self> {
        paths.ensure_base_dirs()?;

        let contents_opt = match std::fs::read_to_string(&paths.config_file) {
            Ok(raw) => {
                if config_needs_split_migration(&raw)? {
                    Some(raw)
                } else {
                    crate::config::migrations::migrate_config_file_if_needed(&paths.config_file)?;
                    Some(std::fs::read_to_string(&paths.config_file)?)
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };

        let mut config = load_split_config(paths, contents_opt)?;

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
                let contents = toml::to_string_pretty(&config)?;
                atomic_write(&paths.config_file, &contents)?;
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

fn config_needs_split_migration(raw: &str) -> anyhow::Result<bool> {
    let doc: DocumentMut = raw.parse().context("parsing config.toml")?;
    let version = crate::config::migrations::doc_version(&doc, "config")?;
    let has_legacy_workspaces = doc
        .get("workspaces")
        .and_then(toml_edit::Item::as_table)
        .is_some_and(|workspaces| !workspaces.is_empty());
    Ok(version == crate::config::migrations::SchemaVersion::Legacy && has_legacy_workspaces)
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
    fn load_rejects_invalid_auth_forward_value() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        std::fs::write(
            &paths.config_file,
            r#"[claude]
auth_forward = "bogus"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
        )
        .unwrap();

        let err = AppConfig::load_or_init(&paths).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unknown variant `bogus`") || msg.contains("invalid auth_forward mode"),
            "expected parse error rejecting `bogus`, got: {msg}"
        );
    }

    #[test]
    fn load_or_init_migrates_legacy_config_version() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"# keep me

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#,
        )
        .unwrap();

        let config = AppConfig::load_or_init(&paths).unwrap();
        let out = std::fs::read_to_string(&paths.config_file).unwrap();

        assert_eq!(config.version, crate::config::CURRENT_CONFIG_VERSION);
        assert!(out.contains(r#"version = "v1alpha1""#), "{out}");
        assert!(out.contains("# keep me"), "{out}");
    }

    #[test]
    fn load_or_init_rejects_newer_config_version() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, r#"version = "v2alpha1""#).unwrap();

        let err = AppConfig::load_or_init(&paths).unwrap_err();

        assert!(err.to_string().contains("only understands up to v1alpha1"));
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

    #[test]
    fn load_migrates_legacy_workspaces_into_split_files() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"[env]
GLOBAL = "yes"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[workspaces.prod]
workdir = "/workspace/prod"

[[workspaces.prod.mounts]]
src = "/tmp/prod"
dst = "/workspace/prod"

[workspaces.prod.env]
LOCAL = "only-prod"
"#,
        )
        .unwrap();

        let config = AppConfig::load_or_init(&paths).unwrap();
        assert!(config.workspaces.contains_key("prod"));

        let global = std::fs::read_to_string(&paths.config_file).unwrap();
        assert!(global.contains(r#"version = "v1alpha1""#), "{global}");
        assert!(global.contains("[env]"), "{global}");
        assert!(!global.contains("[workspaces."), "{global}");

        let workspace = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
        assert!(workspace.contains(r#"version = "v1alpha1""#), "{workspace}");
        assert!(
            workspace.contains(r#"workdir = "/workspace/prod""#),
            "{workspace}"
        );
        assert!(workspace.contains("[env]"), "{workspace}");
        assert!(workspace.contains(r#"LOCAL = "only-prod""#), "{workspace}");
    }

    #[test]
    fn failed_split_migration_leaves_legacy_config_unchanged() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
        std::fs::write(
            paths.workspaces_dir.join("prod.toml"),
            r#"version = "v1alpha1"
workdir = "/other"
"#,
        )
        .unwrap();
        let legacy = r#"[workspaces.prod]
workdir = "/workspace/prod"
"#;
        std::fs::write(&paths.config_file, legacy).unwrap();

        let err = AppConfig::load_or_init(&paths).unwrap_err();
        let out = std::fs::read_to_string(&paths.config_file).unwrap();

        assert!(
            err.to_string()
                .contains("already exists with different contents")
        );
        assert_eq!(out, legacy);
    }

    #[test]
    fn empty_legacy_workspaces_table_still_gets_version_stamp() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(&paths.config_file, "[workspaces]\n").unwrap();

        let config = AppConfig::load_or_init(&paths).unwrap();
        let out = std::fs::read_to_string(&paths.config_file).unwrap();

        assert_eq!(config.version, crate::config::CURRENT_CONFIG_VERSION);
        assert!(out.contains(r#"version = "v1alpha1""#), "{out}");
    }

    #[test]
    fn load_rejects_invalid_workspace_filename() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
        std::fs::write(paths.workspaces_dir.join("..toml"), "").unwrap();

        let err = AppConfig::load_or_init(&paths).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid workspace filename"), "{msg}");
    }
}
