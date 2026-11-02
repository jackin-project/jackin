// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::AppEnv;
use jackin_core::EnvSnapshot;
use jackin_testkit::{ACCESS, REFRESH, Rig, TestResult, guard, launch_id};
use tokio_util::sync::CancellationToken;

use super::support::{Failure, Harness, Shared};

const KEY: &str = "SECRETVALUE";

fn secrets() -> [&'static str; 3] {
    [ACCESS, REFRESH, KEY]
}

fn clean(haystack: &[String]) -> Result<(), Failure> {
    for text in haystack {
        for secret in secrets() {
            if text.contains(secret) {
                return Err(format!("{secret} leaked into: {text}").into());
            }
        }
    }
    Ok(())
}

#[tokio::test]
async fn no_secret_reaches_any_output_json_config_or_debug_text() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let auth = std::fs::read_to_string(rig.home.as_path().join("auth.json"))?;
        assert!(auth.contains(ACCESS) && auth.contains(REFRESH));

        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.env.push(("CLAUDE_KEY".to_owned(), KEY.to_owned()));
        harness.write_config(&rig.config.render())?;
        let home = rig.home.as_path().display().to_string();
        let id = launch_id(1).to_string();
        let sandbox = rig.sandbox_name(1)?;
        let runs: [&[&str]; 8] = [
            &[
                "account",
                "add",
                "codex",
                "--name",
                "second",
                "--chatgpt-from",
                &home,
            ],
            &[
                "account",
                "add",
                "claude",
                "--name",
                "c",
                "--credential",
                "env:CLAUDE_KEY",
            ],
            &["load", "--launch-id", &id],
            &["ls", "--json"],
            &["inspect", sandbox.as_str(), "--json"],
            &["account", "ls", "--json"],
            &["account", "ls"],
            &["doctor"],
        ];
        let mut haystack = Vec::new();
        for args in runs {
            let ran = harness.run(args).await?;
            haystack.push(ran.stdout);
            haystack.push(ran.stderr);
        }
        std::fs::write(
            rig.home.as_path().join("auth.json"),
            format!("not json {ACCESS} {REFRESH}"),
        )?;
        let broken = harness
            .run(&[
                "account",
                "add",
                "codex",
                "--name",
                "third",
                "--chatgpt-from",
                &home,
            ])
            .await?;
        assert_eq!(broken.exit, jackin::ExitClass::Failure);
        haystack.push(broken.stdout);
        haystack.push(broken.stderr);
        haystack.extend(harness.config_bytes()?);

        let failing = Rig::builder().printenv_exit(1).build().await?;
        let mut other = Harness::new(&failing.world);
        other.cwd = failing.repo.root().clone();
        other.env.push(("CLAUDE_KEY".to_owned(), KEY.to_owned()));
        other.write_config(&failing.config.render())?;
        let ran = other
            .run(&["load", "--launch-id", &launch_id(2).to_string()])
            .await?;
        assert_eq!(ran.exit, jackin::ExitClass::Failure);
        haystack.push(ran.stdout);
        haystack.push(ran.stderr);

        let env = AppEnv {
            args: Vec::new(),
            stdout: Box::new(Shared::default()),
            stderr: Box::new(Shared::default()),
            terminal: None,
            cwd: rig.home.clone(),
            config_path: harness.config_path.clone(),
            cache_dir: harness.cache_dir.clone(),
            env: EnvSnapshot::from_pairs([("CLAUDE_KEY".to_owned(), KEY.to_owned())]),
            cancel: CancellationToken::new(),
        };
        haystack.push(format!("{env:?}"));
        clean(&haystack)
    })
    .await
}
