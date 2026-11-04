// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::path::Path;
use std::time::Duration;

use jackin_core::{
    CodexAuthMode, CompatibilityError, ConfigError, DockerSource, DockerSubcommand, FsError,
    GroupFault, HookKind, HostPath, LabelError, ManifestError, OutputStream, PathError, Platform,
    ProcessError, ProcessStatus, Program, SupervisorReading, SupervisorState, plain,
    plain_duration, plain_list,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn host(text: &str) -> Result<HostPath, PathError> {
    HostPath::parse(Path::new(text))
}

#[test]
fn operand_types_print_words_and_never_rust_syntax() {
    let shown = [
        (ProcessStatus::Exited(3).to_string(), "exit code 3"),
        (ProcessStatus::Signaled(9).to_string(), "signal 9"),
        (Program::Git.to_string(), "git"),
        (
            Program::Docker(DockerSubcommand::BuildxBuild).to_string(),
            "docker buildx build",
        ),
        (OutputStream::Stderr.to_string(), "stderr"),
        (HookKind::Preflight.to_string(), "preflight"),
        (CodexAuthMode::ChatgptLogin.to_string(), "chatgpt login"),
        (Platform::LinuxArm64.to_string(), "linux/arm64"),
        (
            DockerSource::Both.to_string(),
            "both the role and the workspace",
        ),
    ];
    for (text, expected) in shown {
        assert_eq!(text, expected);
        assert!(!text.contains(['(', '{', '"']), "{text}");
    }
}

#[test]
fn plain_shows_printable_text_as_is_and_escapes_each_control_character() {
    let cases = [
        ("FROM \"a\" b c", "FROM \"a\" b c"),
        ("it's a \\ path", "it's a \\ path"),
        (
            "h\u{e9}llo \u{20ac} e\u{301}",
            "h\u{e9}llo \u{20ac} e\u{301}",
        ),
        ("pwn\u{1b}[2J", "pwn\\u{1b}[2J"),
        ("a\nb\tc\r", "a\\nb\\tc\\r"),
        ("a\u{202e}b", "a\\u{202e}b"),
        ("a\0b", "a\\0b"),
    ];
    for (input, expected) in cases {
        assert_eq!(plain(input), expected, "{input:?}");
    }
    assert_eq!(plain('\n'), "\\n");
    assert_eq!(plain(host("/a/b\nc").unwrap()), "/a/b\\nc");
}

#[test]
fn lists_join_with_commas_an_empty_list_is_none_and_durations_keep_their_units() {
    assert_eq!(plain_list(&["a", "b"]), "a, b");
    assert_eq!(plain_list::<&str>(&[]), "none");
    assert_eq!(plain_list(&["a\u{1b}[2J"]), "a\\u{1b}[2J");
    assert_eq!(plain_duration(&Duration::from_secs(30)), "30s");
    assert_eq!(plain_duration(&Duration::from_millis(250)), "250ms");
    let rendered = plain_list(&["x", "y"]);
    assert!(!rendered.contains(['[', ']', '"']), "{rendered}");
}

#[test]
fn file_and_process_errors_name_paths_programs_and_streams_plainly() -> TestResult {
    let dir = host("/home/me/my project")?;
    let io = FsError::Io {
        path: dir.clone(),
        kind: ErrorKind::NotFound,
        message: "gone".to_owned(),
    };
    let git = Program::Git;
    let rows: Vec<(String, &str)> = vec![
        (io.to_string(), "i/o error at /home/me/my project: gone"),
        (
            FsError::AlreadyExists(dir.clone()).to_string(),
            "already exists: /home/me/my project",
        ),
        (
            FsError::LockHeld(dir.clone()).to_string(),
            "lock held: /home/me/my project",
        ),
        (
            FsError::SymlinkTarget(dir.clone()).to_string(),
            "refusing to replace the symbolic link /home/me/my project",
        ),
        (
            FsError::Unsynced {
                path: dir.clone(),
                message: "no space\n".to_owned(),
            }
            .to_string(),
            "/home/me/my project was replaced, but syncing its directory failed: no space\\n",
        ),
        (
            PathError::NotADirectory(dir.clone()).to_string(),
            "not a directory: /home/me/my project",
        ),
        (
            PathError::NotAGitWorkTree(dir.clone()).to_string(),
            "not a git work tree: /home/me/my project",
        ),
        (
            PathError::UnsupportedGitEntry(dir).to_string(),
            "the .git of /home/me/my project is a symlink or another kind of file; use a normal repository, a linked worktree, or a submodule checkout",
        ),
        (
            ProcessError::Spawn {
                program: git,
                message: "nope".to_owned(),
            }
            .to_string(),
            "failed to start git: nope",
        ),
        (
            ProcessError::Timeout {
                program: git,
                after: Duration::from_secs(5),
                group: None,
            }
            .to_string(),
            "git timed out after 5s",
        ),
        (
            ProcessError::Cancelled {
                program: git,
                group: None,
            }
            .to_string(),
            "git was cancelled",
        ),
        (
            ProcessError::Group {
                program: git,
                fault: GroupFault::Alive {
                    after: Duration::from_secs(5),
                },
            }
            .to_string(),
            "git: processes of its group were still alive after 5s",
        ),
        (
            ProcessError::Group {
                program: git,
                fault: GroupFault::Unreachable {
                    message: "x\ny\u{1b}[31m".to_owned(),
                },
            }
            .to_string(),
            "git: cannot stop the processes of its group: x\\ny\\u{1b}[31m",
        ),
        (
            ProcessError::Cancelled {
                program: git,
                group: Some(GroupFault::Alive {
                    after: Duration::from_secs(5),
                }),
            }
            .to_string(),
            "git was cancelled; processes of its group were still alive after 5s",
        ),
        (
            ProcessError::Timeout {
                program: git,
                after: Duration::from_secs(5),
                group: Some(GroupFault::Unreachable {
                    message: "nope".to_owned(),
                }),
            }
            .to_string(),
            "git timed out after 5s; cannot stop the processes of its group: nope",
        ),
        (
            ProcessError::OutputTooLarge {
                program: git,
                stream: OutputStream::Stdout,
                limit: 10,
            }
            .to_string(),
            "git wrote more than 10 bytes to stdout",
        ),
        (
            ProcessError::Read {
                program: git,
                stream: OutputStream::Stderr,
                message: "eof".to_owned(),
            }
            .to_string(),
            "reading stderr of git failed: eof",
        ),
    ];
    for (shown, expected) in rows {
        assert_eq!(shown, expected);
    }
    Ok(())
}

#[test]
fn supervisor_readings_print_words_and_escape_the_text_the_sandbox_wrote() {
    let reported = SupervisorReading::Reported(SupervisorState::Waiting);
    assert_eq!(SupervisorReading::Unreadable.to_string(), "unreadable");
    assert_eq!(reported.to_string(), "waiting");
    let invalid = SupervisorReading::Invalid("bad\u{1b}[2J\nnext".to_owned());
    assert_eq!(invalid.to_string(), "invalid (bad\\u{1b}[2J\\nnext)");
}

#[test]
fn string_fields_of_core_errors_are_shown_without_control_characters() -> TestResult {
    let hostile = || "a\u{1b}[2J\nb".to_owned();
    let shown = "a\\u{1b}[2J\\nb";
    let rows: Vec<(String, String)> = vec![
        (
            FsError::Io {
                path: host("/x")?,
                kind: ErrorKind::NotFound,
                message: hostile(),
            }
            .to_string(),
            format!("i/o error at /x: {shown}"),
        ),
        (
            ProcessError::Spawn {
                program: Program::Git,
                message: hostile(),
            }
            .to_string(),
            format!("failed to start git: {shown}"),
        ),
        (
            ProcessError::Read {
                program: Program::Git,
                stream: OutputStream::Stderr,
                message: hostile(),
            }
            .to_string(),
            format!("reading stderr of git failed: {shown}"),
        ),
        (
            CompatibilityError::Syntax { message: hostile() }.to_string(),
            format!("syntax error in compatibility.lock.toml: {shown}"),
        ),
        (
            CompatibilityError::MissingField { key: hostile() }.to_string(),
            format!("missing key {shown} in compatibility.lock.toml"),
        ),
        (
            ConfigError::Syntax {
                line: 1,
                column: 2,
                message: hostile(),
            }
            .to_string(),
            format!("syntax error at line 1, column 2: {shown}"),
        ),
        (
            ConfigError::UnknownField { key: hostile() }.to_string(),
            format!("unknown field {shown}"),
        ),
        (
            ConfigError::MissingField { key: hostile() }.to_string(),
            format!("missing field {shown}"),
        ),
        (
            ConfigError::InvalidValue {
                key: hostile(),
                message: hostile(),
            }
            .to_string(),
            format!("invalid value for {shown}: {shown}"),
        ),
        (
            ConfigError::DuplicateName {
                kind: "role",
                name: hostile(),
            }
            .to_string(),
            format!("duplicate role name {shown}"),
        ),
        (
            ConfigError::MissingReference {
                kind: "role",
                name: hostile(),
            }
            .to_string(),
            format!("unknown role {shown}"),
        ),
        (
            LabelError::UnknownAgent(hostile()).to_string(),
            format!("unknown agent label {shown}"),
        ),
        (
            LabelError::InvalidKey {
                key: hostile(),
                reason: "too long",
            }
            .to_string(),
            format!("invalid label key {shown}: too long"),
        ),
        (
            LabelError::InvalidValue {
                key: hostile(),
                reason: "too long",
            }
            .to_string(),
            format!("invalid value for label {shown}: too long"),
        ),
        (
            ManifestError::Syntax {
                line: 1,
                column: 2,
                message: hostile(),
            }
            .to_string(),
            format!("syntax error at line 1, column 2: {shown}"),
        ),
        (
            ManifestError::UnknownField { key: hostile() }.to_string(),
            format!("unknown field {shown}"),
        ),
        (
            ManifestError::MissingField { key: hostile() }.to_string(),
            format!("missing field {shown}"),
        ),
        (
            ManifestError::InvalidValue {
                key: hostile(),
                message: hostile(),
            }
            .to_string(),
            format!("invalid value for {shown}: {shown}"),
        ),
    ];
    for (message, expected) in rows {
        assert_eq!(message, expected);
    }
    Ok(())
}
