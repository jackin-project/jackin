//! Explicit migration fixture writer for source-derived golden output.
//!
//! The ignored test writes only to a new task-owned output directory. It never
//! updates committed inputs or existing golden files.
//!
//! Run it with `JACKIN_MIGRATION_FIXTURE_OUTPUT_DIR=/absolute/new/path mise
//! exec --locked -- mbx test --locked --offline -p jackin --test
//! migration_fixtures rebake_migration_fixtures_to_output_dir -- --ignored`.

use super::{FixtureMeta, MigrateFn, filename_for, fixture_root, parse_fixture};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const OUTPUT_ENV: &str = "JACKIN_MIGRATION_FIXTURE_OUTPUT_DIR";
const MAX_FIXTURES_PER_KIND: usize = 128;
const MAX_FIXTURE_FILE_BYTES: u64 = 1024 * 1024;

/// Generate current config/workspace golden files into a separate directory.
///
/// Set `JACKIN_MIGRATION_FIXTURE_OUTPUT_DIR` to an absent absolute path, then
/// run this ignored test explicitly. The committed fixture tree remains intact.
#[test]
#[ignore = "explicit fixture rebake; writes only to JACKIN_MIGRATION_FIXTURE_OUTPUT_DIR"]
fn rebake_migration_fixtures_to_output_dir() {
    let output = env::var_os(OUTPUT_ENV)
        .map(PathBuf::from)
        .expect("JACKIN_MIGRATION_FIXTURE_OUTPUT_DIR must name a task-owned output path");
    assert!(output.is_absolute(), "fixture output path must be absolute");
    assert_missing_path(&output);
    let output_name = output
        .file_name()
        .expect("fixture output path must have a final component");
    let output_parent = output
        .parent()
        .expect("fixture output path must have a parent");
    let output_parent = fs::canonicalize(output_parent).unwrap_or_else(|error| {
        panic!(
            "canonicalizing fixture output parent {}: {error}",
            output_parent.display()
        )
    });
    let output = output_parent.join(output_name);
    let source_fixtures = fs::canonicalize(fixture_root()).unwrap();
    assert!(
        !output.starts_with(&source_fixtures),
        "fixture output must be outside the committed source fixtures"
    );

    let staging = tempfile::Builder::new()
        .prefix(".migration-fixture-rebake-")
        .tempdir_in(&output_parent)
        .unwrap();
    rebake_fixtures(
        "config",
        |path| Ok(jackin_config::migrate_config_file_if_needed(path).map(|_| ())?),
        staging.path(),
    );
    rebake_fixtures(
        "workspace",
        |path| Ok(jackin_config::migrate_workspace_file_if_needed(path).map(|_| ())?),
        staging.path(),
    );

    fs::rename(staging.path(), &output).unwrap_or_else(|error| {
        panic!(
            "publishing generated fixture directory {}: {error}",
            output.display()
        )
    });
}

fn assert_missing_path(path: &Path) {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => panic!("fixture output path already exists: {}", path.display()),
        Err(error) => panic!("checking fixture output path {}: {error}", path.display()),
    }
}

fn rebake_fixtures(file_kind: &str, migrate: MigrateFn, output_root: &Path) {
    let file_kind_dir = fixture_root().join(file_kind);
    let mut entries = Vec::new();
    for result in fs::read_dir(&file_kind_dir)
        .unwrap_or_else(|error| panic!("reading {}: {error}", file_kind_dir.display()))
    {
        let entry = result.unwrap_or_else(|error| {
            panic!(
                "reading an entry under {}: {error}",
                file_kind_dir.display()
            )
        });
        if entry.file_type().unwrap().is_dir() {
            entries.push(entry);
        }
    }
    entries.sort_by_key(fs::DirEntry::file_name);
    assert!(
        !entries.is_empty() && entries.len() <= MAX_FIXTURES_PER_KIND,
        "unexpected fixture inventory under {}: {} entries",
        file_kind_dir.display(),
        entries.len()
    );

    let output_kind = output_root.join(file_kind);
    fs::create_dir(&output_kind).unwrap();
    for entry in entries {
        let source_dir = entry.path();
        let name = entry.file_name().into_string().unwrap_or_else(|_| {
            panic!(
                "fixture directory name is not valid UTF-8: {}",
                source_dir.display()
            )
        });
        assert!(
            name.starts_with("from-")
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "unexpected migration fixture directory name: {name}"
        );
        let before = read_fixture_file(&source_dir.join("before.toml"));
        let meta_raw = read_fixture_file(&source_dir.join("meta.toml"));
        let meta: FixtureMeta = toml::from_str(&meta_raw)
            .unwrap_or_else(|error| panic!("parsing {name}/meta.toml: {error}"));
        assert_eq!(
            name,
            format!("from-{}", meta.from_version),
            "fixture directory and from_version differ"
        );

        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join(filename_for(file_kind));
        fs::write(&target, &before).unwrap();
        let (after, output_meta) = if let Some(expected_error) = meta.expected_error.as_deref() {
            let error = migrate(&target).unwrap_err();
            assert!(
                format!("{error:#}").contains(expected_error),
                "{name}: expected error containing {expected_error:?}, got {error:#}"
            );
            assert_eq!(fs::read_to_string(&target).unwrap(), before, "{name}");
            (before, meta_raw)
        } else {
            migrate(&target).unwrap_or_else(|error| panic!("migrating {name}: {error:#}"));
            let after = fs::read_to_string(&target).unwrap();
            parse_fixture(file_kind, &after, &name, "actual");
            let document: toml::Value = toml::from_str(&after).unwrap();
            let target_version = document["version"]
                .as_str()
                .unwrap_or_else(|| panic!("{name}: migrated output has no version"));
            let output_meta =
                replace_target_version(&meta_raw, &meta.target_version, target_version)
                    .unwrap_or_else(|error| panic!("{name}/meta.toml: {error}"));
            (after, output_meta)
        };

        let output_dir = output_kind.join(name);
        fs::create_dir(&output_dir).unwrap();
        fs::write(output_dir.join("after.toml"), after).unwrap();
        fs::write(output_dir.join("meta.toml"), output_meta).unwrap();
    }
}

fn read_fixture_file(path: &Path) -> String {
    let metadata = fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("reading metadata for {}: {error}", path.display()));
    assert!(
        metadata.file_type().is_file(),
        "fixture input must be a regular file: {}",
        path.display()
    );
    assert!(
        metadata.len() <= MAX_FIXTURE_FILE_BYTES,
        "fixture input is larger than {MAX_FIXTURE_FILE_BYTES} bytes: {}",
        path.display()
    );
    fs::read_to_string(path).unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

fn replace_target_version(meta: &str, old: &str, new: &str) -> Result<String, String> {
    let old_line = format!("target_version = \"{old}\"");
    let count = meta.matches(&old_line).count();
    if count != 1 {
        return Err(format!(
            "expected exactly one target_version line for {old}, found {count}"
        ));
    }
    Ok(meta.replacen(&old_line, &format!("target_version = \"{new}\""), 1))
}

#[test]
fn fixture_rebake_updates_only_one_target_version_line() {
    assert_eq!(
        replace_target_version(
            "from_version = \"v1alpha11\"\ntarget_version = \"v1alpha12\"\nsummary = \"test\"\n",
            "v1alpha12",
            "v1alpha13"
        )
        .unwrap(),
        "from_version = \"v1alpha11\"\ntarget_version = \"v1alpha13\"\nsummary = \"test\"\n"
    );
    assert!(
        replace_target_version("target_version = \"v1alpha12\"\n", "v1alpha11", "v1alpha13")
            .is_err()
    );
    assert!(
        replace_target_version(
            "target_version = \"v1alpha12\"\ntarget_version = \"v1alpha12\"\n",
            "v1alpha12",
            "v1alpha13"
        )
        .is_err()
    );
}
