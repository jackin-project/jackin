// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Agent, FileMode, HookKind, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_session::{LaunchError, LaunchStep};
use serde_json::Value;

use super::support::{
    ACCESS, ACCOUNT_ID, PLACEHOLDER, REFRESH, TestResult, builder, failure, guard, has_file,
    launch_id, launch_rig, ready, unreachable,
};
use jackin_testkit::{Fault, Outcome, ProgramScript, Rig, RoleFixture, Trigger};

fn base64url(text: &str) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'-' => 62,
        _ => 63,
    };
    let mut out = Vec::new();
    let mut bits = 0_u32;
    let mut count = 0_u32;
    for c in text.bytes() {
        bits = (bits << 6) | u32::from(value(c));
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push(u8::try_from((bits >> count) & 0xff).unwrap_or(0));
        }
    }
    out
}

#[tokio::test]
async fn credentials_files_have_the_documented_content_and_modes() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.world
            .clock
            .advance(Duration::from_secs(59 * 86_400 + 3_661));
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        let plan = rig.plan(1)?;
        let spec = Agent::Codex.spec();
        assert_eq!(
            handle.file_mode(&spec.auth_file.path),
            Some(FileMode(0o600))
        );
        let auth: Value =
            serde_json::from_slice(&handle.file(&spec.auth_file.path).ok_or("no auth.json")?)?;
        let object = auth.as_object().ok_or("auth.json is not an object")?;
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["OPENAI_API_KEY", "auth_mode", "last_refresh", "tokens"]
        );
        assert_eq!(auth["auth_mode"], "chatgptAuthTokens");
        assert!(auth["OPENAI_API_KEY"].is_null());
        assert_eq!(auth["last_refresh"], "2026-03-01T01:01:01Z");
        let tokens = &auth["tokens"];
        assert_eq!(tokens["access_token"], PLACEHOLDER);
        assert_eq!(tokens["refresh_token"], "");
        assert_eq!(tokens["account_id"], ACCOUNT_ID);
        assert_eq!(tokens.as_object().map(serde_json::Map::len), Some(4));
        let id_token = tokens["id_token"].as_str().ok_or("no id_token")?;
        let parts: Vec<&str> = id_token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: Value = serde_json::from_slice(&base64url(parts[0]))?;
        assert_eq!(header["alg"], "none");
        assert_eq!(
            handle.file_mode(&spec.user_config.path),
            Some(FileMode(0o600))
        );
        assert_eq!(
            handle.file(&spec.user_config.path),
            Some(plan.files().codex_config.text.clone().into_bytes())
        );
        assert_eq!(
            handle.file_mode(&plan.files().env.path),
            Some(FileMode(0o600))
        );
        assert_eq!(
            handle.file(&plan.files().env.path),
            Some(plan.files().env.text.clone().into_bytes())
        );
        let reads = handle.runs(&sandbox_layout::PRINTENV);
        assert_eq!(reads[0].args, ["CODEX_AUTH_ACCESS_TOKEN"]);

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
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Credentials);
        assert!(
            matches!(&*failed.error, LaunchError::PlaceholderMissing { .. }),
            "{:?}",
            failed.error
        );
        let handle = rig.handle(1).await?;
        assert!(!has_file(&handle, &Agent::Codex.spec().auth_file.path));
        assert!(!has_file(
            &handle,
            &sandbox_layout::uploaded_marker(&launch_id(1))
        ));
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

#[tokio::test]
async fn a_printenv_value_without_the_placeholder_prefix_is_refused_and_never_written_or_named()
-> TestResult {
    guard(async {
        let cases = [
            ("sk-FAKE-VALUE-123\n", "sk-FAKE-VALUE-123"),
            ("FAKE-LEAD-openshell:resolve:env:K\n", "FAKE-LEAD"),
            (
                "openshell:resolve:env:K\nsk-FAKE-VALUE-123\n",
                "sk-FAKE-VALUE-123",
            ),
        ];
        for (stdout, secret) in cases {
            let rig = launch_rig().await?;
            rig.gateway().script_program(
                &sandbox_layout::PRINTENV,
                ProgramScript {
                    exit_code: 0,
                    stdout: stdout.as_bytes().to_vec(),
                    stderr: Vec::new(),
                },
            );
            let outcome = rig.launch_observed(1).await?;
            assert_outcome_clean(&outcome, secret);
            let failed = failure(outcome)?;
            assert_eq!(failed.step, LaunchStep::Credentials, "{stdout:?}");
            assert!(
                matches!(&*failed.error, LaunchError::PlaceholderInvalid { .. }),
                "{stdout:?}: {:?}",
                failed.error
            );
            assert_files_clean(&rig, secret);
            let handle = rig.handle(1).await?;
            assert!(
                !has_file(&handle, &Agent::Codex.spec().auth_file.path),
                "{stdout:?}"
            );
            assert!(
                !has_file(&handle, &sandbox_layout::uploaded_marker(&launch_id(1))),
                "{stdout:?}"
            );
        }
        Ok(())
    })
    .await
}
