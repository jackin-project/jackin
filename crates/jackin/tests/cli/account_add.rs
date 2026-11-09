// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{AccountName, Agent, CredentialSource, ProfileId, ProviderName};
use jackin_testkit::{ACCESS, ACCOUNT_ID, Rig, TestResult, World, guard};

use super::support::{FixedConnector, GATEWAY_CONFIG, Harness, failed};

const GATEWAY_ONLY: &str = "default_gateway = \"fake-gateway\"

[gateways.fake-gateway]
openshell_workspace = \"default\"
builder = \"jackin-builder\"
platform = \"linux/arm64\"
";

fn account(name: &str) -> Result<AccountName, Box<dyn std::error::Error>> {
    Ok(AccountName::parse(name)?)
}

fn provider(name: &str) -> Result<ProviderName, Box<dyn std::error::Error>> {
    Ok(ProviderName::for_account(&account(name)?, Agent::Codex))
}

async fn rigged() -> Result<(Rig, Harness, String), Box<dyn std::error::Error>> {
    let rig = Rig::new().await?;
    let mut harness = Harness::new(&rig.world);
    let parent = rig.home.as_path().parent().ok_or("no parent")?;
    harness.cwd = super::support::host_path(parent)?;
    harness.write_config(GATEWAY_ONLY)?;
    let relative = rig
        .home
        .as_path()
        .file_name()
        .ok_or("no name")?
        .to_string_lossy()
        .into_owned();
    Ok((rig, harness, relative))
}

#[tokio::test]
async fn a_chatgpt_account_syncs_the_provider_first_and_a_claude_api_key_needs_agent_omp()
-> TestResult {
    guard(async {
        let (rig, mut harness, home) = rigged().await?;
        harness
            .run(&[
                "account",
                "add",
                "codex",
                "--name",
                "work",
                "--chatgpt-from",
                &home,
            ])
            .await?
            .ok()?;
        let config = harness.stored().await?;
        let stored = config.account(&account("work")?).ok_or("no account")?;
        let CredentialSource::CodexLogin {
            home: stored_home,
            chatgpt_account_id,
        } = stored.credential()
        else {
            return Err("not a codex login".into());
        };
        assert_eq!(stored_home, &rig.home);
        assert_eq!(chatgpt_account_id.as_str(), ACCOUNT_ID);
        assert!(rig.gateway().provider(&provider("work")?).is_some());
        assert_eq!(rig.gateway().profiles(), vec![ProfileId::parse("codex")?]);

        harness.connector = FixedConnector::empty();
        harness
            .env
            .push(("CLAUDE_KEY".to_owned(), "fake-value".to_owned()));
        let before = harness.config_bytes()?;
        let refused = harness
            .run(&[
                "account",
                "add",
                "claude",
                "--name",
                "c",
                "--credential",
                "env:CLAUDE_KEY",
            ])
            .await?;
        failed(&refused, ExitClass::Usage, &["--agent omp"])?;
        assert_eq!(harness.config_bytes()?, before);
        assert_eq!(rig.gateway().providers(), vec![provider("work")?]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_mismatched_or_unreadable_account_fails_and_changes_nothing() -> TestResult {
    guard(async {
        let (rig, harness, home) = rigged().await?;
        let before = harness.config_bytes()?;
        let cases: [(&[&str], &str); 2] = [
            (
                &[
                    "account",
                    "add",
                    "claude",
                    "--name",
                    "wrong",
                    "--chatgpt-from",
                    &home,
                ],
                "claude",
            ),
            (
                &[
                    "account",
                    "add",
                    "codex",
                    "--name",
                    "gone",
                    "--chatgpt-from",
                    "nowhere",
                ],
                "nowhere",
            ),
        ];
        for (args, named) in cases {
            failed(&harness.run(args).await?, ExitClass::Failure, &[named])?;
            assert_eq!(harness.config_bytes()?, before, "{args:?}");
            assert!(rig.gateway().providers().is_empty(), "{args:?}");
        }
        std::fs::write(rig.home.as_path().join("auth.json"), "not json at all")?;
        let broken = harness
            .run(&[
                "account",
                "add",
                "codex",
                "--name",
                "bad",
                "--chatgpt-from",
                &home,
            ])
            .await?;
        failed(&broken, ExitClass::Failure, &[&home])?;
        assert_eq!(harness.config_bytes()?, before);
        assert!(rig.gateway().providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_broken_login_at_a_hostile_path_prints_one_escaped_diagnostic_line() -> TestResult {
    guard(async {
        let (rig, harness, _) = rigged().await?;
        let home = rig.world.home().as_path().join("hostile\u{1b}[2J\nlogin");
        std::fs::create_dir(&home)?;
        std::fs::write(home.join("auth.json"), "not json")?;
        let home_arg = home.to_str().ok_or("host path is not utf-8")?;
        let before = harness.config_bytes()?;
        let ran = harness
            .run(&[
                "account",
                "add",
                "codex",
                "--name",
                "bad",
                "--chatgpt-from",
                home_arg,
            ])
            .await?;
        assert_eq!(ran.exit, ExitClass::Failure);
        assert!(
            ran.stderr.contains("hostile\\u{1b}[2J\\nlogin"),
            "{}",
            ran.stderr
        );
        assert_eq!(ran.stderr.lines().count(), 1, "{:?}", ran.stderr);
        assert!(
            !ran.stderr
                .trim_end_matches('\n')
                .contains(['\u{1b}', '\n', '\r']),
            "{:?}",
            ran.stderr
        );
        assert_eq!(harness.config_bytes()?, before);
        assert!(rig.gateway().providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_codex_api_key_account_requires_agent_omp_and_a_refusal_touches_nothing() -> TestResult {
    guard(async {
        let (rig, mut harness, _home) = rigged().await?;
        harness.connector = FixedConnector::empty();
        harness
            .env
            .push(("OPENAI_KEY".to_owned(), "fake-value".to_owned()));
        let before = harness.config_bytes()?;
        for (name, credential) in [("api", "env:OPENAI_KEY"), ("vault", "op://Dev/OpenAI/key")] {
            let refused = harness
                .run(&[
                    "account",
                    "add",
                    "codex",
                    "--name",
                    name,
                    "--credential",
                    credential,
                ])
                .await?;
            failed(&refused, ExitClass::Usage, &["--agent omp"])?;
            assert_eq!(harness.config_bytes()?, before, "{name}");
        }
        assert!(harness.stored().await?.accounts().next().is_none());
        assert!(rig.gateway().providers().is_empty());
        assert!(rig.gateway().profiles().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn adding_the_same_account_twice_fails_before_the_gateway_is_touched() -> TestResult {
    guard(async {
        let (rig, harness, home) = rigged().await?;
        let args = [
            "account",
            "add",
            "codex",
            "--name",
            "work",
            "--chatgpt-from",
            &home,
        ];
        harness.run(&args).await?.ok()?;
        let record = rig.gateway().provider(&provider("work")?);
        assert!(record.is_some());
        let before = harness.config_bytes()?;
        let auth = rig.home.as_path().join("auth.json");
        let rotated = std::fs::read_to_string(&auth)?.replace(ACCESS, "ACCESS-ROTATED-0001");
        assert!(rotated.contains("ACCESS-ROTATED-0001"));
        std::fs::write(&auth, rotated)?;
        failed(&harness.run(&args).await?, ExitClass::Failure, &["work"])?;
        assert_eq!(harness.config_bytes()?, before);
        assert_eq!(rig.gateway().provider(&provider("work")?), record);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_credential_source_must_exist_where_the_account_is_added_and_a_refusal_never_echoes_it()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let mut harness = Harness::new(&world);
        harness.write_config(GATEWAY_CONFIG)?;
        let long = "A".repeat(129);
        harness.env.push((long.clone(), "fake-value".to_owned()));
        harness
            .env
            .push(("SET_VAR".to_owned(), "fake-value".to_owned()));
        let home = world.home().as_path();
        std::fs::write(home.join("present.key"), "fake-key")?;
        std::fs::create_dir(home.join("sk-FAKE-dir"))?;
        let long_env = format!("env:{long}");
        let refused: [(&str, &str); 4] = [
            ("env:sk_FAKE_1", "needs a variable that is set"),
            (&long_env, "expects env:NAME"),
            ("file:sk-FAKE-missing.key", "needs an existing regular file"),
            ("file:sk-FAKE-dir", "needs an existing regular file"),
        ];
        for (credential, expected) in refused {
            let before = harness.config_bytes()?;
            let ran = harness
                .run(&[
                    "account",
                    "add",
                    "codex",
                    "--name",
                    "x",
                    "--credential",
                    credential,
                ])
                .await?;
            assert_eq!(ran.exit, ExitClass::Usage, "{credential}");
            assert!(
                ran.stderr.contains(expected),
                "{credential}: {}",
                ran.stderr
            );
            assert!(!ran.stderr.contains("sk_FAKE"), "{}", ran.stderr);
            assert!(!ran.stderr.contains("sk-FAKE"), "{}", ran.stderr);
            assert!(!ran.stderr.contains(&long), "{}", ran.stderr);
            assert_eq!(harness.config_bytes()?, before, "{credential}");
        }
        harness
            .run(&[
                "account",
                "add",
                "codex",
                "--name",
                "by-file",
                "--agent",
                "omp",
                "--credential",
                "file:present.key",
            ])
            .await?
            .ok()?;
        assert!(
            harness
                .stored()
                .await?
                .account(&account("by-file")?)
                .is_some()
        );
        let omp = ProviderName::for_account(&account("by-file")?, Agent::Omp);
        let record = world.gateway.provider(&omp).ok_or("no omp provider")?;
        assert_eq!(record.access_token, "fake-key");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_credential_file_link_is_followed_and_an_oversized_file_or_a_link_to_a_pipe_is_refused()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(GATEWAY_CONFIG)?;
        let home = world.home().as_path();
        std::fs::write(home.join("real.key"), "fake-key")?;
        std::os::unix::fs::symlink(home.join("real.key"), home.join("linked.key"))?;
        std::fs::write(home.join("big.key"), vec![b'k'; (1 << 20) + 1])?;
        std::os::unix::fs::symlink(home.join("big.key"), home.join("big-linked.key"))?;
        assert!(
            std::process::Command::new("mkfifo")
                .arg(home.join("pipe.key"))
                .status()?
                .success()
        );
        std::os::unix::fs::symlink(home.join("pipe.key"), home.join("pipe-linked.key"))?;

        harness
            .run(&[
                "account",
                "add",
                "codex",
                "--name",
                "linked",
                "--agent",
                "omp",
                "--credential",
                "file:linked.key",
            ])
            .await?
            .ok()?;
        assert!(
            harness
                .stored()
                .await?
                .account(&account("linked")?)
                .is_some()
        );

        for credential in [
            "file:big.key",
            "file:big-linked.key",
            "file:pipe.key",
            "file:pipe-linked.key",
        ] {
            let before = harness.config_bytes()?;
            let ran = harness
                .run(&[
                    "account",
                    "add",
                    "codex",
                    "--name",
                    "refused",
                    "--credential",
                    credential,
                ])
                .await?;
            assert_eq!(ran.exit, ExitClass::Usage, "{credential}");
            assert!(
                ran.stderr.contains("needs an existing regular file"),
                "{credential}: {}",
                ran.stderr
            );
            assert_eq!(harness.config_bytes()?, before, "{credential}");
        }
        Ok(())
    })
    .await
}
