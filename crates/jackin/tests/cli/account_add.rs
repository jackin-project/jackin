// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{AccountName, Agent, CredentialSource, ProfileId, ProviderName};
use jackin_testkit::{ACCESS, ACCOUNT_ID, Rig, TestResult, guard};

use super::support::{FixedConnector, Harness, failed};

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
async fn a_chatgpt_account_syncs_the_provider_first_and_other_providers_never_connect() -> TestResult
{
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
            .run(&[
                "account",
                "add",
                "claude",
                "--name",
                "c",
                "--credential",
                "env:CLAUDE_KEY",
            ])
            .await?
            .ok()?;
        let config = harness.stored().await?;
        let claude = config.account(&account("c")?).ok_or("no claude account")?;
        assert!(matches!(
            claude.credential(),
            CredentialSource::Env { var } if var.as_str() == "CLAUDE_KEY"
        ));
        assert_eq!(rig.gateway().providers(), vec![provider("work")?]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unsupported_mismatched_or_unreadable_account_fails_and_changes_nothing() -> TestResult {
    guard(async {
        let (rig, harness, home) = rigged().await?;
        let before = harness.config_bytes()?;
        let cases: [(&[&str], &str); 3] = [
            (
                &[
                    "account",
                    "add",
                    "codex",
                    "--name",
                    "api",
                    "--credential",
                    "env:X",
                ],
                "api",
            ),
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
