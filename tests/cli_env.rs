#![cfg(unix)]

//! Integration coverage for the `jackin config env` /
//! `jackin workspace env` CLI verbs against a temp `$HOME`.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::{TempDir, tempdir};

struct Env {
    _temp: TempDir,
    home: PathBuf,
}

fn setup_env() -> Env {
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    Env { _temp: temp, home }
}

fn jackin(env: &Env) -> Command {
    let mut cmd = Command::cargo_bin("jackin").unwrap();
    cmd.env("HOME", &env.home);
    cmd
}

fn config_path(env: &Env) -> PathBuf {
    env.home.join(".config/jackin/config.toml")
}

fn read_config(env: &Env) -> String {
    fs::read_to_string(config_path(env)).unwrap()
}

/// `[agents.<name>]` needs the required `git` field for serde to
/// accept the table.
fn seed_agent(env: &Env, name: &str) {
    let path = config_path(env);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let entry = format!("\n[agents.{name}]\ngit = \"https://example.com/{name}.git\"\n");
    fs::write(&path, format!("{existing}{entry}")).unwrap();
}

fn seed_workspace(env: &Env, name: &str, workdir: &str) {
    fs::create_dir_all(workdir).unwrap();
    jackin(env)
        .args(["workspace", "create", name, "--workdir", workdir])
        .assert()
        .success();
}

#[test]
fn config_env_set_global() {
    let env = setup_env();
    jackin(&env)
        .args([
            "config",
            "env",
            "set",
            "API_TOKEN",
            "op://Personal/api/token",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set API_TOKEN."));
    let contents = read_config(&env);
    assert!(
        contents.contains("[env]"),
        "missing [env] table:\n{contents}"
    );
    assert!(
        contents.contains("API_TOKEN = \"op://Personal/api/token\""),
        "missing API_TOKEN entry:\n{contents}"
    );
}

#[test]
fn config_env_set_with_agent() {
    let env = setup_env();
    seed_agent(&env, "agent-smith");
    jackin(&env)
        .args([
            "config",
            "env",
            "set",
            "LOG_LEVEL",
            "debug",
            "--agent",
            "agent-smith",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set LOG_LEVEL."));
    let contents = read_config(&env);
    assert!(
        contents.contains("[agents.agent-smith.env]"),
        "missing [agents.agent-smith.env]:\n{contents}"
    );
    assert!(
        contents.contains("LOG_LEVEL = \"debug\""),
        "missing LOG_LEVEL entry:\n{contents}"
    );
}

#[test]
fn config_env_set_with_comment() {
    let env = setup_env();
    jackin(&env)
        .args([
            "config",
            "env",
            "set",
            "OPENAI_KEY",
            "op://Work/OpenAI/key",
            "--comment",
            "rotate quarterly",
        ])
        .assert()
        .success();
    let contents = read_config(&env);
    // The comment line must appear above the key within the file.
    let comment_pos = contents
        .find("# rotate quarterly")
        .unwrap_or_else(|| panic!("missing comment line:\n{contents}"));
    let key_pos = contents
        .find("OPENAI_KEY")
        .unwrap_or_else(|| panic!("missing key:\n{contents}"));
    assert!(
        comment_pos < key_pos,
        "comment must precede key; got:\n{contents}"
    );
}

#[test]
fn config_env_unset_removes_key() {
    let env = setup_env();
    jackin(&env)
        .args(["config", "env", "set", "FOO", "bar"])
        .assert()
        .success();
    jackin(&env)
        .args(["config", "env", "unset", "FOO"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed FOO."));
    let contents = read_config(&env);
    assert!(
        !contents.contains("FOO"),
        "FOO should be absent after unset; got:\n{contents}"
    );
}

#[test]
fn config_env_unset_missing_key_exits_0() {
    let env = setup_env();
    jackin(&env)
        .args(["config", "env", "unset", "NO_SUCH_KEY"])
        .assert()
        .success()
        .stdout(predicate::str::contains("NO_SUCH_KEY not set."));
}

#[test]
fn config_env_list_empty() {
    let env = setup_env();
    jackin(&env)
        .args(["config", "env", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No env vars set."));
}

#[test]
fn config_env_list_shows_keys() {
    let env = setup_env();
    jackin(&env)
        .args(["config", "env", "set", "ALPHA", "one"])
        .assert()
        .success();
    jackin(&env)
        .args(["config", "env", "set", "BETA", "two"])
        .assert()
        .success();
    jackin(&env)
        .args(["config", "env", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ALPHA"))
        .stdout(predicate::str::contains("BETA"))
        .stdout(predicate::str::contains("one"))
        .stdout(predicate::str::contains("two"));
}

#[test]
fn workspace_env_set_workspace_scope() {
    let env = setup_env();
    let workdir = env.home.join("Projects/prod");
    seed_workspace(&env, "prod", workdir.to_str().unwrap());
    jackin(&env)
        .args([
            "workspace",
            "env",
            "set",
            "prod",
            "DB_URL",
            "op://Work/Prod/db-url",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Set DB_URL."));
    let contents = read_config(&env);
    assert!(
        contents.contains("[workspaces.prod.env]"),
        "missing [workspaces.prod.env]:\n{contents}"
    );
    assert!(
        contents.contains("DB_URL = \"op://Work/Prod/db-url\""),
        "missing DB_URL entry:\n{contents}"
    );
}

#[test]
fn workspace_env_set_workspace_agent_scope() {
    let env = setup_env();
    let workdir = env.home.join("Projects/prod");
    seed_workspace(&env, "prod", workdir.to_str().unwrap());
    jackin(&env)
        .args([
            "workspace",
            "env",
            "set",
            "prod",
            "OPENAI_KEY",
            "op://Work/OpenAI/key",
            "--agent",
            "agent-smith",
        ])
        .assert()
        .success();
    let contents = read_config(&env);
    assert!(
        contents.contains("[workspaces.prod.agents.agent-smith.env]"),
        "missing [workspaces.prod.agents.agent-smith.env]:\n{contents}"
    );
    assert!(
        contents.contains("OPENAI_KEY = \"op://Work/OpenAI/key\""),
        "missing OPENAI_KEY entry:\n{contents}"
    );
}

#[test]
fn workspace_env_list_unknown_workspace_exits_nonzero() {
    let env = setup_env();
    jackin(&env)
        .args(["workspace", "env", "list", "no-such-ws"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no-such-ws"));
}

// ── brick-the-CLI regressions ───────────────────────────────────

#[test]
fn config_env_set_reserved_name_rejected_with_clear_error() {
    let env = setup_env();
    jackin(&env)
        .args(["config", "env", "set", "DOCKER_HOST", "tcp://bad"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
    let path = config_path(&env);
    if path.exists() {
        let contents = read_config(&env);
        assert!(
            !contents.contains("DOCKER_HOST"),
            "rejected reserved-name set should not have written the entry; got:\n{contents}"
        );
    }
}

#[test]
fn config_env_set_unknown_agent_rejected() {
    let env = setup_env();
    jackin(&env)
        .args([
            "config",
            "env",
            "set",
            "FOO",
            "bar",
            "--agent",
            "ghost-unknown",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ghost-unknown"))
        .stderr(predicate::str::contains("not registered"));
    let path = config_path(&env);
    if path.exists() {
        let contents = read_config(&env);
        assert!(
            !contents.contains("[agents.ghost-unknown]"),
            "rejected unknown-agent set must not have created a stub agent table; got:\n{contents}"
        );
    }
}

/// Same protection for `workspace env set` reserved-name writes.
#[test]
fn workspace_env_set_reserved_name_rejected() {
    let env = setup_env();
    let workdir = env.home.join("Projects/prod");
    seed_workspace(&env, "prod", workdir.to_str().unwrap());
    jackin(&env)
        .args([
            "workspace",
            "env",
            "set",
            "prod",
            "DOCKER_HOST",
            "tcp://bad",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
    let contents = read_config(&env);
    assert!(
        !contents.contains("DOCKER_HOST"),
        "rejected reserved-name workspace-env set must not have written the entry; got:\n{contents}"
    );
}

/// Same protection for `workspace env set --agent <unknown>`.
#[test]
fn workspace_env_set_unknown_agent_rejected() {
    let env = setup_env();
    let workdir = env.home.join("Projects/prod");
    seed_workspace(&env, "prod", workdir.to_str().unwrap());
    jackin(&env)
        .args([
            "workspace",
            "env",
            "set",
            "prod",
            "FOO",
            "bar",
            "--agent",
            "ghost-unknown",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ghost-unknown"))
        .stderr(predicate::str::contains("not registered"));
    let contents = read_config(&env);
    assert!(
        !contents.contains("ghost-unknown"),
        "rejected unknown-agent workspace-env set must not have leaked the agent name on disk; got:\n{contents}"
    );
}
