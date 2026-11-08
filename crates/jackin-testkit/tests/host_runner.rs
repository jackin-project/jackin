// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{
    DockerSubcommand, EnvValue, OutputLine, OutputStream, ProcessError, ProcessOutput,
    ProcessRunner, ProcessSpec, ProcessStatus, ProcessStdin, Program,
};
use jackin_session::TokioProcessRunner;
use jackin_testkit::{CancelsOn, PausesOn, ProcessReply, ScriptedProcessRunner, ScriptedProgram};
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
    jackin_testkit::guard(async {
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
    jackin_testkit::guard(async {
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
}

fn inspecting(word: &str) -> ProcessSpec {
    docker_spec(
        DockerSubcommand::ImageInspect,
        "/usr/bin",
        &["--format", "{{.Id}}", word],
    )
}

fn inspector() -> Arc<ScriptedProcessRunner> {
    let runner = Arc::new(ScriptedProcessRunner::new());
    runner.expect_docker(
        DockerSubcommand::ImageInspect,
        &[],
        ProcessReply::exited(0, b"sha256:0\n", b""),
    );
    runner
}

fn cancelled(result: &Result<ProcessOutput, ProcessError>) -> bool {
    matches!(
        result,
        Err(ProcessError::Cancelled {
            program: Program::Docker(DockerSubcommand::ImageInspect),
            group: None
        })
    )
}

#[tokio::test]
async fn cancels_on_cancels_the_shared_token_at_its_word_and_ends_the_call_of_that_token()
-> TestResult {
    jackin_testkit::guard(async {
        let runner = inspector();
        let shared = CancellationToken::new();
        let wrapper = CancelsOn::new(&runner, "marked", &shared);

        let plain = wrapper.run(&inspecting("plain"), None, &shared).await?;
        assert_eq!(plain.status, ProcessStatus::Exited(0));
        assert!(!shared.is_cancelled(), "a call without the word cancelled");

        let elsewhere = CancellationToken::new();
        let unrelated = wrapper.run(&inspecting("marked"), None, &elsewhere).await?;
        assert_eq!(unrelated.status, ProcessStatus::Exited(0));
        assert!(
            shared.is_cancelled(),
            "the word did not cancel the shared token"
        );
        assert!(!elsewhere.is_cancelled());

        let handed = wrapper.run(&inspecting("plain"), None, &shared).await;
        assert!(cancelled(&handed), "{handed:?}");

        let fresh = CancellationToken::new();
        let wrapper = CancelsOn::new(&runner, "marked", &fresh);
        let at_the_word = wrapper.run(&inspecting("marked"), None, &fresh).await;
        assert!(cancelled(&at_the_word), "{at_the_word:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn pauses_on_parks_the_first_call_at_its_word_until_release_and_lets_the_rest_pass()
-> TestResult {
    jackin_testkit::guard(async {
        let runner = inspector();
        let pause = Arc::new(PausesOn::new(&runner, "marked"));
        let token = CancellationToken::new();
        let parked = tokio::spawn({
            let (pause, token) = (Arc::clone(&pause), token.clone());
            async move { pause.run(&inspecting("marked"), None, &token).await }
        });

        pause.reached().await;
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        assert!(!parked.is_finished(), "the call did not park");
        assert!(
            runner.seen().is_empty(),
            "the parked call reached the runner"
        );
        let passing = pause.run(&inspecting("plain"), None, &token).await?;
        assert_eq!(passing.status, ProcessStatus::Exited(0));

        pause.release();
        let released = parked.await??;
        assert_eq!(released.status, ProcessStatus::Exited(0));
        assert!(
            runner
                .seen()
                .iter()
                .any(|call| call.args.iter().any(|arg| arg == "marked")),
            "the released call never ran"
        );

        let second = pause.run(&inspecting("marked"), None, &token).await?;
        assert_eq!(second.status, ProcessStatus::Exited(0));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_of_the_token_a_parked_call_was_handed_ends_the_pause_with_a_cancelled_call()
-> TestResult {
    jackin_testkit::guard(async {
        let runner = inspector();
        let pause = Arc::new(PausesOn::new(&runner, "marked"));
        let token = CancellationToken::new();
        let parked = tokio::spawn({
            let (pause, token) = (Arc::clone(&pause), token.clone());
            async move { pause.run(&inspecting("marked"), None, &token).await }
        });
        pause.reached().await;
        token.cancel();
        let result = parked.await?;
        assert!(cancelled(&result), "{result:?}");
        Ok(())
    })
    .await
}
