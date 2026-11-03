// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::time::Duration;

use jackin_xtask::{
    Exit, HostCommand, HostError, HostProcess, RealHost, Scratch, child_env, snapshot_tree,
};
use rustix::process::{Pid, test_kill_process};
use tokio_util::sync::CancellationToken;

use super::scenario::Scenario;
use super::scenario::world::ACCESS;
use super::support::capture;

fn parent() -> BTreeMap<String, String> {
    [
        ("HOME", "/home/real"),
        ("PATH", "/usr/bin:/bin"),
        ("TMPDIR", "/var/tmp"),
        ("USER", "tester"),
        ("LANG", "C"),
        ("DOCKER_HOST", "unix:///docker.sock"),
        ("DOCKER_CONFIG", "/docker-config"),
        ("DOCKER_CONTEXT", "ctx"),
        ("SSH_AUTH_SOCK", "/ssh.sock"),
        ("OPENSHELL_GATEWAY_ENDPOINT", "https://leak.invalid"),
        ("OPENSHELL_GATEWAY_INSECURE", "1"),
        ("JACKIN_X", "1"),
        ("GIT_DIR", "/leak/.git"),
        ("OPENAI_API_KEY", "sk-leak"),
        ("CODEX_HOME", "/leak/codex"),
    ]
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .into()
}

#[tokio::test]
async fn child_environment_is_an_allowlist() {
    let dir = tempfile::tempdir().expect("tempdir");
    let scratch = Scratch::create(&dir.path().join("scratch")).expect("scratch");
    let env = child_env(&parent(), &scratch, false);
    let command = HostCommand::new("/usr/bin/env", Duration::from_secs(5)).env(env);
    let output = RealHost::default()
        .run(&command, &CancellationToken::new())
        .await
        .expect("env");
    let seen: BTreeMap<String, String> = output
        .stdout_text()
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();
    let handed = parent();
    for name in [
        "HOME",
        "PATH",
        "TMPDIR",
        "USER",
        "LANG",
        "DOCKER_HOST",
        "DOCKER_CONFIG",
        "DOCKER_CONTEXT",
        "SSH_AUTH_SOCK",
    ] {
        assert_eq!(seen.get(name), handed.get(name), "{name} is handed through");
    }
    for leak in [
        "OPENSHELL_GATEWAY_ENDPOINT",
        "OPENSHELL_GATEWAY_INSECURE",
        "JACKIN_X",
        "GIT_DIR",
        "OPENAI_API_KEY",
        "CODEX_HOME",
    ] {
        assert!(!seen.contains_key(leak), "{leak} must not reach a child");
    }
    for name in ["XDG_CONFIG_HOME", "XDG_CACHE_HOME"] {
        assert!(
            seen[name].starts_with(&scratch.root().display().to_string()),
            "{name} points into the scratch directory"
        );
    }
}

#[tokio::test]
async fn scratch_config_is_isolated_and_the_real_registry_is_not_written() {
    let scenario = Scenario::new().await;
    scenario.knobs().observe_symlink = true;
    let before = snapshot_tree(&scenario.world.registry);
    let ran = scenario.run().await;
    ran.expect("an untouched registry", &[]);
    assert_eq!(
        snapshot_tree(&scenario.world.registry),
        before,
        "listing and mtimes"
    );
    assert!(!scenario.world.registry.join("active_gateway").exists());
    assert!(
        !scenario.world.home.join(".config/jackin").exists(),
        "only the scratch config is written"
    );
    let links = scenario.state().links.clone();
    assert_eq!(
        links,
        std::slice::from_ref(&scenario.world.registry),
        "the scratch openshell directory links to the real one"
    );

    let writer = Scenario::new().await;
    writer.knobs().registry_write = true;
    let ran = writer.run().await;
    ran.expect("a write into the symlinked registry", &[("CLEAN", "FAIL")]);
    ran.reason_has("registry", "CLEAN", "registry");
}

#[tokio::test]
async fn fixture_projects_have_the_shapes_journey_steps_need() {
    let dir = tempfile::tempdir().expect("tempdir");
    let scratch = Scratch::create(&dir.path().join("scratch")).expect("scratch");
    let mut env = BTreeMap::new();
    env.insert("PATH".to_owned(), std::env::var("PATH").unwrap_or_default());
    env.insert("HOME".to_owned(), dir.path().display().to_string());
    let project = scratch
        .fixture_project(&RealHost::default(), &env, "p1", &CancellationToken::new())
        .await
        .expect("fixture");
    let git = |args: &[&str]| {
        let output = capture(
            std::process::Command::new("git")
                .args(args)
                .current_dir(&project),
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    assert_eq!(
        git(&["status", "--porcelain"]),
        "?? untracked.txt\n",
        "tracked files are clean"
    );
    let listed: Vec<String> = git(&["ls-files", "-co", "--exclude-standard", "-z"])
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(
        listed.contains(&"notes and docs/guide.txt".to_owned()),
        "a tracked file in a directory with a space"
    );
    assert!(listed.contains(&"untracked.txt".to_owned()));
    assert!(
        !listed.contains(&"ignored.log".to_owned()),
        "the ignored file is not listed"
    );
    assert!(project.join("ignored.log").exists());
    assert!(
        git(&["ls-files", "-s", "bin/run.sh"]).starts_with("100755"),
        "an executable file"
    );
}

#[tokio::test]
async fn scratch_is_removed_even_when_the_setup_fails() {
    let no_login = Scenario::new().await;
    fs::remove_file(no_login.world.codex_home.join("auth.json")).expect("remove");
    let ran = no_login.run().await;
    assert_eq!(ran.status.code(), 2);
    assert!(
        !no_login.world.scratch_root.join("abcd").exists(),
        "no scratch without a login"
    );

    let mut missing_role = Scenario::new().await;
    missing_role.args.role = Some(missing_role.world.root.join("no-such-role"));
    let ran = missing_role.run().await;
    assert_eq!(ran.status.code(), 2);
    assert!(
        ran.stderr.contains("no-such-role"),
        "the message names the path: {}",
        ran.stderr
    );

    let broken = Scenario::new().await;
    broken.host_rule("gateway add", None, 1, &format!("cannot add {ACCESS}"));
    let ran = broken.run().await;
    assert_eq!(ran.status.code(), 2, "{}", ran.stderr);
    assert!(
        !broken.world.scratch_root.join("abcd").exists(),
        "no scratch after a config failure"
    );
    let logs: Vec<String> = fs::read_dir(&broken.world.log_dir)
        .expect("the log directory remains")
        .flatten()
        .map(|entry| fs::read_to_string(entry.path()).unwrap_or_default())
        .collect();
    assert!(!logs.is_empty(), "the redacted logs remain");
    assert!(logs.iter().all(|text| !text.contains(ACCESS)));
    assert!(
        !broken
            .fake
            .providers()
            .iter()
            .any(|name| name.as_str().contains("e2e")),
        "no provider was left"
    );
}

fn pid_alive(pid: i32) -> bool {
    Pid::from_raw(pid).is_some_and(|pid| test_kill_process(pid).is_ok())
}

async fn wait_gone(pid: i32) -> bool {
    for _ in 0..100 {
        if !pid_alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

fn sh(script: &str, timeout: Duration) -> HostCommand {
    let mut env = BTreeMap::new();
    env.insert("PATH".to_owned(), "/usr/bin:/bin".to_owned());
    HostCommand::new("/bin/sh", timeout)
        .args(["-c", script])
        .env(env)
}

#[tokio::test]
async fn real_host_kills_the_whole_process_group_on_timeout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pidfile = dir.path().join("pid");
    let script = format!("sleep 600 & echo $! > {}; sleep 600", pidfile.display());
    let started = tokio::time::Instant::now();
    let result = RealHost::default()
        .run(
            &sh(&script, Duration::from_secs(1)),
            &CancellationToken::new(),
        )
        .await;
    assert!(
        matches!(result, Err(HostError::Timeout { .. })),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    let pid: i32 = fs::read_to_string(&pidfile)
        .expect("pid")
        .trim()
        .parse()
        .expect("number");
    assert!(
        wait_gone(pid).await,
        "the background process of the group is gone"
    );

    let host = RealHost::default();
    let none = CancellationToken::new();
    let ok = host
        .run(&sh("echo out; echo err >&2", Duration::from_secs(5)), &none)
        .await
        .expect("ok");
    assert_eq!(
        (
            ok.exit,
            ok.stdout_text().as_str(),
            ok.stderr_text().as_str()
        ),
        (Exit::Code(0), "out\n", "err\n")
    );
    let seven = host
        .run(&sh("exit 7", Duration::from_secs(5)), &none)
        .await
        .expect("seven");
    assert_eq!(seven.exit, Exit::Code(7));
    let killed = host
        .run(&sh("kill -9 $$", Duration::from_secs(5)), &none)
        .await
        .expect("killed");
    assert_eq!(killed.exit, Exit::Signal(9));
    let quick = host
        .run(&sh("cat; echo done", Duration::from_secs(5)), &none)
        .await
        .expect("cat");
    assert_eq!(quick.stdout_text(), "done\n", "stdin is /dev/null");

    let token = CancellationToken::new();
    let script = format!("sleep 600 & echo $! > {}; sleep 600", pidfile.display());
    let trigger = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        trigger.cancel();
    });
    let result = host
        .run(&sh(&script, Duration::from_secs(30)), &token)
        .await;
    assert!(
        matches!(result, Err(HostError::Cancelled { .. })),
        "{result:?}"
    );
    let pid: i32 = fs::read_to_string(&pidfile)
        .expect("pid")
        .trim()
        .parse()
        .expect("number");
    assert!(wait_gone(pid).await, "cancel kills the group");
}

#[tokio::test]
async fn real_host_caps_captured_output() {
    let command = sh("head -c 3145728 /dev/zero", Duration::from_secs(10)).output_cap(1024 * 1024);
    let result = RealHost::default()
        .run(&command, &CancellationToken::new())
        .await;
    match result {
        Err(HostError::OutputTooLarge { stream, limit, .. }) => {
            assert_eq!(stream, "stdout");
            assert_eq!(limit, 1024 * 1024);
        }
        other => panic!("expected the cap to fire, got {other:?}"),
    }
}
