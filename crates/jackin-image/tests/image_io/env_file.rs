// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use jackin_core::{
    AttemptId, EnvFile, EnvFileError, EnvName, LaunchId, SupervisorState, sandbox_layout,
};
use tempfile::TempDir;

use crate::support::{JACKIN_START, TestResult};

const CHILD: &str = ": > \"$1.ran\"\nout=$1\nshift\nfor n in \"$@\"; do\n  if printenv \"$n\" > /dev/null; then\n    printf '%s=%s\\n' \"$n\" \"$(printenv \"$n\")\"\n  else\n    printf '%s unset\\n' \"$n\"\n  fi\ndone >> \"$out\"\n";
const HEAD: &str = "set -u\nJACKIN_RUN=$1; JACKIN_PROJECT=$2; JACKIN_ENVFILE=$3; child=$4; out=$5; JACKIN_MAIN_OK=0; JACKIN_MAIN_LINES=\nshift 5\n";
const REPORT: &str =
    "for n in \"$@\"; do\n  if printenv \"$n\" > /dev/null; then echo \"AFTER $n\"; fi\ndone\n";

fn script() -> Result<&'static str, Box<dyn Error>> {
    Ok(std::str::from_utf8(JACKIN_START)?)
}

fn function(script: &str, name: &str) -> Result<String, Box<dyn Error>> {
    let opening = format!("{name}() {{");
    let lines: Vec<&str> = script.lines().collect();
    let begin = lines
        .iter()
        .position(|line| *line == opening)
        .ok_or_else(|| format!("no {opening} line"))?;
    let end = lines[begin..]
        .iter()
        .position(|line| *line == "}")
        .ok_or_else(|| format!("no closing brace after {opening}"))?;
    Ok(lines[begin..=begin + end].join("\n"))
}

fn functions() -> Result<String, Box<dyn Error>> {
    let script = script()?;
    let mut text = String::new();
    for name in [
        "publish",
        "park",
        "kinds",
        "write_record",
        "record_codex",
        "drain",
        "check_env",
        "run_attempt",
    ] {
        text.push_str(&function(script, name)?);
        text.push('\n');
    }
    Ok(text)
}

struct Shell {
    _dir: TempDir,
    root: PathBuf,
}

impl Shell {
    fn new() -> Result<Shell, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().canonicalize()?;
        fs::create_dir(root.join("run"))?;
        fs::create_dir(root.join("project"))?;
        Ok(Shell { _dir: dir, root })
    }

    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }

    fn run(&self, body: &str, names: &[&str]) -> Result<Output, Box<dyn Error>> {
        let text = format!("{HEAD}{}{body}", functions()?);
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(text)
            .arg("sh")
            .arg(self.path("run"))
            .arg(self.path("project"))
            .arg(self.path("envfile"))
            .arg(CHILD)
            .arg(self.path("out"))
            .args(names)
            .current_dir(&self.root)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        Ok(command.spawn()?.wait_with_output()?)
    }

    fn state(&self) -> Result<String, Box<dyn Error>> {
        Ok(fs::read_to_string(self.root.join("run/state"))?)
    }

    fn pwned(&self) -> bool {
        ["", "run", "project"]
            .iter()
            .any(|dir| self.root.join(dir).join("pwned").exists())
    }

    fn ran(&self, name: &str) -> bool {
        Path::new(&self.path(&format!("{name}.ran"))).exists()
    }
}

fn names_of(map: &BTreeMap<EnvName, String>) -> Vec<&str> {
    map.keys().map(EnvName::as_str).collect()
}

fn render(map: &BTreeMap<EnvName, String>) -> Result<String, Box<dyn Error>> {
    Ok(EnvFile::new(map.clone())?.render())
}

fn one(name: &str, value: &str) -> Result<BTreeMap<EnvName, String>, Box<dyn Error>> {
    Ok(BTreeMap::from([(EnvName::parse(name)?, value.to_owned())]))
}

fn after_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with("AFTER "))
        .map(str::to_owned)
        .collect()
}

fn check_exports(shell: &Shell, label: &str, map: &BTreeMap<EnvName, String>) -> TestResult {
    fs::write(shell.path("envfile"), render(map)?)?;
    let names = names_of(map);
    let output = shell.run(
        &format!("run_attempt 7 sh -c \"$child\" sh \"$out\" \"$@\"\n{REPORT}"),
        &names,
    )?;
    assert!(
        output.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = String::new();
    for (name, value) in map {
        expected.push_str(&format!(
            "{name}={value}
"
        ));
    }
    assert_eq!(
        fs::read_to_string(shell.path("out"))?,
        expected,
        "{label}: child environment"
    );
    assert_eq!(shell.state()?, "exited 7 0\n", "{label}: final state");
    assert!(!shell.pwned(), "{label}: a command ran");
    assert_eq!(
        after_lines(&output),
        Vec::<String>::new(),
        "{label}: names left in the shell"
    );
    fs::remove_file(shell.path("out"))?;
    Ok(())
}

#[test]
fn jackin_start_exports_env_file_values_to_the_child_exactly_as_env_file_renders_them() -> TestResult
{
    let long = "a".repeat(4096);
    let values = [
        "plain",
        "",
        "it's",
        "''",
        "a b",
        "$HOME",
        "$(touch pwned)",
        "`touch pwned`",
        "a\\b",
        "*",
        "-n",
        "=x=",
        "\t",
        "x\r",
        "é日本",
        long.as_str(),
    ];
    let shell = Shell::new()?;
    for value in values {
        let label = format!(
            "value of {} bytes {:?}",
            value.len(),
            value.chars().take(12).collect::<String>()
        );
        check_exports(&shell, &label, &one("JK_S1_V", value)?)?;
    }
    let several = BTreeMap::from([
        (EnvName::parse("JK_S1_A")?, "1".to_owned()),
        (EnvName::parse("JK_S1_B")?, "two words".to_owned()),
        (EnvName::parse("JK_S1_C")?, String::new()),
    ]);
    check_exports(&shell, "several names", &several)?;
    fs::write(shell.path("envfile"), "")?;
    let output = shell.run("run_attempt 7 sh -c \"$child\" sh \"$out\"\n", &[])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(shell.ran("out"), "the child did not run for an empty file");
    assert_eq!(shell.state()?, "exited 7 0\n", "empty file state");
    Ok(())
}

#[test]
fn jackin_start_does_not_start_the_agent_on_a_malformed_env_file_and_reports_it() -> TestResult {
    let rows: [(&str, &str, usize); 13] = [
        ("unterminated quote", "export JK_S2_A='x\n", 1),
        ("unquoted value", "export JK_S2_A=x\n", 1),
        ("bare command", "touch pwned\n", 1),
        ("trailing junk", "export JK_S2_A='x'; touch pwned\n", 1),
        ("digit first", "export 1A='x'\n", 1),
        ("dash in name", "export A-B='x'\n", 1),
        ("no final newline", "export JK_S2_A='x'", 1),
        ("good then bad", "export JK_S2_A='x'\nexport JK_S2_B=y\n", 2),
        ("crlf", "export JK_S2_A='x'\r\n", 1),
        ("inner quote", "export JK_S2_A='it's'\n", 1),
        ("leading space", " export JK_S2_A='x'\n", 1),
        ("comment", "# note\n", 1),
        ("lone newline", "\n", 1),
    ];
    for (label, text, line) in rows {
        match EnvFile::parse(text) {
            Err(EnvFileError::Syntax { line: found }) => {
                assert_eq!(found, line, "{label}: oracle line");
            }
            other => return Err(format!("{label}: EnvFile::parse gave {other:?}").into()),
        }
        let shell = Shell::new()?;
        fs::write(shell.path("envfile"), text)?;
        let body = format!("run_attempt 9 sh -c \"$child\" sh \"$out\" JK_S2_A\n{REPORT}");
        let output = shell.run(&body, &["JK_S2_A"])?;
        assert!(output.status.success(), "{label}: shell failed");
        assert!(!shell.ran("out"), "{label}: the child ran");
        assert!(!shell.pwned(), "{label}: a command ran");
        assert_eq!(
            after_lines(&output),
            Vec::<String>::new(),
            "{label}: exported"
        );
        assert_eq!(shell.state()?, "exited 9 65\n", "{label}: state");
        assert_eq!(
            SupervisorState::parse(&shell.state()?)?,
            SupervisorState::Exited {
                attempt: AttemptId::parse("9")?,
                code: 65
            },
            "{label}: parsed state"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(&shell.path("envfile")), "{label}: {stderr}");
        assert!(
            stderr.contains(&format!("line {line}")),
            "{label}: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn jackin_start_reads_the_env_file_before_every_attempt_and_a_missing_file_means_no_values()
-> TestResult {
    let launch = LaunchId::parse("launch-1")?;
    let assigned = script()?
        .lines()
        .find_map(|line| line.strip_prefix("JACKIN_ENVFILE="))
        .ok_or("no JACKIN_ENVFILE= line")?;
    assert_eq!(
        assigned.replace("$JACKIN_LAUNCH_ID", launch.as_str()),
        sandbox_layout::env_file(&launch).as_str(),
        "env file path against the layout"
    );

    let shell = Shell::new()?;
    let first = render(&BTreeMap::from([
        (EnvName::parse("JK_S3_A")?, "1".to_owned()),
        (EnvName::parse("JK_S3_B")?, "2".to_owned()),
    ]))?;
    let second = render(&one("JK_S3_A", "3")?)?;
    fs::write(shell.path("e1"), first)?;
    fs::write(shell.path("e2"), second)?;
    let attempt = |number: usize| {
        format!(
            "run_attempt {number} sh -c \"$child\" sh \"{}\" \"$@\"\n",
            shell.path(&format!("o{number}"))
        )
    };
    let body = format!(
        "cp \"{e1}\" \"$JACKIN_ENVFILE\"\n{a1}cp \"{e2}\" \"$JACKIN_ENVFILE\"\n{a2}rm -f \"$JACKIN_ENVFILE\"\n{a3}{REPORT}",
        e1 = shell.path("e1"),
        e2 = shell.path("e2"),
        a1 = attempt(1),
        a2 = attempt(2),
        a3 = attempt(3),
    );
    let output = shell.run(&body, &["JK_S3_A", "JK_S3_B"])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (number, expected) in [
        (1, "JK_S3_A=1\nJK_S3_B=2\n"),
        (2, "JK_S3_A=3\nJK_S3_B unset\n"),
        (3, "JK_S3_A unset\nJK_S3_B unset\n"),
    ] {
        let seen = fs::read_to_string(shell.path(&format!("o{number}")))?;
        assert_eq!(seen, expected, "attempt {number} environment");
    }
    assert!(shell.ran("o3"), "the attempt without a file did not start");
    assert_eq!(shell.state()?, "exited 3 0\n");
    assert_eq!(
        after_lines(&output),
        Vec::<String>::new(),
        "names left in the shell"
    );
    Ok(())
}

#[test]
fn jackin_start_treats_an_unreadable_env_file_as_malformed() -> TestResult {
    let shell = Shell::new()?;
    let envfile = shell.root.join("envfile");
    fs::write(&envfile, render(&one("JK_S9_A", "x")?)?)?;
    fs::set_permissions(&envfile, fs::Permissions::from_mode(0o000))?;
    match fs::read(&envfile) {
        Err(error) if error.kind() == ErrorKind::PermissionDenied => {}
        other => {
            return Err(format!(
                "the env file is readable despite mode 000 (running as root?): {other:?}"
            )
            .into());
        }
    }
    let body = format!("run_attempt 9 sh -c \"$child\" sh \"$out\" JK_S9_A\n{REPORT}");
    let output = shell.run(&body, &["JK_S9_A"])?;
    assert!(output.status.success(), "the shell failed");
    assert!(!shell.ran("out"), "the agent ran");
    assert_eq!(
        after_lines(&output),
        Vec::<String>::new(),
        "names were exported"
    );
    assert_eq!(shell.state()?, "exited 9 65\n");
    assert_eq!(
        SupervisorState::parse(&shell.state()?)?,
        SupervisorState::Exited {
            attempt: AttemptId::parse("9")?,
            code: 65
        }
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&shell.path("envfile")), "{stderr}");
    assert!(stderr.contains("is not readable"), "{stderr}");
    assert!(!stderr.contains("malformed"), "{stderr}");
    Ok(())
}
