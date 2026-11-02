// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use jackin_core::{
    DockerSubcommand, EnvValue, ProcessRunner, ProcessSpec, ProcessStatus, ProcessStdin, Program,
};
use jackin_session::TokioProcessRunner;
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
                &["--tag=jackin/x", "ctx"],
                &["buildx", "build", "--tag=jackin/x", "ctx"],
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
