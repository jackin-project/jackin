// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use jackin_core::Agent;
use tempfile::TempDir;
use tokio::time::{Instant, sleep};

type TestResult = Result<(), Box<dyn Error>>;

const SCRIPT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/images/jackin-start"));
const POLL: Duration = Duration::from_millis(50);
const STEP: Duration = Duration::from_secs(10);
const QUIET_POLLS: u32 = 7;
const LAUNCH: [&str; 5] = [
    "codex",
    "--sandbox",
    "danger-full-access",
    "-c",
    "check_for_update_on_startup=false",
];
const FAKE_CODEX: &str = "#!/bin/sh\nprintf 'REAL %s\\n' \"$*\" >> \"$LOG\"\n/usr/bin/awk 'BEGIN { for (n in ENVIRON) { v = ENVIRON[n]; gsub(/\\n/, \"<NL>\", v); print n \"\\t\" v } }' > \"$DUMP\"\nexit 0\n";

fn substitute(text: &str, from: &str, to: &str) -> Result<String, Box<dyn Error>> {
    let count = text.matches(from).count();
    if count != 1 {
        return Err(format!("{from:?} matched {count} times in jackin-start").into());
    }
    Ok(text.replacen(from, to, 1))
}

struct Supervisor {
    _dir: TempDir,
    root: PathBuf,
    child: Option<Child>,
}

struct Setup<'a> {
    env: Vec<(&'a str, &'a str)>,
    env_file: Option<&'a str>,
    args: Vec<&'a str>,
    slow_record: bool,
    missing_awk: bool,
}

impl<'a> Setup<'a> {
    fn new() -> Setup<'a> {
        Setup {
            env: vec![("HOME", "/h")],
            env_file: None,
            args: LAUNCH.to_vec(),
            slow_record: false,
            missing_awk: false,
        }
    }
}

impl Supervisor {
    fn prepare(setup: &Setup<'_>) -> Result<(TempDir, PathBuf), Box<dyn Error>> {
        let dir = TempDir::new()?;
        let root = dir.path().canonicalize()?;
        for sub in [
            "sandbox/.jackin/run",
            "sandbox/project",
            "bin",
            "evil",
            "marks",
        ] {
            fs::create_dir_all(root.join(sub))?;
        }
        if let Some(text) = setup.env_file {
            fs::write(
                root.join("sandbox/.jackin/env-L1"),
                text.replace("@ROOT@", &root.display().to_string()),
            )?;
        }
        install(&root.join("bin/codex"), FAKE_CODEX)?;
        Ok((dir, root))
    }

    fn spawn(setup: Setup<'_>, dir: TempDir, root: PathBuf) -> Result<Supervisor, Box<dyn Error>> {
        let r = root.display().to_string();
        let mut text = SCRIPT.to_owned();
        let mut replacements = vec![
            (
                "JACKIN_RUN=/sandbox/.jackin/run\n",
                format!("JACKIN_RUN={r}/sandbox/.jackin/run\n"),
            ),
            (
                "JACKIN_PROJECT=/sandbox/project\n",
                format!("JACKIN_PROJECT={r}/sandbox/project\n"),
            ),
            (
                "JACKIN_BIN_DIR=/usr/local/bin\n",
                format!("JACKIN_BIN_DIR={r}/bin\n"),
            ),
            (
                "JACKIN_ENVIRON_FILE=/proc/$$/environ\n",
                format!("JACKIN_ENVIRON_FILE={r}/environ\n"),
            ),
            (
                "JACKIN_ENVFILE=/sandbox/.jackin/env-$",
                format!("JACKIN_ENVFILE={r}/sandbox/.jackin/env-$"),
            ),
            ("sleep 0.5", format!("sleep {}", POLL.as_secs_f64())),
        ];
        if setup.slow_record {
            install(
                &root.join("slowmv"),
                "#!/bin/sh\nsleep 0.4\nexec /bin/mv \"$@\"\n",
            )?;
            replacements.push(("/bin/mv -f", format!("{r}/slowmv -f")));
        }
        if setup.missing_awk {
            replacements.push(("/usr/bin/awk", format!("{r}/no-such-awk")));
        }
        for (from, to) in replacements {
            text = substitute(&text, from, &to)?;
        }
        fs::write(root.join("jackin-start"), text)?;
        let mut command = Command::new("/bin/sh");
        command
            .arg(root.join("jackin-start"))
            .args(&setup.args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LC_ALL", "C")
            .env("JACKIN_LAUNCH_ID", "L1")
            .env("LOG", root.join("log"))
            .env("DUMP", root.join("dump"))
            .envs(setup.env.iter().copied())
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(fs::File::create(root.join("stdout"))?)
            .stderr(fs::File::create(root.join("stderr"))?)
            .process_group(0);
        let child = command.spawn()?;
        Ok(Supervisor {
            _dir: dir,
            root,
            child: Some(child),
        })
    }

    fn start(setup: Setup<'_>) -> Result<Supervisor, Box<dyn Error>> {
        let (dir, root) = Supervisor::prepare(&setup)?;
        Supervisor::spawn(setup, dir, root)
    }

    fn run_dir(&self) -> PathBuf {
        self.root.join("sandbox/.jackin/run")
    }

    fn state(&self) -> String {
        fs::read_to_string(self.run_dir().join("state")).unwrap_or_default()
    }

    fn record(&self) -> Option<String> {
        fs::read_to_string(self.run_dir().join("env-kinds")).ok()
    }

    fn runs(&self) -> usize {
        fs::read_to_string(self.root.join("log")).map_or(0, |log| log.lines().count())
    }

    fn alive(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }

    async fn wait_until(
        &self,
        limit: Duration,
        want: &str,
        done: impl Fn(&str) -> bool,
    ) -> TestResult {
        let deadline = Instant::now() + limit;
        while !done(self.state().trim_end()) {
            if Instant::now() > deadline {
                return Err(format!("state {:?}, wanted {want:?}", self.state()).into());
            }
            sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }

    async fn wait_state(&self, want: &str) -> TestResult {
        self.wait_until(STEP, want, |state| state == want).await
    }

    fn request(&self, id: &str) -> TestResult {
        let run = self.run_dir();
        fs::write(run.join("start.tmp"), format!("{id}\n"))?;
        fs::rename(run.join("start.tmp"), run.join("start"))?;
        Ok(())
    }

    async fn attempt(&self, id: u32) -> TestResult {
        if id == 1 {
            self.wait_state("waiting").await?;
        }
        self.request(&id.to_string())?;
        self.wait_state(&format!("exited {id} 0")).await
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let group = format!("-{}", child.id());
            drop(Command::new("kill").args(["-KILL", "--", &group]).status());
            drop(child.kill());
            drop(child.wait());
        }
    }
}

fn install(path: &Path, body: &str) -> TestResult {
    fs::write(path, body)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

const MAIN_LINES: &str = "main AWS_SECRET_ACCESS_KEY other\nmain GITHUB_TOKEN empty\nmain MULTI_KEY other\nmain OPENAI_API_KEY placeholder\n";
const ENV_FILE: &str = "export CODEX_AUTH_ACCESS_TOKEN='openshell:resolve:env:CODEX_AUTH_ACCESS_TOKEN'\nexport STRIPE_SECRET='sk-live-FAKE'\n";

fn golden_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("HOME", "/h"),
        ("OPENAI_API_KEY", "openshell:resolve:env:OPENAI_API_KEY"),
        ("GITHUB_TOKEN", ""),
        ("AWS_SECRET_ACCESS_KEY", "AKIAFAKEFAKEFAKE"),
        ("MULTI_KEY", "openshell:resolve:env:X\nreal"),
        ("SSH_AUTH_SOCK", "/x"),
    ]
}

#[tokio::test]
async fn jackin_start_records_credential_variable_names_and_kinds_of_both_environments()
-> TestResult {
    let mut setup = Setup::new();
    setup.env = golden_env();
    setup.env_file = Some(ENV_FILE);
    setup.slow_record = true;
    let (dir, root) = Supervisor::prepare(&setup)?;
    fs::write(root.join("sandbox/.jackin/run/env-kinds"), "stale\n")?;
    let supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.wait_state("waiting").await?;
    assert_eq!(
        supervisor.record().as_deref(),
        Some(format!("jackin-env-kinds 2\nattempt 0\n{MAIN_LINES}").as_str()),
        "the record at the moment the state reads waiting"
    );
    supervisor.attempt(1).await?;
    let codex = "agent AWS_SECRET_ACCESS_KEY other\nagent CODEX_AUTH_ACCESS_TOKEN placeholder\nagent GITHUB_TOKEN empty\nagent MULTI_KEY other\nagent OPENAI_API_KEY placeholder\nagent STRIPE_SECRET other\n";
    assert_eq!(
        supervisor.record().as_deref(),
        Some(format!("jackin-env-kinds 2\nattempt 1\n{MAIN_LINES}{codex}").as_str())
    );
    Ok(())
}

#[tokio::test]
async fn jackin_start_names_credential_variables_by_the_shared_rule() -> TestResult {
    let yes = [
        "OPENAI_API_KEY",
        "KEY",
        "MY_KEY_2",
        "AUTH",
        "PASSWORD",
        "CREDENTIAL",
        "CREDENTIALS",
        "X_TOKEN",
        "DB_PASSWD",
        "A_SECRET_B",
    ];
    let no = [
        "KEYBOARD",
        "TOKENIZER",
        "OAUTH_X",
        "SSH_AUTH_SOCK",
        "openai_api_key",
    ];
    let mut setup = Setup::new();
    setup.env = yes
        .iter()
        .chain(no.iter())
        .map(|name| (*name, "v"))
        .collect();
    let (dir, root) = Supervisor::prepare(&setup)?;
    fs::write(root.join("environ"), b"P73-API-KEY=hidden\0HOME=/h\0")?;
    let supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.wait_state("waiting").await?;
    let record = supervisor.record().ok_or("no record")?;
    let mut named: Vec<&str> = record
        .lines()
        .filter_map(|line| line.strip_prefix("main "))
        .filter_map(|rest| rest.split_once(' ').map(|(name, _)| name))
        .collect();
    named.sort_unstable();
    let mut wanted: Vec<&str> = yes.to_vec();
    wanted.push("INVALID_NAME");
    wanted.sort_unstable();
    assert_eq!(named, wanted, "record:\n{record}");
    assert!(record.contains("main INVALID_NAME other\n"), "{record}");
    Ok(())
}

#[tokio::test]
async fn jackin_start_record_never_holds_a_value() -> TestResult {
    let mut setup = Setup::new();
    setup.env = golden_env();
    setup.env_file = Some(ENV_FILE);
    let supervisor = Supervisor::start(setup)?;
    supervisor.attempt(1).await?;
    supervisor.attempt(2).await?;
    let secrets = [
        "AKIAFAKEFAKEFAKE",
        "sk-live-FAKE",
        "openshell:resolve:env",
        "real",
    ];
    let mut seen = 0;
    for entry in fs::read_dir(supervisor.run_dir())? {
        let path = entry?.path();
        let body = fs::read_to_string(&path)?;
        seen += 1;
        for secret in secrets {
            assert!(!body.contains(secret), "{secret} in {}", path.display());
        }
    }
    assert!(seen >= 2, "the run directory held {seen} files");
    for stream in ["stdout", "stderr"] {
        let body = fs::read_to_string(supervisor.root.join(stream))?;
        for secret in secrets {
            assert!(!body.contains(secret), "{secret} in {stream}");
        }
    }
    Ok(())
}

fn classify(dump: &str) -> Result<Vec<String>, regex::Error> {
    let credential =
        regex::Regex::new(r"(^|_)(KEY|TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIALS?|AUTH)(_|$)")?;
    let mut lines: Vec<String> = dump
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(name, _)| *name != "SSH_AUTH_SOCK" && credential.is_match(name))
        .map(|(name, value)| {
            let kind = if value.is_empty() {
                "empty"
            } else if value.starts_with("openshell:resolve:") && !value.contains("<NL>") {
                "placeholder"
            } else {
                "other"
            };
            format!("agent {name} {kind}")
        })
        .collect();
    lines.sort();
    Ok(lines)
}

#[tokio::test]
async fn jackin_start_record_describes_the_environment_codex_really_receives() -> TestResult {
    let mut setup = Setup::new();
    setup.env = golden_env();
    setup.env.push(("P_TOKEN", "real-looking"));
    setup.env_file = Some(
        "export P_TOKEN=''\nexport CODEX_AUTH_ACCESS_TOKEN='openshell:resolve:env:C'\nexport STRIPE_SECRET='sk-live-FAKE'\n",
    );
    let supervisor = Supervisor::start(setup)?;
    supervisor.attempt(1).await?;
    let received = classify(&fs::read_to_string(supervisor.root.join("dump"))?)?;
    let record = supervisor.record().ok_or("no record")?;
    let mut recorded: Vec<String> = record
        .lines()
        .filter(|line| line.starts_with("agent "))
        .map(str::to_owned)
        .collect();
    recorded.sort();
    assert_eq!(recorded, received, "record:\n{record}");
    assert!(recorded.contains(&"agent P_TOKEN empty".to_owned()));
    assert!(recorded.contains(&"agent STRIPE_SECRET other".to_owned()));
    assert!(record.contains("main P_TOKEN other\n"));
    Ok(())
}

#[tokio::test]
async fn jackin_start_replaces_the_whole_record_at_each_attempt() -> TestResult {
    let mut setup = Setup::new();
    setup.env = golden_env();
    setup.env_file = Some(ENV_FILE);
    let supervisor = Supervisor::start(setup)?;
    supervisor.attempt(1).await?;
    supervisor.attempt(2).await?;
    let record = supervisor.record().ok_or("no record")?;
    assert!(
        record.starts_with("jackin-env-kinds 2\nattempt 2\n"),
        "{record}"
    );
    assert_eq!(record.matches("attempt ").count(), 1, "{record}");
    assert_eq!(record.matches(MAIN_LINES).count(), 1, "{record}");
    assert!(!supervisor.run_dir().join("env-kinds.tmp").exists());
    fs::write(
        supervisor.root.join("sandbox/.jackin/env-L1"),
        "touch pwned\n",
    )?;
    supervisor.request("3")?;
    supervisor.wait_state("exited 3 65").await?;
    assert_eq!(
        supervisor.record().as_deref(),
        Some(format!("jackin-env-kinds 2\nattempt 3\n{MAIN_LINES}").as_str())
    );
    Ok(())
}

#[tokio::test]
async fn jackin_start_calls_no_program_by_bare_name_after_sourcing_the_env_file() -> TestResult {
    let mut setup = Setup::new();
    setup.env = golden_env();
    setup.env_file = Some("export PATH='@ROOT@/evil'\nexport STRIPE_SECRET='sk-live-FAKE'\n");
    let (dir, root) = Supervisor::prepare(&setup)?;
    for name in [
        "awk", "sort", "mv", "tr", "env", "codex", "cat", "printf", "sed",
    ] {
        install(
            &root.join("evil").join(name),
            &format!("#!/bin/sh\n: > '{}/marks/{name}'\n", root.display()),
        )?;
    }
    let supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.attempt(1).await?;
    let record = supervisor.record().ok_or("no record")?;
    assert!(record.contains("agent STRIPE_SECRET other\n"), "{record}");
    assert_eq!(supervisor.runs(), 1, "the real fake Codex ran once");
    let marks: Vec<String> = fs::read_dir(supervisor.root.join("marks"))?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    assert!(
        marks.is_empty(),
        "programs from the env file PATH ran: {marks:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_record_write_does_not_stop_the_supervisor() -> TestResult {
    let setup = Setup::new();
    let (dir, root) = Supervisor::prepare(&setup)?;
    fs::create_dir(root.join("sandbox/.jackin/run/env-kinds.tmp"))?;
    let mut supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.wait_state("waiting").await?;
    supervisor.attempt(1).await?;
    supervisor.attempt(2).await?;
    assert!(supervisor.alive(), "jackin-start stopped");
    assert_eq!(supervisor.runs(), 2);
    assert!(supervisor.record().is_none(), "a record appeared");
    Ok(())
}

#[tokio::test]
async fn jackin_start_runs_the_agent_by_absolute_path_whatever_path_the_env_file_sets() -> TestResult
{
    let mut setup = Setup::new();
    setup.env_file = Some("export PATH='@ROOT@/evil:/usr/bin:/bin'\n");
    let (dir, root) = Supervisor::prepare(&setup)?;
    install(
        &root.join("evil/codex"),
        &format!("#!/bin/sh\n: > '{}/marks/evil'\n", root.display()),
    )?;
    let supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.attempt(1).await?;
    assert!(
        !supervisor.root.join("marks/evil").exists(),
        "evil/codex ran"
    );
    let log = fs::read_to_string(supervisor.root.join("log"))?;
    assert_eq!(
        log,
        "REAL --sandbox danger-full-access -c check_for_update_on_startup=false\n"
    );

    let mut setup = Setup::new();
    let (dir, root) = Supervisor::prepare(&setup)?;
    let tool = format!("{}/tool", root.display());
    install(
        &root.join("tool"),
        "#!/bin/sh\nprintf 'TOOL %s\\n' \"$*\" >> \"$LOG\"\n",
    )?;
    setup.args = vec![tool.as_str(), "one", "two"];
    let supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.attempt(1).await?;
    assert_eq!(
        fs::read_to_string(supervisor.root.join("log"))?,
        "TOOL one two\n"
    );
    Ok(())
}

#[test]
fn launch_command_name_resolves_to_the_agent_binary_path() -> TestResult {
    let spec = Agent::Codex.spec();
    let directory = SCRIPT
        .lines()
        .find_map(|line| line.strip_prefix("JACKIN_BIN_DIR="))
        .ok_or("no JACKIN_BIN_DIR= line in jackin-start")?;
    let name = spec.launch_command.first().ok_or("empty launch command")?;
    assert!(
        !name.contains('/'),
        "launch command starts with a path: {name}"
    );
    assert_eq!(format!("{directory}/{name}"), spec.binary.as_str());
    Ok(())
}

#[tokio::test]
async fn jackin_start_accepts_only_the_successor_of_the_last_attempt() -> TestResult {
    let supervisor = Supervisor::start(Setup::new())?;
    supervisor.attempt(1).await?;
    assert_eq!(supervisor.runs(), 1);
    let pin = "9".repeat(18);
    supervisor.request(&pin)?;
    sleep(POLL * QUIET_POLLS).await;
    assert_eq!(
        supervisor.state(),
        "exited 1 0\n",
        "an oversized id was taken"
    );
    assert_eq!(supervisor.runs(), 1);
    supervisor.attempt(2).await?;
    assert_eq!(supervisor.runs(), 2, "the counter was pinned");
    supervisor.request("4")?;
    sleep(POLL * QUIET_POLLS).await;
    assert_eq!(supervisor.state(), "exited 2 0\n", "a skipped id was taken");
    assert_eq!(supervisor.runs(), 2);
    supervisor.request("2")?;
    sleep(POLL * QUIET_POLLS).await;
    assert_eq!(
        supervisor.state(),
        "exited 2 0\n",
        "a repeated id was taken"
    );
    assert_eq!(supervisor.runs(), 2, "a repeated id ran again");
    supervisor.attempt(3).await?;
    assert_eq!(supervisor.runs(), 3);
    Ok(())
}

#[tokio::test]
async fn a_stale_record_is_removed_when_the_classifier_fails() -> TestResult {
    let mut setup = Setup::new();
    setup.env = golden_env();
    setup.missing_awk = true;
    let (dir, root) = Supervisor::prepare(&setup)?;
    fs::write(root.join("sandbox/.jackin/run/env-kinds"), "stale\n")?;
    let supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.attempt(1).await?;
    assert_eq!(
        supervisor.record(),
        None,
        "a stale or partial record survived"
    );
    assert!(!supervisor.run_dir().join("env-kinds.tmp").exists());
    Ok(())
}

const BIG_STEP: Duration = Duration::from_secs(150);

fn replace_env_file(root: &Path, text: &str) -> TestResult {
    let dir = root.join("sandbox/.jackin");
    fs::write(dir.join("env-L1.tmp"), text)?;
    fs::rename(dir.join("env-L1.tmp"), dir.join("env-L1"))?;
    Ok(())
}

fn dump_value(supervisor: &Supervisor, name: &str) -> Result<Option<String>, Box<dyn Error>> {
    let dump = fs::read_to_string(supervisor.root.join("dump"))?;
    Ok(dump
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_owned()))
}

fn marks(supervisor: &Supervisor) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(fs::read_dir(supervisor.root.join("marks"))?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?)
}

#[tokio::test]
async fn jackin_start_applies_only_the_bytes_it_validated() -> TestResult {
    let big = "export V='x'\n".repeat(200_000);
    let mut setup = Setup::new();
    setup.env_file = Some(&big);
    let (dir, root) = Supervisor::prepare(&setup)?;
    install(
        &root.join("evil/prog"),
        &format!("#!/bin/sh\n: > '{}/marks/evil'\n", root.display()),
    )?;
    let supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.wait_state("waiting").await?;
    let started = Instant::now();
    supervisor.request("1")?;
    supervisor
        .wait_until(BIG_STEP, "exited 1 0", |state| state == "exited 1 0")
        .await?;
    let control = started.elapsed();
    let swap_delay = (control / 10).clamp(Duration::from_millis(300), Duration::from_millis(1200));
    assert_eq!(
        dump_value(&supervisor, "V")?.as_deref(),
        Some("x"),
        "control"
    );
    let root = supervisor.root.display().to_string();
    let swaps = [
        format!("export V='second'\nexec {root}/evil/prog\n"),
        format!("touch {root}/marks/touched\n"),
    ];
    let mut applied = 0;
    for (index, swap) in swaps.iter().enumerate() {
        let id = index + 2;
        replace_env_file(&supervisor.root, &big)?;
        let before = supervisor.state();
        fs::write(supervisor.root.join("swap"), swap)?;
        supervisor.request(&id.to_string())?;
        sleep(swap_delay).await;
        assert_eq!(
            supervisor.state(),
            before,
            "invalid instrument: validation of attempt {id} ended before the swap ({control:?} per attempt)"
        );
        fs::rename(
            supervisor.root.join("swap"),
            supervisor.root.join("sandbox/.jackin/env-L1"),
        )?;
        let prefix = format!("exited {id} ");
        supervisor
            .wait_until(BIG_STEP, &prefix, |state| state.starts_with(&prefix))
            .await?;
        assert_eq!(
            marks(&supervisor)?,
            Vec::<String>::new(),
            "attempt {id} ran a swapped byte"
        );
        if supervisor.state() == format!("exited {id} 0\n") {
            applied += 1;
            assert_eq!(
                dump_value(&supervisor, "V")?.as_deref(),
                Some("x"),
                "attempt {id}"
            );
        } else {
            assert_eq!(
                supervisor.state(),
                format!("exited {id} 65\n"),
                "attempt {id}"
            );
        }
    }
    assert!(
        applied >= 1,
        "no swap attempt reached Codex with the validated file ({control:?} per attempt)"
    );
    Ok(())
}

#[tokio::test]
async fn jackin_start_refuses_every_malformed_env_file_and_runs_nothing() -> TestResult {
    let rows: [(&str, &str, usize); 19] = [
        ("bare command", "touch pwned\n", 1),
        ("no export", "A=1\n", 1),
        ("digit name", "export 1A='x'\n", 1),
        ("dash in name", "export A-B='x'\n", 1),
        ("empty name", "export ='x'\n", 1),
        ("unquoted", "export A=x\n", 1),
        ("text after quote", "export A='x'extra\n", 1),
        ("semicolon", "export A='x'; touch pwned\n", 1),
        ("substitution", "export A='x'$(touch pwned)\n", 1),
        ("quote break", "export A='x'y'z'\n", 1),
        ("comment", "# comment\n", 1),
        ("blank line", "\n", 1),
        ("crlf", "export A='x'\r\n", 1),
        ("no final newline", "export A='x'\nexport B='y'", 2),
        ("continuation", "export A='x'\n  exec evil\n", 2),
        ("third line", "export A='x'\nexport B='y'\nbad\n", 3),
        ("only no final newline", "export A='x'", 1),
        ("supervisor namespace", "export JACKIN_RUN='x'\n", 1),
        (
            "supervisor namespace after a valid line",
            "export A='x'\nexport JACKIN_PROJECT='y'\n",
            2,
        ),
    ];
    let mut setup = Setup::new();
    setup.env_file = Some("");
    let supervisor = Supervisor::start(setup)?;
    supervisor.wait_state("waiting").await?;
    for (index, (label, text, line)) in rows.iter().enumerate() {
        let id = index + 1;
        replace_env_file(&supervisor.root, text)?;
        supervisor.request(&id.to_string())?;
        supervisor.wait_state(&format!("exited {id} 65")).await?;
        let stderr = fs::read_to_string(supervisor.root.join("stderr"))?;
        let last = stderr.lines().last().unwrap_or_default();
        assert!(
            last.ends_with(&format!("env-L1 is malformed at line {line}")),
            "{label}: {stderr}"
        );
        assert_eq!(supervisor.runs(), 0, "{label}: Codex ran");
        assert!(
            !supervisor.root.join("pwned").exists(),
            "{label}: a command ran"
        );
    }
    Ok(())
}

#[tokio::test]
async fn jackin_start_gives_codex_every_env_value_byte_for_byte() -> TestResult {
    let long = "z".repeat(100_000);
    let values: [&str; 14] = [
        "it's",
        "a b",
        "$HOME",
        "$(touch @ROOT@/marks/cmd)",
        "`touch @ROOT@/marks/tick`",
        "\"q\"",
        "a\\nb",
        "*",
        "#x",
        "  padded  ",
        "",
        "a=b=c",
        "'''",
        &long,
    ];
    let mut text = String::new();
    for (index, value) in values.iter().enumerate() {
        text.push_str(&format!(
            "export V{index}='{}'\n",
            value.replace('\'', "'\\''")
        ));
    }
    let mut setup = Setup::new();
    setup.env_file = Some(&text);
    let supervisor = Supervisor::start(setup)?;
    supervisor.attempt(1).await?;
    let root = supervisor.root.display().to_string();
    for (index, value) in values.iter().enumerate() {
        let wanted = value.replace("@ROOT@", &root);
        assert_eq!(
            dump_value(&supervisor, &format!("V{index}"))?.as_deref(),
            Some(wanted.as_str()),
            "V{index}"
        );
    }
    assert_eq!(marks(&supervisor)?, Vec::<String>::new());
    assert!(!supervisor.root.join("pwned").exists());
    Ok(())
}

#[tokio::test]
async fn jackin_start_reports_an_unreadable_env_file_and_accepts_a_missing_one() -> TestResult {
    let mut setup = Setup::new();
    setup.env_file = Some("export V='x'\n");
    let (dir, root) = Supervisor::prepare(&setup)?;
    let path = root.join("sandbox/.jackin/env-L1");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000))?;
    let readable = fs::read(&path).is_ok();
    let supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.wait_state("waiting").await?;
    supervisor.request("1")?;
    if readable {
        supervisor.wait_state("exited 1 0").await?;
    } else {
        supervisor.wait_state("exited 1 65").await?;
        let stderr = fs::read_to_string(supervisor.root.join("stderr"))?;
        assert!(stderr.contains("env-L1 is not readable"), "{stderr}");
        assert_eq!(supervisor.runs(), 0);
    }
    fs::remove_file(&path)?;
    supervisor.request("2")?;
    supervisor.wait_state("exited 2 0").await?;
    assert_eq!(
        dump_value(&supervisor, "V")?,
        None,
        "a removed file left its variable"
    );
    Ok(())
}

const HOSTILE: &str = "/definitely-not-present-jackin-review";
const CONTROL: &str = "ROLE_NOTE";
const RESET_BY_ANY_SHELL: [&str; 1] = ["IFS"];
const GOLDEN_CODEX_LINES: &str = "agent AWS_SECRET_ACCESS_KEY other\nagent GITHUB_TOKEN empty\nagent MULTI_KEY other\nagent OPENAI_API_KEY placeholder\n";
const COLLISION_FLOOR: [&str; 21] = [
    "project",
    "run",
    "bin_dir",
    "environ_file",
    "envfile",
    "attempt_id",
    "main_ok",
    "main_lines",
    "last",
    "attempt",
    "rc",
    "bad",
    "env_text",
    "program",
    "codex_lines",
    "IFS",
    "CDPATH",
    "LC_ALL",
    "TMPDIR",
    "ENV",
    "BASH_ENV",
];

fn shell_variables_of_the_script() -> Result<Vec<String>, regex::Error> {
    let assigned = regex::Regex::new(
        r"(?m)(?:^|[\s;&|(])([A-Za-z_][A-Za-z0-9_]*)=|\bread -r ([A-Za-z_][A-Za-z0-9_]*)|\bfor ([A-Za-z_][A-Za-z0-9_]*) in\b",
    )?;
    let mut names: std::collections::BTreeSet<String> = COLLISION_FLOOR
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    for captures in assigned.captures_iter(SCRIPT) {
        if let Some(name) = captures.iter().skip(1).flatten().next() {
            names.insert(name.as_str().to_owned());
        }
    }
    names.retain(|name| !name.starts_with("JACKIN_"));
    Ok(names.into_iter().collect())
}

async fn attempt_exporting(
    supervisor: &Supervisor,
    id: usize,
    name: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    for stale in ["dump", "dump.cwd"] {
        drop(fs::remove_file(supervisor.root.join(stale)));
    }
    replace_env_file(&supervisor.root, &format!("export {name}='{HOSTILE}'\n"))?;
    supervisor.request(&id.to_string())?;
    let prefix = format!("exited {id} ");
    supervisor
        .wait_until(STEP, &prefix, |state| state.starts_with(&prefix))
        .await?;
    let mut problems = Vec::new();
    let state = supervisor.state();
    if state.trim_end() != format!("exited {id} 0") {
        problems.push(format!("{name}: state {:?}", state.trim_end()));
    }
    let project = supervisor.root.join("sandbox/project");
    let cwd = fs::read_to_string(supervisor.root.join("dump.cwd")).unwrap_or_default();
    if cwd.trim_end() != project.display().to_string() {
        problems.push(format!("{name}: Codex ran in {:?}", cwd.trim_end()));
    }
    let record = supervisor.record().unwrap_or_default();
    let wanted = format!("jackin-env-kinds 2\nattempt {id}\n{MAIN_LINES}{GOLDEN_CODEX_LINES}");
    if record != wanted {
        problems.push(format!("{name}: record {record:?}"));
    }
    let applied = dump_value(supervisor, name).unwrap_or_default();
    if applied.as_deref() != Some(HOSTILE) && !RESET_BY_ANY_SHELL.contains(&name) {
        problems.push(format!("{name}: Codex received {applied:?}"));
    }
    Ok(problems)
}

#[tokio::test]
async fn a_role_variable_name_cannot_change_what_the_supervisor_does() -> TestResult {
    let mut setup = Setup::new();
    setup.env = golden_env();
    setup.env_file = Some("");
    let (dir, root) = Supervisor::prepare(&setup)?;
    install(
        &root.join("bin/codex"),
        &FAKE_CODEX.replace("exit 0\n", "pwd -P > \"$DUMP.cwd\"\nexit 0\n"),
    )?;
    let supervisor = Supervisor::spawn(setup, dir, root)?;
    supervisor.wait_state("waiting").await?;
    let control = attempt_exporting(&supervisor, 1, CONTROL).await?;
    assert_eq!(control, Vec::<String>::new(), "the harmless name");
    let names = shell_variables_of_the_script()?;
    assert!(names.len() > COLLISION_FLOOR.len(), "{names:?}");
    let mut problems = Vec::new();
    for (index, name) in names.iter().enumerate() {
        problems.extend(attempt_exporting(&supervisor, index + 2, name).await?);
    }
    assert_eq!(problems, Vec::<String>::new());
    Ok(())
}
