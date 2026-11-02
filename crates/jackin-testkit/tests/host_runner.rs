// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use jackin_core::{
    DockerSubcommand, EnvValue, OutputLine, OutputStream, ProcessOutput, ProcessRunner,
    ProcessSpec, ProcessStatus, ProcessStdin, Program,
};
use jackin_session::TokioProcessRunner;
use jackin_testkit::{ProcessReply, ScriptedProcessRunner, ScriptedProgram};
use tokio_util::sync::CancellationToken;

type TestResult = Result<(), Box<dyn Error>>;

fn docker_spec(sub: DockerSubcommand, path: &str, args: &[&str]) -> ProcessSpec {
    ProcessSpec {
        program: Program::Docker(sub),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        cwd: None,
        env: vec![("PATH".to_owned(), EnvValue::Plain(path.to_owned()))],
        env_remove_prefixes: Vec::new(),
        stdin: ProcessStdin::Null,
        timeout: Duration::from_secs(10),
    }
}

#[tokio::test]
async fn docker_programs_run_the_docker_binary_with_their_subcommand_words_first() -> TestResult {
    tokio::time::timeout(Duration::from_secs(30), async {
        let dir = tempfile::tempdir()?;
        let bin = dir.path().canonicalize()?.join("bin");
        fs::create_dir(&bin)?;
        let docker = bin.join("docker");
        fs::write(&docker, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n")?;
        fs::set_permissions(&docker, fs::Permissions::from_mode(0o755))?;
        let path = bin.to_str().ok_or("path is not UTF-8")?;
        let rows: [(DockerSubcommand, &[&str], &[&str]); 2] = [
            (
                DockerSubcommand::BuildxBuild,
                &["--tag=jackin.invalid/x", "ctx"],
                &["buildx", "build", "--tag=jackin.invalid/x", "ctx"],
            ),
            (
                DockerSubcommand::ImageInspect,
                &["--format", "{{.Id}}", "t"],
                &["image", "inspect", "--format", "{{.Id}}", "t"],
            ),
        ];
        for (sub, args, expected) in rows {
            let spec = docker_spec(sub, path, args);
            let output = TokioProcessRunner
                .run(&spec, None, &CancellationToken::new())
                .await?;
            assert_eq!(output.status, ProcessStatus::Exited(0), "{sub:?}");
            let printed = String::from_utf8(output.stdout)?;
            let words: Vec<&str> = printed.lines().collect();
            assert_eq!(words, expected, "{sub:?}");
        }
        Ok(())
    })
    .await
    .map_err(|_| "the test exceeded its 30 s guard")?
}

const LINES: usize = 5000;

async fn collect_run(
    runner: &dyn ProcessRunner,
    spec: &ProcessSpec,
    sink: bool,
) -> Result<(ProcessOutput, Vec<OutputLine>), Box<dyn Error>> {
    let cancel = CancellationToken::new();
    if !sink {
        return Ok((runner.run(spec, None, &cancel).await?, Vec::new()));
    }
    let (lines, mut received) = tokio::sync::mpsc::channel::<OutputLine>(LINES + 16);
    let output = runner.run(spec, Some(&lines), &cancel).await?;
    drop(lines);
    let mut seen = Vec::new();
    while let Some(line) = received.recv().await {
        seen.push(line);
    }
    Ok((output, seen))
}

#[tokio::test]
async fn the_scripted_runner_returns_the_same_tail_as_the_live_runner_for_a_long_output()
-> TestResult {
    tokio::time::timeout(Duration::from_secs(60), async {
        let dir = tempfile::tempdir()?;
        let bin = dir.path().canonicalize()?.join("bin");
        fs::create_dir(&bin)?;
        let docker = bin.join("docker");
        fs::write(
            &docker,
            format!(
                "#!/bin/sh\ni=1\nwhile [ $i -le {LINES} ]; do printf '%0100d\\n' $i; i=$((i+1)); done\n"
            ),
        )?;
        fs::set_permissions(&docker, fs::Permissions::from_mode(0o755))?;
        let path = bin.to_str().ok_or("path is not UTF-8")?;
        let live = docker_spec(DockerSubcommand::BuildxBuild, path, &["x"]);

        let numbered: Vec<(OutputStream, String)> = (1..=LINES)
            .map(|number| (OutputStream::Stdout, format!("{number:0100}")))
            .collect();
        let scripted = ScriptedProcessRunner::new();
        scripted.expect(
            ScriptedProgram::Docker,
            &[],
            ProcessReply::lines(ProcessStatus::Exited(0), numbered),
        );

        let (real, real_lines) = collect_run(&TokioProcessRunner, &live, true).await?;
        let (fake, fake_lines) = collect_run(&scripted, &live, true).await?;
        assert_eq!(real.status, fake.status);
        assert_eq!(real.stdout.len(), 65536);
        assert_eq!(fake.stdout.len(), real.stdout.len());
        assert!(fake.stdout == real.stdout, "the scripted tail differs");
        assert_eq!(fake.stderr, real.stderr);
        assert_eq!(real_lines.len(), LINES);
        assert_eq!(fake_lines, real_lines);

        let (real, _) = collect_run(&TokioProcessRunner, &live, false).await?;
        let (fake, _) = collect_run(&scripted, &live, false).await?;
        assert_eq!(real.stdout.len(), LINES * 101);
        assert_eq!(fake.stdout.len(), real.stdout.len());
        assert!(fake.stdout == real.stdout, "the scripted text differs");
        Ok(())
    })
    .await
    .map_err(|_| "the test exceeded its 60 s guard")?
}
