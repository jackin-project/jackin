// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{DockerRisk, EngineCert, EnvFile, sandbox_layout};
use jackin_testkit::{KEY_MARKER, Rig, TestResult, engine_pem, guard, launch_id};

use jackin::ExitClass;

use super::support::{Failure, Harness, failed};

const WARNING: &str = "warning: the Docker capability is enabled";

const RISKS: [&str; DockerRisk::ALL.len()] = [
    "  The agent can read the engine client key.",
    "  Containers bypass the sandbox network policy.",
    "  A bind mount names a path on the engine host, not in the sandbox.",
    "  The engine outlives the sandbox. Sandboxes that share one engine see each other's containers, and you remove them.",
];

fn certs(rig: &Rig) -> Result<String, Failure> {
    let dir = rig.home.as_path().join("engine-certs");
    std::fs::create_dir(&dir)?;
    for cert in EngineCert::ALL {
        std::fs::write(dir.join(cert.file_name()), engine_pem(cert))?;
    }
    Ok(dir.display().to_string())
}

async fn env_names(rig: &Rig, id: u8) -> Result<Vec<String>, Failure> {
    let bytes = rig
        .handle(id)
        .await?
        .file(&sandbox_layout::env_file(&launch_id(id)))
        .ok_or("no env file")?;
    let file = EnvFile::parse(&String::from_utf8(bytes)?)?;
    Ok(file.values().keys().map(ToString::to_string).collect())
}

#[tokio::test]
async fn a_docker_workspace_gets_the_engine_variables_and_one_warning_and_a_plain_one_gets_neither()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        let certs = certs(&rig)?;
        let root = rig.loaded.root().as_path().display().to_string();
        let home = rig.home.as_path().display().to_string();
        let repo = rig.repo.root().as_path().display().to_string();
        let setup: [&[&str]; 4] = [
            &[
                "gateway",
                "add",
                "fake-gateway",
                "--builder",
                "jackin-builder",
                "--platform",
                "linux/arm64",
                "--engine-port",
                "2376",
                "--engine-certs",
                &certs,
                "--default",
            ],
            &["role", "add", "sentinel", &root],
            &[
                "account",
                "add",
                "codex",
                "--name",
                "work",
                "--chatgpt-from",
                &home,
            ],
            &["workspace", "add", "platform-backend", &repo, "--docker"],
        ];
        for args in setup {
            harness.run(args).await?.ok()?;
        }
        let ran = harness
            .run(&["load", "--launch-id", &launch_id(1).to_string()])
            .await?;
        ran.ok()?;
        assert_eq!(ran.stderr.matches(WARNING).count(), 1, "{}", ran.stderr);
        let first_step = ran.stderr.find("image...").ok_or("no step line")?;
        assert_eq!(ran.stderr.find(WARNING), Some(0), "{}", ran.stderr);
        assert!(
            ran.stderr.find(WARNING) < Some(first_step),
            "{}",
            ran.stderr
        );
        assert!(!ran.stderr.contains(KEY_MARKER) && !ran.stdout.contains(KEY_MARKER));
        let after: Vec<&str> = ran
            .stderr
            .lines()
            .skip_while(|line| !line.starts_with(WARNING))
            .skip(1)
            .collect();
        let risks: Vec<&str> = after
            .iter()
            .copied()
            .take_while(|line| line.starts_with("  "))
            .collect();
        assert_eq!(risks, RISKS, "{}", ran.stderr);
        let names = env_names(&rig, 1).await?;
        for wanted in ["DOCKER_HOST", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"] {
            assert!(
                names.iter().any(|name| name == wanted),
                "{wanted}: {names:?}"
            );
        }

        let key = std::path::Path::new(&certs).join("key.pem");
        let valid = rig.home.as_path().join("other-key.pem");
        std::fs::write(&valid, engine_pem(EngineCert::Key))?;
        std::fs::remove_file(&key)?;
        std::os::unix::fs::symlink(&valid, &key)?;
        let refused = harness
            .run(&["load", "--launch-id", &launch_id(3).to_string()])
            .await?;
        failed(&refused, ExitClass::Failure, &["key.pem"])?;
        assert!(!refused.stderr.contains(WARNING), "{}", refused.stderr);
        assert!(!refused.stderr.contains(KEY_MARKER));
        std::fs::remove_file(&key)?;
        std::fs::write(&key, engine_pem(EngineCert::Key))?;
        harness.write_config(&rig.config.render())?;
        let plain = harness
            .run(&["load", "--launch-id", &launch_id(2).to_string()])
            .await?;
        plain.ok()?;
        assert!(!plain.stderr.contains(WARNING), "{}", plain.stderr);
        assert!(
            env_names(&rig, 2)
                .await?
                .iter()
                .all(|name| !name.starts_with("DOCKER_"))
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_refused_stale_retry_prints_the_refusal_and_no_retry_hint() -> TestResult {
    guard(async {
        let rig = Rig::builder()
            .workspace_docker(true)
            .engine(jackin_testkit::ENGINE_PORT)
            .build()
            .await?;
        rig.write_engine_certs().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;
        let id = launch_id(1).to_string();
        harness.run(&["load", "--launch-id", &id]).await?.ok()?;
        let flipped = rig
            .config
            .render()
            .replace("docker = true", "docker = false");
        assert_ne!(flipped, rig.config.render());
        harness.write_config(&flipped)?;
        let refused = harness.run(&["load", "--launch-id", &id]).await?;
        failed(
            &refused,
            ExitClass::Failure,
            &["cannot continue sandbox", "start a new launch"],
        )?;
        assert!(!refused.stderr.contains("retry with"), "{}", refused.stderr);
        assert!(
            !refused.stderr.contains("--launch-id"),
            "{}",
            refused.stderr
        );
        Ok(())
    })
    .await
}
