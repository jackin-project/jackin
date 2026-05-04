#![cfg(unix)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::{TempDir, tempdir};

/// Creates an isolated jackin environment: temp HOME with a pre-populated
/// workspace config. Returns the tempdir (keep alive for the duration of the
/// test) and the host directories used for mount sources.
struct Env {
    _temp: TempDir,
    home: std::path::PathBuf,
    proj_alpha: std::path::PathBuf,
    sub_a: std::path::PathBuf,
    sub_b: std::path::PathBuf,
}

fn setup_env() -> Env {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let proj_alpha = home.join("Projects").join("proj-alpha");
    let sub_a = proj_alpha.join("sub-a");
    let sub_b = proj_alpha.join("sub-b");
    fs::create_dir_all(&sub_a).unwrap();
    fs::create_dir_all(&sub_b).unwrap();
    Env {
        _temp: temp,
        home,
        proj_alpha,
        sub_a,
        sub_b,
    }
}

fn jackin(env: &Env) -> Command {
    let mut cmd = Command::cargo_bin("jackin").unwrap();
    cmd.env("HOME", &env.home);
    cmd
}

fn create_workspace_with_children(env: &Env, name: &str) {
    jackin(env)
        .args([
            "workspace",
            "create",
            name,
            "--workdir",
            env.sub_a.to_str().unwrap(),
            "--mount",
            env.sub_a.to_str().unwrap(),
            "--mount",
            env.sub_b.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn workspace_create_auto_collapses_and_prints_summary() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            env.proj_alpha.to_str().unwrap(),
            "--mount",
            env.proj_alpha.to_str().unwrap(),
            "--mount",
            env.sub_a.to_str().unwrap(),
            "--mount",
            env.sub_b.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("collapsed"))
        .stderr(predicate::str::contains("sub-a"))
        .stderr(predicate::str::contains("sub-b"));
}

#[test]
fn workspace_create_workdir_parent_does_not_collapse_child_mount() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            env.proj_alpha.to_str().unwrap(),
            "--mount",
            env.sub_a.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("collapsed").not());

    jackin(&env)
        .args(["workspace", "show", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sub-a"))
        .stdout(predicate::str::contains("proj-alpha"));
}

#[test]
fn workspace_create_rejects_duplicate_destination() {
    // Two `--mount` entries with the same explicit `dst` must fail loudly
    // at the validator (`workspace::mounts.rs`'s "duplicate mount
    // destination" bail) rather than silently collapsing one of them.
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            env.proj_alpha.to_str().unwrap(),
            "--mount",
            &format!("{}:/work", env.sub_a.to_str().unwrap()),
            "--mount",
            &format!("{}:/work", env.sub_b.to_str().unwrap()),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate mount destination"));
}

#[test]
fn workspace_create_accepts_src_colon_dst_spec() {
    // `--mount src:/dst` should parse and persist the explicit container
    // path. Pre-existing tests only exercise the `src` and `src:ro`
    // shapes — this pins the third documented form. `--workdir` matches
    // the explicit dst so the workdir-vs-dst validator is satisfied.
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            "/code",
            "--mount",
            &format!("{}:/code", env.sub_a.to_str().unwrap()),
        ])
        .assert()
        .success();

    jackin(&env)
        .args(["workspace", "show", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("/code"));
}

#[test]
fn workspace_create_rejects_duplicate_name() {
    // Creating a workspace with a name that already exists must fail
    // loudly — silently overwriting the existing config would lose the
    // operator's prior mount/env/workdir setup.
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            env.sub_a.to_str().unwrap(),
            "--mount",
            env.sub_a.to_str().unwrap(),
        ])
        .assert()
        .success();

    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            env.sub_b.to_str().unwrap(),
            "--mount",
            env.sub_b.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn workspace_create_rejects_readonly_mismatch() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            env.proj_alpha.to_str().unwrap(),
            "--mount",
            env.proj_alpha.to_str().unwrap(),
            "--mount",
            &format!("{}:ro", env.sub_a.to_str().unwrap()),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("readonly"));
}

#[test]
fn workspace_edit_with_yes_collapses_children_under_new_parent() {
    let env = setup_env();
    create_workspace_with_children(&env, "test");
    jackin(&env)
        .args([
            "workspace",
            "edit",
            "test",
            "--mount",
            env.proj_alpha.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("collapsed"));

    jackin(&env)
        .args(["workspace", "show", "test"])
        .assert()
        .success()
        // proj-alpha must appear as a mount source after collapse
        .stdout(predicate::str::contains("proj-alpha"))
        // sub-b was a mount that got collapsed — must not appear in the mount table
        // (sub-a is the workdir so it still appears in the Workdir row; we only
        // verify it is gone as a standalone mount by checking sub-b is absent)
        .stdout(predicate::str::contains("sub-b").not());
}

#[test]
fn workspace_edit_fails_on_readonly_mismatch_with_clear_error() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            env.sub_a.to_str().unwrap(),
            "--mount",
            &format!("{}:ro", env.sub_a.to_str().unwrap()),
        ])
        .assert()
        .success();

    jackin(&env)
        .args([
            "workspace",
            "edit",
            "test",
            "--mount",
            env.proj_alpha.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("readonly"));
}

#[test]
fn workspace_edit_fails_on_child_under_existing_parent() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            env.proj_alpha.to_str().unwrap(),
            "--mount",
            env.proj_alpha.to_str().unwrap(),
        ])
        .assert()
        .success();

    jackin(&env)
        .args([
            "workspace",
            "edit",
            "test",
            "--mount",
            env.sub_a.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already covered"));
}

#[test]
fn workspace_edit_non_tty_without_yes_bails() {
    let env = setup_env();
    create_workspace_with_children(&env, "test");
    // assert_cmd does not attach a TTY by default — stdin is not a terminal.
    jackin(&env)
        .args([
            "workspace",
            "edit",
            "test",
            "--mount",
            env.proj_alpha.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("without confirmation"));
}

fn seed_legacy_config_with_violation(env: &Env) {
    let config_path = env.home.join(".config/jackin/config.toml");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    let proj = env.proj_alpha.to_str().unwrap();
    let sub_a = env.sub_a.to_str().unwrap();
    fs::write(
        &config_path,
        format!(
            r#"
[workspaces.test]
workdir = "{proj}"

[[workspaces.test.mounts]]
src = "{proj}"
dst = "{proj}"
readonly = false

[[workspaces.test.mounts]]
src = "{sub_a}"
dst = "{sub_a}"
readonly = false
"#
        ),
    )
    .unwrap();
}

#[test]
fn workspace_prune_removes_pre_existing_redundants() {
    let env = setup_env();
    seed_legacy_config_with_violation(&env);

    jackin(&env)
        .args(["workspace", "prune", "test", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pruned"));

    jackin(&env)
        .args(["workspace", "show", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sub-a").not());
}

#[test]
fn workspace_prune_on_clean_workspace_is_noop() {
    let env = setup_env();
    jackin(&env)
        .args([
            "workspace",
            "create",
            "test",
            "--workdir",
            env.proj_alpha.to_str().unwrap(),
            "--mount",
            env.proj_alpha.to_str().unwrap(),
        ])
        .assert()
        .success();

    jackin(&env)
        .args(["workspace", "prune", "test", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no redundant mounts"));
}

#[test]
fn workspace_edit_rejects_pre_existing_violation_without_prune() {
    let env = setup_env();
    seed_legacy_config_with_violation(&env);

    jackin(&env)
        .args(["workspace", "edit", "test", "--allowed-role", "some-role"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("redundant"))
        .stderr(predicate::str::contains("prune"));
}

#[test]
fn workspace_edit_with_prune_cleans_pre_existing_violations() {
    let env = setup_env();
    seed_legacy_config_with_violation(&env);

    jackin(&env)
        .args([
            "workspace",
            "edit",
            "test",
            "--allowed-role",
            "some-role",
            "--prune",
            "--yes",
        ])
        .assert()
        .success();

    jackin(&env)
        .args(["workspace", "show", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sub-a").not());
}
