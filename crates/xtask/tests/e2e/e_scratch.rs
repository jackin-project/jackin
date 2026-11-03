// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{GroupFault, ProcessError, ProcessRunner, ProcessSpec, ProcessStdin, Program};
use jackin_testkit::guard_within;
use jackin_xtask::{
    Exit, HostCommand, HostError, HostOutput, HostProcess, RealHost, Scratch, ToolRunner,
    child_env, snapshot_tree,
};
use rustix::process::{
    Pid, Signal, WaitId, WaitIdOptions, kill_process, test_kill_process, waitid,
};
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
            jackin_testkit::git_command()
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

struct KillRecorded(std::path::PathBuf);

impl Drop for KillRecorded {
    fn drop(&mut self) {
        let recorded = fs::read_to_string(&self.0)
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok())
            .and_then(Pid::from_raw);
        if let Some(pid) = recorded {
            kill_process(pid, Signal::KILL).unwrap_or_default();
        }
    }
}

async fn bounded_run(
    host: &RealHost,
    command: &HostCommand,
    cancel: &CancellationToken,
) -> Result<HostOutput, HostError> {
    let mut result = None;
    guard_within(Duration::from_secs(15), async {
        result = Some(host.run(command, cancel).await);
        Ok(())
    })
    .await
    .expect("the run did not return");
    result.expect("the run finished")
}

#[tokio::test]
async fn real_host_kills_the_whole_process_group_on_timeout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pidfile = dir.path().join("pid");
    let _reap = KillRecorded(pidfile.clone());
    let script = format!("sleep 600 & echo $! > {}; sleep 600", pidfile.display());
    let started = tokio::time::Instant::now();
    let result = bounded_run(
        &RealHost::default(),
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
    let result = bounded_run(&host, &sh(&script, Duration::from_secs(30)), &token).await;
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

const GRACE: Duration = Duration::from_secs(5);
const CAP: usize = 1024 * 1024;

struct Held(Child);

impl Drop for Held {
    fn drop(&mut self) {
        drop(self.0.kill());
        drop(self.0.wait());
    }
}

fn zombie_left() -> Option<GroupFault> {
    if cfg!(target_os = "linux") {
        None
    } else {
        Some(GroupFault::Alive { after: GRACE })
    }
}

async fn group_of(pidfile: &Path) -> Result<i32, Box<dyn Error>> {
    for _ in 0..1000 {
        if let Some(group) = fs::read_to_string(pidfile)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return Ok(group);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("the script never wrote its group id".into())
}

async fn exited_unreaped(zombie: &Held) -> Result<(), Box<dyn Error>> {
    let pid = Pid::from_raw(i32::try_from(zombie.0.id())?).ok_or("the zombie has no pid")?;
    for _ in 0..1000 {
        let options = WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG;
        if waitid(WaitId::Pid(pid), options)?.is_some() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("the zombie never exited".into())
}

#[derive(Clone, Copy)]
enum Ending {
    OutputCap,
    TimeOut,
    Cancel,
}

async fn run_with_a_zombie(
    ending: Ending,
) -> Result<Result<HostOutput, HostError>, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let pidfile = dir.path().join("pid");
    let go = dir.path().join("go");
    let script = match ending {
        Ending::OutputCap => format!(
            "echo $$ > {}; while [ ! -e {} ]; do sleep 0.05; done; head -c 3145728 /dev/zero",
            pidfile.display(),
            go.display()
        ),
        Ending::TimeOut | Ending::Cancel => {
            format!("echo $$ > {}; sleep 600", pidfile.display())
        }
    };
    let timeout = match ending {
        Ending::TimeOut => Duration::from_secs(3),
        Ending::OutputCap | Ending::Cancel => Duration::from_secs(60),
    };
    let command = sh(&script, timeout).output_cap(CAP);
    let cancel = CancellationToken::new();
    let driver = async {
        let group = group_of(&pidfile).await?;
        let zombie = Held(
            Command::new("sh")
                .args(["-c", "exit 0"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .process_group(group)
                .spawn()?,
        );
        exited_unreaped(&zombie).await?;
        match ending {
            Ending::OutputCap => fs::write(&go, b"")?,
            Ending::TimeOut => {}
            Ending::Cancel => cancel.cancel(),
        }
        Ok::<_, Box<dyn Error>>(zombie)
    };
    let host = RealHost::default();
    let (result, zombie) = tokio::join!(bounded_run(&host, &command, &cancel), driver);
    let mut zombie = zombie?;
    match zombie.0.try_wait() {
        Ok(Some(status)) if status.success() => Ok(result),
        other => Err(format!("the host reaped the zombie or it failed: {other:?}").into()),
    }
}

#[tokio::test]
async fn a_failed_group_stop_after_too_much_output_is_a_group_error_and_never_swallowed() {
    let result = run_with_a_zombie(Ending::OutputCap).await.expect("the run");
    match (result, zombie_left()) {
        (Err(HostError::OutputTooLarge { stream, limit, .. }), None) => {
            assert_eq!((stream, limit), ("stdout", CAP));
        }
        (Err(HostError::Group { program, fault }), Some(alive)) => {
            assert_eq!(program, "/bin/sh");
            assert_eq!(fault, alive);
        }
        (other, expected) => panic!("expected {expected:?} as the fault, got {other:?}"),
    }
}

#[tokio::test]
async fn a_timeout_that_leaves_only_a_zombie_in_the_group_keeps_the_timeout_and_its_fault() {
    let result = run_with_a_zombie(Ending::TimeOut).await.expect("the run");
    match result {
        Err(HostError::Timeout {
            program,
            after,
            group,
        }) => {
            assert_eq!(program, "/bin/sh");
            assert_eq!(after, Duration::from_secs(3));
            assert_eq!(group, zombie_left());
        }
        other => panic!("expected Timeout, got {other:?}"),
    }
}

#[tokio::test]
async fn a_cancel_that_leaves_only_a_zombie_in_the_group_keeps_the_cancel_and_its_fault() {
    let result = run_with_a_zombie(Ending::Cancel).await.expect("the run");
    match result {
        Err(HostError::Cancelled { program, group }) => {
            assert_eq!(program, "/bin/sh");
            assert_eq!(group, zombie_left());
        }
        other => panic!("expected Cancelled, got {other:?}"),
    }
}

#[derive(Debug)]
struct Failing(Mutex<Option<HostError>>);

#[async_trait]
impl HostProcess for Failing {
    async fn run(
        &self,
        _command: &HostCommand,
        _cancel: &CancellationToken,
    ) -> Result<HostOutput, HostError> {
        Err(self
            .0
            .lock()
            .expect("lock")
            .take()
            .expect("one error per run"))
    }
}

async fn tool_run(error: HostError) -> Result<jackin_core::ProcessOutput, ProcessError> {
    let host = Arc::new(Failing(Mutex::new(Some(error))));
    let spec = ProcessSpec {
        program: Program::Git,
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: Vec::new(),
        stdin: ProcessStdin::Null,
        timeout: Duration::from_secs(9),
    };
    ToolRunner::new(host, BTreeMap::new())
        .run(&spec, None, &CancellationToken::new())
        .await
}

#[tokio::test]
async fn a_tool_run_hands_the_group_fault_of_the_host_to_the_process_error() {
    let alive = GroupFault::Alive { after: GRACE };
    let lost = GroupFault::Unreachable {
        message: "cannot list /proc".to_owned(),
    };
    let program = || "git".to_owned();

    match tool_run(HostError::Timeout {
        program: program(),
        after: Duration::from_secs(4),
        group: Some(alive.clone()),
    })
    .await
    {
        Err(ProcessError::Timeout {
            program,
            after,
            group,
        }) => {
            assert_eq!(program, Program::Git);
            assert_eq!(after, Duration::from_secs(4));
            assert_eq!(group, Some(alive.clone()), "a timeout keeps its fault");
        }
        other => panic!("expected Timeout, got {other:?}"),
    }
    match tool_run(HostError::Timeout {
        program: program(),
        after: Duration::from_secs(4),
        group: None,
    })
    .await
    {
        Err(ProcessError::Timeout { group, .. }) => {
            assert_eq!(group, None, "a clean stop has no fault");
        }
        other => panic!("expected Timeout, got {other:?}"),
    }
    match tool_run(HostError::Cancelled {
        program: program(),
        group: Some(lost.clone()),
    })
    .await
    {
        Err(ProcessError::Cancelled { program, group }) => {
            assert_eq!(program, Program::Git);
            assert_eq!(group, Some(lost.clone()), "a cancel keeps its fault");
        }
        other => panic!("expected Cancelled, got {other:?}"),
    }
    match tool_run(HostError::Cancelled {
        program: program(),
        group: None,
    })
    .await
    {
        Err(ProcessError::Cancelled { group, .. }) => {
            assert_eq!(group, None, "a clean stop has no fault");
        }
        other => panic!("expected Cancelled, got {other:?}"),
    }
    match tool_run(HostError::Group {
        program: program(),
        fault: alive.clone(),
    })
    .await
    {
        Err(ProcessError::Group { program, fault }) => {
            assert_eq!(program, Program::Git);
            assert_eq!(fault, alive, "a group error keeps its fault");
        }
        other => panic!("expected Group, got {other:?}"),
    }
}
