use super::AppConfig;
use crate::paths::JackinPaths;
use anyhow::Context;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use toml_edit::DocumentMut;

// Per-process counter mixed with the PID into the staged-write filename.
// Combined with the PID it produces unique suffixes across concurrent
// migrations, so two writers cannot clobber each other's staged file before
// rename, and a leftover staged file cannot truncate an operator-created
// `<name>.tmp` workspace file.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
        Err(e) => {
            return Err(e).with_context(|| {
                format!("reading workspaces directory {}", workspaces_dir.display())
            });
        }
    };

    for entry in entries {
        let entry = entry.with_context(|| {
            format!("scanning workspaces directory {}", workspaces_dir.display())
        })?;
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
    // Crash-recovery ordering: the global rewrite is the commit point. If
    // we crash before it, the legacy `[workspaces.*]` tables remain
    // authoritative and the next load_or_init re-runs this function. The
    // exists+equal short-circuit below keeps that re-entry idempotent.
    std::fs::create_dir_all(&paths.workspaces_dir).with_context(|| {
        format!(
            "creating workspaces directory {}",
            paths.workspaces_dir.display()
        )
    })?;
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
                "cannot migrate workspace {name:?}: {} already exists with different contents \
                 than the legacy config.toml. Reconcile the two copies (delete the split file to \
                 take the legacy version, or remove [workspaces.{name}] from config.toml to take \
                 the split file) and re-run.",
                path.display()
            );
        }
        let contents = toml::to_string_pretty(workspace)
            .with_context(|| format!("serializing workspace {name:?}"))?;
        atomic_write(&path, &contents)?;
    }

    // Lossy: serde round-trip drops comments and blank lines from
    // `config.toml`. Acceptable here because this path runs once at legacy
    // migration; steady-state edits go through `ConfigEditor`.
    let global_contents = toml::to_string_pretty(global_config).with_context(|| {
        format!(
            "serializing migrated global config for {}",
            paths.config_file.display()
        )
    })?;
    atomic_write(&paths.config_file, &global_contents)?;
    Ok(())
}

pub fn atomic_write(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent directory {}", parent.display()))?;
    }
    // Place the `.tmp` marker mid-filename rather than as the extension so
    // `load_workspace_files`'s `extension == "toml"` filter ignores leftover
    // staged files. PID + counter make the suffix unique across processes
    // and concurrent in-process writers.
    let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut staged_name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    staged_name.push(format!(".tmp.{}.{counter}", std::process::id()));
    let tmp = path.with_file_name(staged_name);

    let staged = stage_write(&tmp, contents);
    if let Err(err) = staged {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }

    if let Err(rename_err) = std::fs::rename(&tmp, path) {
        // Rename failure leaves the staged file behind; clean up so it does
        // not accumulate.
        let _ = std::fs::remove_file(&tmp);
        return Err(rename_err)
            .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()));
    }
    Ok(())
}

fn stage_write(tmp: &Path, contents: &str) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }

    #[cfg(not(unix))]
    std::fs::write(tmp, contents)?;

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
                    Some(
                        std::fs::read_to_string(&paths.config_file).with_context(|| {
                            format!("re-reading {} after migration", paths.config_file.display())
                        })?,
                    )
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                return Err(e).with_context(|| format!("reading {}", paths.config_file.display()));
            }
        };

        let mut config = load_split_config(paths, contents_opt)?;

        // Pre-sync validation: gives the operator the canonical
        // validate_reserved_names error rather than save()'s "rejecting
        // candidate config" wrapper. ConfigEditor::save runs the same check
        // via validate_candidate; this call covers the path where save() is
        // never invoked because builtins did not drift.
        crate::operator_env::validate_reserved_names(&config)?;

        let builtins_changed = config.sync_builtin_agents();

        if builtins_changed {
            // ConfigEditor::open recurses into load_or_init when the file is
            // missing; bootstrap once here so the editor sees an existing
            // file and preserves operator comments rather than going through
            // the lossy serde rewrite.
            if !paths.config_file.exists() {
                let contents = toml::to_string_pretty(&config)?;
                atomic_write(&paths.config_file, &contents)?;
            }
            let mut editor = crate::config::ConfigEditor::open(paths)?;
            for &(name, git) in crate::config::roles::BUILTIN_ROLES {
                editor.upsert_builtin_agent(name, git);
            }
            // Take save()'s post-write parse: it preserves [roles.X.env] that
            // sync_builtin_agents cleared in-memory.
            config = editor.save()?;
        }

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

    #[test]
    fn config_needs_split_migration_returns_false_for_legacy_without_workspaces() {
        let raw = "[roles.agent-smith]\ngit = \"https://example.test/role.git\"\n";
        assert!(!config_needs_split_migration(raw).unwrap());
    }

    #[test]
    fn config_needs_split_migration_returns_false_for_versioned_with_workspaces() {
        // Versioned config with a leftover `[workspaces.X]` table: split
        // migration is skipped here because `load_split_config` will
        // `std::mem::take` and split-migrate the workspaces.
        let raw = "version = \"v1alpha1\"\n\n[workspaces.prod]\nworkdir = \"/workspace/prod\"\n";
        assert!(!config_needs_split_migration(raw).unwrap());
    }

    #[test]
    fn config_needs_split_migration_returns_true_for_legacy_with_workspaces() {
        let raw = "[workspaces.prod]\nworkdir = \"/workspace/prod\"\n";
        assert!(config_needs_split_migration(raw).unwrap());
    }

    #[test]
    fn config_needs_split_migration_returns_false_for_empty_workspaces_table() {
        let raw = "[workspaces]\n";
        assert!(!config_needs_split_migration(raw).unwrap());
    }

    #[test]
    fn atomic_write_creates_parent_directories() {
        let temp = tempdir().unwrap();
        let nested = temp.path().join("a/b/c/file.toml");
        atomic_write(&nested, "k = 1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&nested).unwrap(), "k = 1\n");
    }

    #[test]
    fn atomic_write_overwrites_existing_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("file.toml");
        atomic_write(&path, "k = 1\n").unwrap();
        atomic_write(&path, "k = 2\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "k = 2\n");
    }

    #[test]
    fn atomic_write_cleans_staged_file_on_rename_failure() {
        // Force rename to fail by placing a directory at the destination.
        let temp = tempdir().unwrap();
        let target = temp.path().join("target.toml");
        std::fs::create_dir(&target).unwrap();

        let err = atomic_write(&target, "k = 1\n").unwrap_err();
        assert!(format!("{err:#}").contains("renaming"), "{err}");

        // No `.tmp.<pid>.<n>` leftovers in the parent directory.
        let leaks: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("target.toml.tmp.")
            })
            .collect();
        assert!(leaks.is_empty(), "leftover staged files: {leaks:?}");
    }

    #[test]
    fn load_or_init_dual_migrates_legacy_config_with_legacy_workspaces() {
        // Pin the dual-migration contract: a legacy `config.toml` (no
        // `version`) carrying `[workspaces.X]` tables ends up with
        // `version = "v1alpha1"` on the global file AND on each split
        // workspace file after one load. The current registries are
        // no-ops; once a real content-changing config migration lands,
        // this test guards the ordering that the version migration runs
        // alongside the split rather than getting silently skipped.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::write(
            &paths.config_file,
            r#"# operator comment
[env]
GLOBAL = "yes"

[workspaces.prod]
workdir = "/workspace/prod"

[[workspaces.prod.mounts]]
src = "/tmp/prod"
dst = "/workspace/prod"
"#,
        )
        .unwrap();

        let config = AppConfig::load_or_init(&paths).unwrap();
        assert!(config.workspaces.contains_key("prod"));

        let global_on_disk = std::fs::read_to_string(&paths.config_file).unwrap();
        let global_parsed: toml::Value = toml::from_str(&global_on_disk).unwrap();
        assert_eq!(global_parsed["version"].as_str().unwrap(), "v1alpha1");
        assert!(!global_on_disk.contains("[workspaces."), "{global_on_disk}");

        let prod_on_disk = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
        let prod_parsed: toml::Value = toml::from_str(&prod_on_disk).unwrap();
        assert_eq!(prod_parsed["version"].as_str().unwrap(), "v1alpha1");

        // Re-running is a no-op: file content stays byte-identical.
        let global_before = std::fs::read(&paths.config_file).unwrap();
        let prod_before = std::fs::read(paths.workspaces_dir.join("prod.toml")).unwrap();
        AppConfig::load_or_init(&paths).unwrap();
        let global_after = std::fs::read(&paths.config_file).unwrap();
        let prod_after = std::fs::read(paths.workspaces_dir.join("prod.toml")).unwrap();
        assert_eq!(global_before, global_after);
        assert_eq!(prod_before, prod_after);
    }

    #[test]
    fn load_workspace_files_migrates_legacy_split_file_in_place() {
        // Pin the contract that legacy `workspaces/<name>.toml` files (no
        // `version` key) get rewritten on first load. Without this test the
        // migrate-on-scan call in `load_workspace_files` is unreachable in
        // tests — every other workspace fixture uses `version = "v1alpha1"`.
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
        std::fs::write(
            paths.workspaces_dir.join("prod.toml"),
            "# keep me\nworkdir = \"/workspace/prod\"\n",
        )
        .unwrap();

        let map = load_workspace_files(&paths.workspaces_dir).unwrap();
        assert!(map.contains_key("prod"));

        let on_disk = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
        let parsed: toml::Value = toml::from_str(&on_disk).unwrap();
        assert_eq!(parsed["version"].as_str().unwrap(), "v1alpha1");
        assert!(on_disk.contains("# keep me"), "{on_disk}");
    }

    #[test]
    fn load_workspace_files_ignores_leftover_staged_files() {
        // A `.tmp.<pid>.<n>` file in workspaces/ must not be treated as a
        // workspace file (extension filter is `.toml`).
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();
        std::fs::create_dir_all(&paths.workspaces_dir).unwrap();
        std::fs::write(
            paths.workspaces_dir.join("real.toml"),
            "version = \"v1alpha1\"\nworkdir = \"/w\"\n",
        )
        .unwrap();
        std::fs::write(
            paths.workspaces_dir.join("real.toml.tmp.99999.0"),
            "garbage",
        )
        .unwrap();

        let map = load_workspace_files(&paths.workspaces_dir).unwrap();
        assert!(map.contains_key("real"));
        assert_eq!(map.len(), 1);
    }
}
