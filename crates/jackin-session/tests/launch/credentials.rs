// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, FileMode, HookKind, Provider, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_session::LaunchError;
use jackin_testkit::{Fault, Outcome, ProgramScript, Rig, RoleFixture, Trigger};
use tokio_util::sync::CancellationToken;

use super::support::{
    ACCESS, PLACEHOLDER, REFRESH, TestResult, builder, failure, guard, launch_rig, ready,
    unreachable,
};

#[tokio::test]
async fn credentials_files_have_the_documented_content_and_modes() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        let plan = rig.plan(1)?;
        let spec = Agent::Codex.spec();
        assert!(handle.file(&spec.auth_file.path).is_none());
        assert_eq!(
            handle.file_mode(&spec.user_config.path),
            Some(FileMode(0o600))
        );
        assert_eq!(
            handle.file(&spec.user_config.path),
            Some(
                plan.files()
                    .static_files
                    .iter()
                    .find(|file| file.path == spec.user_config.path)
                    .ok_or("no codex config")?
                    .text
                    .clone()
                    .into_bytes()
            )
        );
        assert_eq!(
            handle.file_mode(&plan.files().env.path),
            Some(FileMode(0o600))
        );
        assert_eq!(
            handle.file(&plan.files().env.path),
            Some(plan.files().env.text.clone().into_bytes())
        );
        assert!(handle.runs(&sandbox_layout::PRINTENV).is_empty());

        let bare = builder()
            .await?
            .role(RoleFixture::new("sentinel")?)
            .build()
            .await?;
        ready(bare.launch_observed(1).await?)?;
        let handle = bare.handle(1).await?;
        let plan = bare.plan(1)?;
        assert_eq!(plan.files().env.text, "");
        assert_eq!(handle.file(&plan.files().env.path), Some(Vec::new()));

        let rig = builder().await?.printenv_exit(1).build().await?;
        ready(rig.launch_observed(1).await?)?;
        assert!(
            rig.handle(1)
                .await?
                .runs(&sandbox_layout::PRINTENV)
                .is_empty()
        );
        Ok(())
    })
    .await
}

fn assert_files_clean(rig: &Rig, secret: &str) {
    for handle in rig.gateway().sandboxes() {
        for (file, bytes) in handle.files() {
            assert!(
                !String::from_utf8_lossy(&bytes).contains(secret),
                "{file:?}"
            );
        }
    }
    for call in rig.world.runner.scripted().seen() {
        assert!(!format!("{call:?}").contains(secret));
    }
}

fn assert_outcome_clean(outcome: &Outcome, secret: &str) {
    for event in &outcome.events {
        assert!(!format!("{event:?}").contains(secret));
    }
    if let Err(failure) = &outcome.result {
        let text = format!("{failure} {failure:?}");
        assert!(!text.contains(secret), "{text}");
    }
}

#[tokio::test]
async fn no_fixture_secret_reaches_a_sandbox_file_an_event_or_an_error() -> TestResult {
    guard(async {
        let mut rigs = Vec::new();
        let mut outcomes = Vec::new();
        let full = launch_rig().await?;
        outcomes.push(full.launch_observed(1).await?);
        rigs.push(full);

        let provider = launch_rig().await?;
        provider.gateway().inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::Provider),
            error: unreachable(),
        });
        outcomes.push(provider.launch_observed(1).await?);
        rigs.push(provider);

        let missing = builder().await?.login(false).build().await?;
        outcomes.push(missing.launch_observed(1).await?);
        rigs.push(missing);

        let echo = launch_rig().await?;
        echo.gateway().script_program(
            &sandbox_layout::hook_path(HookKind::Preflight),
            ProgramScript {
                exit_code: 2,
                stdout: format!("{PLACEHOLDER}\n").into_bytes(),
                stderr: Vec::new(),
            },
        );
        outcomes.push(echo.launch_observed(1).await?);
        rigs.push(echo);

        for secret in [ACCESS, REFRESH] {
            for rig in &rigs {
                assert_files_clean(rig, secret);
            }
            for outcome in &outcomes {
                assert_outcome_clean(outcome, secret);
            }
        }
        Ok(())
    })
    .await
}

async fn use_chatgpt_account(rig: &mut Rig) -> TestResult {
    rig.config = jackin_core::Config::parse(&format!(
        "{}\n[accounts.chatgpt]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"{}\", chatgpt_account_id = \"3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f\" }}\n",
        rig.config.render(),
        rig.home.as_path().display()
    ))?;
    rig.world
        .config
        .edit(vec![jackin_core::ConfigEdit::AddAccount(
            jackin_core::Account::new(
                jackin_core::AccountName::parse("chatgpt")?,
                Provider::Codex,
                jackin_core::CredentialSource::CodexLogin {
                    home: rig.home.clone(),
                    chatgpt_account_id: jackin_core::ChatgptAccountId::parse(
                        "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f",
                    )?,
                },
            )?,
        )])
        .await?;
    Ok(())
}

async fn run_t01(rig: &Rig) -> Result<Outcome, Box<dyn std::error::Error>> {
    let request = rig
        .request(1)?
        .tuple(Agent::Codex, Provider::Codex)
        .account(Some(jackin_core::AccountName::parse("chatgpt")?))
        .model(Some("gpt-6.1-sol".to_owned()));
    rig.run(request, CancellationToken::new()).await
}

#[tokio::test]
async fn t01_launch_writes_placeholder_auth_without_host_tokens() -> TestResult {
    guard(async {
        let mut rig = launch_rig().await?;
        use_chatgpt_account(&mut rig).await?;
        rig.gateway().script_program(
            &sandbox_layout::PRINTENV,
            ProgramScript {
                exit_code: 0,
                stdout: format!("{PLACEHOLDER}\n").into_bytes(),
                stderr: Vec::new(),
            },
        );
        ready(run_t01(&rig).await?)?;
        let handle = rig.handle(1).await?;
        let spec = Agent::Codex.spec();
        assert_eq!(
            handle.file_mode(&spec.auth_file.path),
            Some(FileMode(0o600))
        );
        let auth = String::from_utf8(
            handle
                .file(&spec.auth_file.path)
                .ok_or("missing Codex auth state")?,
        )?;
        let value: serde_json::Value = serde_json::from_str(&auth)?;
        assert_eq!(value["tokens"]["access_token"], PLACEHOLDER);
        assert_eq!(value["tokens"]["refresh_token"], "");
        assert_eq!(value["OPENAI_API_KEY"], serde_json::Value::Null);
        assert!(!auth.contains(ACCESS));
        assert!(!auth.contains(REFRESH));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn t01_missing_placeholder_fails_at_credentials_without_auth_state() -> TestResult {
    guard(async {
        let mut rig = builder().await?.printenv_exit(1).build().await?;
        use_chatgpt_account(&mut rig).await?;
        let failed = failure(run_t01(&rig).await?)?;
        assert!(
            matches!(&*failed.error, LaunchError::PlaceholderMissing { key, .. } if key == "CODEX_AUTH_ACCESS_TOKEN"),
            "{failed:?}"
        );
        let handle = rig.handle(1).await?;
        assert!(handle.file(&Agent::Codex.spec().auth_file.path).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn t01_malformed_placeholders_fail_without_echo_or_auth_state() -> TestResult {
    guard(async {
        for stdout in [
            "openshell:resolve:\n",
            "sk-FAKE-VALUE-123\n",
            "FAKE-LEAD-openshell:resolve:env:K\n",
            "openshell:resolve:env:K\nsk-FAKE-VALUE-123\n",
        ] {
            let mut rig = launch_rig().await?;
            use_chatgpt_account(&mut rig).await?;
            rig.gateway().script_program(
                &sandbox_layout::PRINTENV,
                ProgramScript {
                    exit_code: 0,
                    stdout: stdout.as_bytes().to_vec(),
                    stderr: Vec::new(),
                },
            );
            let outcome = run_t01(&rig).await?;
            assert_outcome_clean(&outcome, stdout.trim());
            let failed = failure(outcome)?;
            assert!(
                matches!(&*failed.error, LaunchError::PlaceholderInvalid { key, .. } if key == "CODEX_AUTH_ACCESS_TOKEN"),
                "{failed:?}"
            );
            let handle = rig.handle(1).await?;
            assert!(handle.file(&Agent::Codex.spec().auth_file.path).is_none());
        }
        Ok(())
    })
    .await
}
