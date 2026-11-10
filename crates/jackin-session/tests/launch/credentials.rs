// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, FileMode, HookKind, Provider, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_session::PlanError;
use jackin_testkit::{Fault, Outcome, ProgramScript, Rig, RoleFixture, Trigger};
use tokio_util::sync::CancellationToken;

use super::support::{
    ACCESS, PLACEHOLDER, REFRESH, TestResult, builder, guard, launch_rig, ready, unreachable,
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

async fn assert_t01_refusal_without_credential_effects(rig: Rig) -> TestResult {
    let request = rig
        .request(1)?
        .tuple(Agent::Codex, Provider::Codex)
        .model(Some("gpt-6.1-sol".to_owned()));
    let error = rig
        .run(request, CancellationToken::new())
        .await
        .err()
        .ok_or("expected T01 refusal")?;
    assert!(
        matches!(
            error.downcast_ref::<PlanError>(),
            Some(PlanError::TupleNotSupported { agent: Agent::Codex, provider: Provider::Codex, reason })
                if reason == "final live proof pending after host re-login"
        ),
        "{error}"
    );
    assert!(rig.gateway().providers().is_empty());
    assert!(rig.gateway().sandboxes().is_empty());
    assert!(rig.world.engine.builds().is_empty());
    Ok(())
}

#[tokio::test]
async fn t01_refusal_does_not_read_a_missing_placeholder() -> TestResult {
    guard(async {
        let rig = builder().await?.printenv_exit(1).build().await?;
        assert_t01_refusal_without_credential_effects(rig).await
    })
    .await
}

#[tokio::test]
async fn t01_refusal_does_not_read_an_invalid_placeholder() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().script_program(
            &sandbox_layout::PRINTENV,
            ProgramScript {
                exit_code: 0,
                stdout: b"sk-FAKE-VALUE-123\n".to_vec(),
                stderr: Vec::new(),
            },
        );
        assert_t01_refusal_without_credential_effects(rig).await
    })
    .await
}

#[tokio::test]
async fn t01_refusal_never_reads_or_names_malformed_placeholder_values() -> TestResult {
    guard(async {
        for stdout in [
            "openshell:resolve:\n",
            "sk-FAKE-VALUE-123\n",
            "FAKE-LEAD-openshell:resolve:env:K\n",
            "openshell:resolve:env:K\nsk-FAKE-VALUE-123\n",
        ] {
            let rig = launch_rig().await?;
            rig.gateway().script_program(
                &sandbox_layout::PRINTENV,
                ProgramScript {
                    exit_code: 0,
                    stdout: stdout.as_bytes().to_vec(),
                    stderr: Vec::new(),
                },
            );
            assert_t01_refusal_without_credential_effects(rig).await?;
        }
        Ok(())
    })
    .await
}
