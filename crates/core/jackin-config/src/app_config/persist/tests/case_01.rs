// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    wait_for_mtime_tick();

    // Second load should not rewrite
    AppConfig::load_or_init(&paths).unwrap();
    let mtime_after = std::fs::metadata(&paths.config_file)
        .unwrap()
        .modified()
        .unwrap();

    assert_eq!(mtime_before, mtime_after);
}

#[test]
fn load_or_init_waits_for_an_existing_config_writer() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    AppConfig::load_or_init(&paths).unwrap();

    let writer = acquire_config_write_lock(&paths.config_file).unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let worker_paths = paths.clone();
    let worker = std::thread::spawn(move || {
        let result = AppConfig::load_or_init(&worker_paths);
        done_tx.send(result.is_ok()).unwrap();
    });

    assert!(matches!(
        done_rx.recv_timeout(TestDuration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    drop(writer);
    assert!(done_rx.recv_timeout(TestDuration::from_secs(1)).unwrap());
    worker.join().unwrap();
}

#[test]
fn split_migration_does_not_commit_an_earlier_file_when_a_later_file_is_unreadable() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    AppConfig::load_or_init(&paths).unwrap();
    std::fs::create_dir_all(&paths.workspaces_dir).unwrap();

    let alpha_path = paths.workspaces_dir.join("alpha.toml");
    let alpha_before = b"version = \"v1alpha1\"\nworkdir = \"/workspace/alpha\"\n";
    std::fs::write(&alpha_path, alpha_before).unwrap();
    std::fs::create_dir(paths.workspaces_dir.join("zulu.toml")).unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();
    assert!(err.to_string().contains("reading"), "{err:#}");
    assert_eq!(std::fs::read(&alpha_path).unwrap(), alpha_before);
}

#[test]
fn load_rejects_invalid_auth_forward_value() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[github]
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
fn load_failure_exports_once_without_path_or_config_contents() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("config-secret-root");
    let paths = JackinPaths::for_tests(&root);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "config-secret-invalid-content = [unterminated",
    )
    .unwrap();
    let config_path = paths.config_file.to_string_lossy().into_owned();
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    AppConfig::load_or_init(&paths).unwrap_err();

    export.force_flush();
    assert_eq!(export.event_count("config.operation"), 1);
    assert!(export.contains_log_text("load"));
    assert!(export.contains_log_text("config_error"));
    for prohibited in [
        config_path.as_str(),
        "config-secret-root",
        "config-secret-invalid-content",
        "unterminated",
    ] {
        assert!(!export.contains_log_text(prohibited));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_config_load_failure_exports_once_without_private_input() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("wire-config-secret-root");
    let paths = JackinPaths::for_tests(&root);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        "wire-config-secret-key = [wire-config-secret-value",
    )
    .unwrap();
    let config_path = paths.config_file.to_string_lossy().into_owned();
    let testbed = jackin_otlp_testbed::Testbed::start().expect("start OTLP testbed");
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )
    .expect("initialize wire test export");

    AppConfig::load_or_init(&paths).unwrap_err();
    jackin_diagnostics::flush_wire_test_export().expect("flush wire test export");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let records = loop {
        let records = testbed
            .log_records()
            .into_iter()
            .filter(|record| record.event_name == "config.operation")
            .collect::<Vec<_>>();
        if records.len() == 1 {
            break records;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "config operation wire event did not arrive exactly once"
        );
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    };
    let wire_text = format!("{records:?}");
    for expected in ["global", "load", "failure", "config_error"] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    let prohibited = [
        config_path.as_str(),
        "wire-config-secret-root",
        "wire-config-secret-key",
        "wire-config-secret-value",
    ];
    for value in prohibited {
        assert!(!wire_text.contains(value), "exported {value}");
    }
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
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

    assert_eq!(config.version, CURRENT_CONFIG_VERSION);
    assert!(
        out.contains(&format!(r#"version = "{CURRENT_CONFIG_VERSION}""#)),
        "{out}"
    );
    assert!(out.contains("# keep me"), "{out}");
}

#[test]
fn load_or_init_rejects_newer_config_version() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, r#"version = "v2alpha1""#).unwrap();

    let err = AppConfig::load_or_init(&paths).unwrap_err();

    assert!(
        err.to_string()
            .contains(&format!("only understands up to {CURRENT_CONFIG_VERSION}"))
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

    wait_for_mtime_tick();

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
    assert!(
        global.contains(&format!(r#"version = "{CURRENT_CONFIG_VERSION}""#)),
        "{global}"
    );
    assert!(global.contains("[env]"), "{global}");
    assert!(!global.contains("[workspaces."), "{global}");

    let workspace = std::fs::read_to_string(paths.workspaces_dir.join("prod.toml")).unwrap();
    assert!(
        workspace.contains(&format!(r#"version = "{CURRENT_WORKSPACE_VERSION}""#)),
        "{workspace}"
    );
    assert!(
        workspace.contains(r#"workdir = "/workspace/prod""#),
        "{workspace}"
    );
    assert!(workspace.contains("[env]"), "{workspace}");
    assert!(workspace.contains(r#"LOCAL = "only-prod""#), "{workspace}");
}

#[test]
fn load_migrates_legacy_global_agent_tables_before_embedded_split() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"[claude]
auth_forward = "sync"

[roles.builder]
git = "https://example.test/builder.git"

[roles.builder.codex]
auth_forward = "sync"

[workspaces.prod]
workdir = "/workspace/prod"

[[workspaces.prod.mounts]]
src = "/tmp/prod"
dst = "/workspace/prod"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();
    assert_eq!(
        config.roles["builder"].git,
        "https://example.test/builder.git"
    );
    assert!(config.workspaces.contains_key("prod"));

    let global = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(
        !global.contains("[claude]"),
        "legacy agent table survived: {global}"
    );
    assert!(
        !global.contains("roles.builder.codex") && !global.contains("[roles.builder.codex]"),
        "legacy role agent table survived: {global}"
    );
}
