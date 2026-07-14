use super::{docker_startup_error, take_post_console_config};
use jackin_config::AppConfig;
use jackin_config::{MountConfig, WorkspaceConfig};
use jackin_core::JackinPaths;
use jackin_core::agent::Agent;
use jackin_core::isolation::MountIsolation;
use tempfile::tempdir;

#[test]
fn docker_startup_error_includes_visible_detail() {
    let error = anyhow::anyhow!(
        "failed to connect to Docker daemon: connect to Docker host unix:///tmp/missing.sock"
    );

    let (title, message) = docker_startup_error(&error);

    assert_eq!(title, "Docker daemon not reachable");
    assert!(message.contains("jackin could not connect to the Docker daemon."));
    assert!(message.contains("failed to connect to Docker daemon"));
    assert!(message.contains("connect to Docker host unix:///tmp/missing.sock"));
    assert!(message.contains("Start Docker or switch to a reachable Docker context"));
}

/// Launch-speed 008g: a no-op console (no settings/workspace save) must not
/// re-read disk. If something else mutates the on-disk file after the console
/// returns its in-memory model, post-console still uses the returned model.
#[test]
fn no_op_console_skips_disk_reload_for_post_console_config() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut on_disk = AppConfig::load_or_init(&paths).unwrap();
    on_disk.env.insert(
        "JACKIN_TEST_NOOP".to_owned(),
        jackin_core::EnvValue::Plain("from-console".to_owned()),
    );
    // Persist the "console-owned" snapshot, then poison disk with a different
    // value that a reload would pick up.
    std::fs::write(
        &paths.config_file,
        toml::to_string(&on_disk).expect("serialize console snapshot"),
    )
    .unwrap();
    let console_owned = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(
        console_owned
            .env
            .get("JACKIN_TEST_NOOP")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("from-console")
    );

    let mut poisoned = console_owned.clone();
    poisoned.env.insert(
        "JACKIN_TEST_NOOP".to_owned(),
        jackin_core::EnvValue::Plain("from-disk-after-console".to_owned()),
    );
    std::fs::write(
        &paths.config_file,
        toml::to_string(&poisoned).expect("serialize poisoned disk"),
    )
    .unwrap();

    // Shipped path: use the returned console config, not load_or_init.
    let post = take_post_console_config(console_owned);
    assert_eq!(
        post.env
            .get("JACKIN_TEST_NOOP")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("from-console"),
        "no-op console path must keep the returned model and ignore later disk writes"
    );
    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(
        reloaded
            .env
            .get("JACKIN_TEST_NOOP")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("from-disk-after-console"),
        "control: disk really changed; reload would have returned the poison"
    );
}

/// Launch-speed 008g: after a successful settings/workspace save the console
/// mutates its in-memory `AppConfig`; that value must feed the next launch even
/// if disk is still lagging or was replaced underfoot.
#[test]
fn saved_console_config_feeds_post_console_launch_path() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let workspace_root = temp.path().join("project");
    std::fs::create_dir_all(&workspace_root).unwrap();
    let canonical = workspace_root.canonicalize().unwrap();

    // Disk starts without the saved workspace.
    let disk_before = AppConfig::load_or_init(&paths).unwrap();
    assert!(!disk_before.workspaces.contains_key("saved-ws"));

    // Console save path updates the in-memory model (mirrors *config = saved).
    let mut console_owned = disk_before;
    console_owned.workspaces.insert(
        "saved-ws".to_owned(),
        WorkspaceConfig {
            workdir: "/workspace/project".to_owned(),
            mounts: vec![MountConfig {
                src: canonical.display().to_string(),
                dst: "/workspace/project".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            }],
            default_agent: Some(Agent::Codex),
            ..Default::default()
        },
    );

    let post = take_post_console_config(console_owned);
    assert!(
        post.workspaces.contains_key("saved-ws"),
        "post-console launch must see the workspace the console saved in memory"
    );
    assert_eq!(
        post.workspaces
            .get("saved-ws")
            .and_then(|ws| ws.default_agent),
        Some(Agent::Codex)
    );

    // Disk still lacks the workspace (save may write asynchronously / tests
    // prove in-memory handoff, not the background writer).
    let still_disk = AppConfig::load_or_init(&paths).unwrap();
    assert!(
        !still_disk.workspaces.contains_key("saved-ws"),
        "control: disk never received the save; only the returned model carries it"
    );
}
