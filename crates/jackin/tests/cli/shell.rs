// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin::ExitClass;
use jackin_core::{EnvName, Phase, SandboxPath, sandbox_layout};
use jackin_testkit::{Fault, Rig, SandboxHandle, TestResult, Trigger, guard, launch_id};

use super::support::{Failure, Harness, ScriptedTerminal, Step, failed};

const VALUE: &str = "launched value";

async fn prepared(rig: &Rig) -> Result<(Harness, SandboxHandle), Failure> {
    let harness = Harness::new(&rig.world);
    harness.write_config(&rig.config.render())?;
    Ok((harness, rig.handle(1).await?))
}

fn typed(script: &[u8]) -> Result<ScriptedTerminal, Failure> {
    Ok(ScriptedTerminal::new(vec![Step::Bytes(script.to_vec())])?.0)
}

#[tokio::test]
async fn a_shell_runs_in_the_project_with_the_launched_env_and_leaves_the_main_process_alone()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let env = BTreeMap::from([(EnvName::parse("GREETING")?, VALUE.to_owned())]);
        rig.launch_with_env(1, env).await?;
        let (harness, handle) = prepared(&rig).await?;
        let before = handle.supervisor();
        let name = handle.name();
        let ran = harness
            .command(&["shell", name.as_str()])
            .terminal(typed(b"printf '%s' \"$GREETING\" > f; exit\n")?)
            .run()
            .await?;
        ran.ok()?;
        let written = handle.file(&SandboxPath::parse("/sandbox/project/f")?);
        assert_eq!(written, Some(VALUE.as_bytes().to_vec()));
        assert_eq!(handle.supervisor(), before);
        assert_eq!(handle.attachments(), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_shell_needs_a_terminal_an_env_file_and_a_ready_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let (harness, handle) = prepared(&rig).await?;
        let name = handle.name();
        let none = harness.run(&["shell", name.as_str()]).await?;
        failed(&none, ExitClass::Failure, &["terminal"])?;

        handle.exit_main(4);
        let ended = harness
            .command(&["shell", name.as_str()])
            .terminal(typed(b"touch f; exit\n")?)
            .run()
            .await?;
        failed(&ended, ExitClass::Failure, &[name.as_str()])?;
        assert_eq!(handle.phase(), Phase::Error { exit_code: Some(4) });
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_sandbox_whose_launch_never_wrote_the_env_file_gets_no_shell() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.gateway().inject(Fault::FailAt {
            trigger: Trigger::Write(sandbox_layout::env_file(&launch_id(1))),
            error: jackin_openshell::GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let failure = rig
            .launch_with(1, tokio_util::sync::CancellationToken::new())
            .await?;
        failure.err().ok_or("the launch should have failed")?;
        let (harness, handle) = prepared(&rig).await?;
        let name = handle.name();
        let ran = harness
            .command(&["shell", name.as_str()])
            .terminal(typed(b"touch f; exit\n")?)
            .run()
            .await?;
        failed(&ran, ExitClass::Failure, &[name.as_str(), "env file"])?;
        assert_eq!(
            handle.file(&SandboxPath::parse("/sandbox/project/f")?),
            None
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn osc52_is_blocked_in_both_directions_of_a_shell() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let (harness, handle) = prepared(&rig).await?;
        let name = handle.name();
        let script = vec![
            Step::Bytes(b"read line <<'END'\n".to_vec()),
            Step::Bytes(b"a\x1b]52;c;c2VjcmV0\x07b\n".to_vec()),
            Step::Bytes(
                b"END\nprintf '%s' \"$line\" > f; printf 'x\\033]52;c;c2VjcmV0\\007y\\033]52;c;?\\033\\\\z'; exit\n"
                    .to_vec(),
            ),
        ];
        let ran = harness
            .command(&["shell", name.as_str()])
            .terminal(ScriptedTerminal::new(script)?.0)
            .run()
            .await?;
        ran.ok()?;
        assert!(ran.stdout.contains("xyz"), "{:?}", ran.stdout);
        assert!(!ran.stdout.contains("]52"), "{:?}", ran.stdout);
        let written = handle.file(&SandboxPath::parse("/sandbox/project/f")?);
        assert_eq!(written, Some(b"ab".to_vec()));
        Ok(())
    })
    .await
}
