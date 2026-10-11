// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(unix)]
#[test]
fn private_config_fs_rejects_empty_and_dot_roots_before_creating_home() {
    let current = std::env::current_dir().unwrap();
    let sentinel = format!(".jackin-rejected-root-{}", std::process::id());
    let sentinel_path = current.join("provider-config/home").join(&sentinel);
    assert!(!sentinel_path.exists());

    for root in [Path::new(""), Path::new(".")] {
        let error = private_config_fs::open_directory(root, Path::new(&sentinel)).unwrap_err();
        assert!(format!("{error:#}").contains("root"));
        assert!(
            !sentinel_path.exists(),
            "invalid root must not create a private config home"
        );
    }
}

#[cfg(unix)]
#[test]
fn private_config_fs_rejects_lock_symlink_without_following_target() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let directory_path = temp.path().join("provider-config/home/.codex");
    let outside = temp.path().join("outside-lock");
    std::fs::write(&outside, b"outside").unwrap();
    let lock_path = directory_path.join(".jackin-private-provider-config.lock");
    symlink(&outside, &lock_path).unwrap();

    let error = private_config_fs::lock(&directory).unwrap_err();
    assert!(!format!("{error:#}").is_empty());
    assert_eq!(std::fs::read(&outside).unwrap(), b"outside");
    assert!(
        std::fs::symlink_metadata(&lock_path)
            .unwrap()
            .file_type()
            .is_symlink(),
        "lock symlink must remain untouched"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn private_config_fs_normalizes_macos_lexical_root_aliases() {
    assert_eq!(
        private_config_fs::normalize_root(Path::new("/var")).unwrap(),
        PathBuf::from("/private/var")
    );
    assert_eq!(
        private_config_fs::normalize_root(Path::new("/tmp/jackin")).unwrap(),
        PathBuf::from("/private/tmp/jackin")
    );
    assert_eq!(
        private_config_fs::normalize_root(Path::new("/etc")).unwrap(),
        PathBuf::from("/private/etc")
    );
    assert_eq!(
        private_config_fs::normalize_root(Path::new("/various/jackin")).unwrap(),
        PathBuf::from("/various/jackin")
    );
    private_config_fs::normalize_root(Path::new("/var/../tmp"))
        .expect_err("private account config root must reject parent traversal");
}

#[cfg(unix)]
#[test]
fn private_config_fs_rejects_non_leaf_names_before_filesystem_access() {
    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let directory_path = temp.path().join("provider-config/home/.codex");
    let outside = temp.path().join("provider-config/home/outside");
    let catalog = br#"{"models":[{"slug":"fixture"}]}"#;
    std::fs::write(&outside, catalog).unwrap();

    for name in [
        "",
        ".",
        "..",
        "../outside",
        "nested/name",
        "/outside",
        "outside\0",
    ] {
        assert!(
            private_config_fs::read_optional(&directory, name).is_err(),
            "name {name:?} must be rejected"
        );
    }

    let error = private_config_fs::quarantine(&directory, "../outside", "test").unwrap_err();
    assert!(format!("{error:#}").contains("name"));
    assert_eq!(std::fs::read(&outside).unwrap(), catalog);

    let error = private_config_fs::publish_catalog(&directory, "../outside", catalog, |_| Ok(()))
        .unwrap_err();
    assert!(format!("{error:#}").contains("name"));
    assert_eq!(std::fs::read(&outside).unwrap(), catalog);

    let error = private_config_fs::publish_atomic(
        &directory,
        "../outside",
        b"replacement",
        private_config_fs::Artifact::CodexConfig,
        |_| Ok(()),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("name"));
    assert_eq!(std::fs::read(&outside).unwrap(), catalog);

    let (temp_name, temp_file) = private_config_fs::create_temp_file(&directory).unwrap();
    let outside_owned = temp.path().join("provider-config/home/outside-owned");
    std::fs::hard_link(directory_path.join(&temp_name), &outside_owned).unwrap();
    let error =
        private_config_fs::cleanup_owned_temp(&directory, "../outside-owned", &temp_file, Ok(()))
            .unwrap_err();
    assert!(format!("{error:#}").contains("name"));
    assert!(
        outside_owned.exists(),
        "invalid cleanup must not unlink outside"
    );
    private_config_fs::cleanup_owned_temp(&directory, &temp_name, &temp_file, Ok(())).unwrap();
    assert!(
        outside_owned.exists(),
        "hard link must remain outside the directory"
    );
}

#[cfg(unix)]
#[test]
fn private_config_fs_does_not_remove_replaced_orphan_staging_files() {
    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let directory_path = temp.path().join("provider-config/home/.codex");
    let (name, file) = private_config_fs::create_temp_file(&directory).unwrap();
    std::fs::remove_file(directory_path.join(&name)).unwrap();
    std::fs::write(directory_path.join(&name), b"foreign orphan").unwrap();

    let error =
        private_config_fs::cleanup_owned_temp(&directory, &name, &file, Ok(())).unwrap_err();
    assert!(format!("{error:#}").contains("changed ownership"));
    assert_eq!(
        std::fs::read(directory_path.join(&name)).unwrap(),
        b"foreign orphan"
    );
    std::fs::remove_file(directory_path.join(&name)).unwrap();
}

#[cfg(unix)]
#[test]
fn codex_catalog_publication_is_immutable_and_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let contents = br#"{"models":[{"slug":"fixture"}]}"#;
    let name = codex_catalog_filename(contents);

    private_config_fs::publish_catalog(&directory, &name, contents, |_| Ok(())).unwrap();
    private_config_fs::publish_catalog(&directory, &name, contents, |_| Ok(())).unwrap();
    assert_eq!(
        std::fs::read(temp.path().join("provider-config/home/.codex").join(&name)).unwrap(),
        contents
    );

    let collision = br#"{"models":[{"slug":"different"}]}"#;
    let error =
        private_config_fs::publish_catalog(&directory, &name, collision, |_| Ok(())).unwrap_err();
    assert!(
        format!("{error:#}").contains("different contents"),
        "unexpected collision error: {error:#}"
    );
}

#[cfg(unix)]
#[test]
fn codex_catalog_publication_rejects_symlink_target() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let contents = br#"{"models":[{"slug":"fixture"}]}"#;
    let name = codex_catalog_filename(contents);
    let outside = temp.path().join("outside.json");
    std::fs::write(&outside, b"outside").unwrap();
    symlink(
        &outside,
        temp.path().join("provider-config/home/.codex").join(&name),
    )
    .unwrap();

    let error =
        private_config_fs::publish_catalog(&directory, &name, contents, |_| Ok(())).unwrap_err();
    assert!(
        !format!("{error:#}").is_empty(),
        "symlink target must be rejected"
    );
    assert_eq!(std::fs::read(&outside).unwrap(), b"outside");
}

#[cfg(unix)]
#[test]
fn codex_catalog_publication_failure_leaves_no_staged_file() {
    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let contents = br#"{"models":[{"slug":"fixture"}]}"#;
    let name = codex_catalog_filename(contents);
    let error =
        private_config_fs::publish_catalog(&directory, &name, contents, |point| match point {
            private_config_fs::PublishPoint::BeforeInstall(
                private_config_fs::Artifact::CodexCatalog,
            ) => anyhow::bail!("injected publication failure"),
            _ => Ok(()),
        })
        .unwrap_err();
    assert!(format!("{error:#}").contains("injected publication failure"));
    assert!(
        !temp
            .path()
            .join("provider-config/home/.codex")
            .join(&name)
            .exists()
    );
    let leftovers = std::fs::read_dir(temp.path().join("provider-config/home/.codex"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".jackin-private-provider-config-")
        })
        .count();
    assert_eq!(leftovers, 0, "owned staging file must be cleaned up");
}

#[cfg(unix)]
#[test]
fn codex_catalog_install_failures_leave_a_durable_retryable_catalog() {
    for failed_at in [
        private_config_fs::PublishPoint::Installed(private_config_fs::Artifact::CodexCatalog),
        private_config_fs::PublishPoint::DirectorySynced(private_config_fs::Artifact::CodexCatalog),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let directory =
            private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
        let contents = br#"{"models":[{"slug":"retry"}]}"#;
        let name = codex_catalog_filename(contents);
        let error = private_config_fs::publish_catalog(&directory, &name, contents, |point| {
            (point == failed_at)
                .then_some(anyhow::anyhow!("injected catalog publication failure"))
                .map_or(Ok(()), Err)
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected catalog publication failure"));
        assert_eq!(
            std::fs::read(temp.path().join("provider-config/home/.codex").join(&name)).unwrap(),
            contents
        );
        private_config_fs::publish_catalog(&directory, &name, contents, |_| Ok(())).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn config_rename_failures_preserve_the_new_catalog_pair_and_retry() {
    for failed_at in [
        private_config_fs::PublishPoint::Installed(private_config_fs::Artifact::CodexConfig),
        private_config_fs::PublishPoint::DirectorySynced(private_config_fs::Artifact::CodexConfig),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let directory =
            private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
        let catalog = br#"{"models":[{"slug":"pair"}]}"#;
        let catalog_name = codex_catalog_filename(catalog);
        private_config_fs::publish_catalog(&directory, &catalog_name, catalog, |_| Ok(())).unwrap();
        let config = format!("model_catalog_json = \"/home/agent/.codex/{catalog_name}\"\n");
        let error = private_config_fs::publish_atomic(
            &directory,
            "config.toml",
            config.as_bytes(),
            private_config_fs::Artifact::CodexConfig,
            |point| {
                (point == failed_at)
                    .then_some(anyhow::anyhow!("injected config publication failure"))
                    .map_or(Ok(()), Err)
            },
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected config publication failure"));
        assert_eq!(
            std::fs::read_to_string(temp.path().join("provider-config/home/.codex/config.toml"))
                .unwrap(),
            config
        );
        assert!(
            temp.path()
                .join("provider-config/home/.codex")
                .join(&catalog_name)
                .is_file()
        );
        private_config_fs::publish_atomic(
            &directory,
            "config.toml",
            config.as_bytes(),
            private_config_fs::Artifact::CodexConfig,
            |_| Ok(()),
        )
        .unwrap();
    }
}

#[test]
fn codex_catalog_rotation_preserves_stale_container_reference() {
    let temp = tempfile::tempdir().unwrap();
    let (config, first_instances) = codex_moonshot_fixture();
    configure_for_test(temp.path(), &config, &first_instances).unwrap();
    let directory = temp.path().join("provider-config/home/.codex");
    let first_catalog = {
        let document: toml::Value =
            toml::from_str(&std::fs::read_to_string(directory.join("config.toml")).unwrap())
                .unwrap();
        Path::new(document["model_catalog_json"].as_str().unwrap())
            .file_name()
            .unwrap()
            .to_owned()
    };
    assert!(directory.join(&first_catalog).is_file());

    let second_instances = [instance(
        "codex-work",
        Agent::Codex,
        "work",
        Some("k3"),
        None,
    )];
    let second_config = config;
    // The first fixture uses k3-256k; the second publication intentionally
    // changes the catalog content while keeping the slot/container path.
    configure_for_test(temp.path(), &second_config, &second_instances).unwrap();
    let document: toml::Value =
        toml::from_str(&std::fs::read_to_string(directory.join("config.toml")).unwrap()).unwrap();
    let second_catalog = Path::new(document["model_catalog_json"].as_str().unwrap())
        .file_name()
        .unwrap()
        .to_owned();
    assert_ne!(first_catalog, second_catalog);
    assert!(directory.join(&first_catalog).is_file());
    assert!(directory.join(&second_catalog).is_file());
}
