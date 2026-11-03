// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{DockerLabel, HookKind, ImageLabel, LaunchId, Phase, SandboxName, TerminalText};
use jackin_openshell::GatewayError;
use jackin_session::{LaunchError, LaunchFailure, LaunchNextStep, LaunchStep};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn launch() -> Fallible<LaunchId> {
    Ok(LaunchId::parse("01hzy3k5m7p9r2t4v6w8x0abcd")?)
}

fn sandbox() -> Fallible<SandboxName> {
    Ok(SandboxName::parse("jackin-proj-0abc")?)
}

fn failure(
    step: LaunchStep,
    sandbox: Option<SandboxName>,
    error: LaunchError,
) -> Fallible<LaunchFailure> {
    Ok(LaunchFailure {
        launch_id: launch()?,
        step,
        sandbox,
        error: Box::new(error),
    })
}

#[test]
fn a_failure_reads_as_one_summary_and_names_the_retry_when_a_sandbox_remains() -> Fallible<()> {
    let hook = failure(
        LaunchStep::Hooks,
        Some(sandbox()?),
        LaunchError::HookFailed {
            hook: HookKind::Setup,
            exit_code: 7,
            output: TerminalText::sanitize(b"npm: not found\n"),
        },
    )?;
    assert_eq!(
        hook.summary(),
        "launch 01hzy3k5m7p9r2t4v6w8x0abcd failed at step hooks: the setup hook exited with code 7\nnpm: not found\n"
    );
    assert_eq!(
        hook.next_step(),
        Some(LaunchNextStep::Retry {
            sandbox: sandbox()?,
            launch: launch()?
        })
    );

    let ended = failure(
        LaunchStep::Start,
        Some(sandbox()?),
        LaunchError::MainProcessEnded {
            sandbox: sandbox()?,
            phase: Phase::Completed { exit_code: 0 },
        },
    )?;
    assert!(
        ended
            .summary()
            .ends_with("only purge --force can remove it"),
        "{}",
        ended.summary()
    );

    let gateway = failure(
        LaunchStep::Provider,
        None,
        LaunchError::Gateway(GatewayError::Unreachable {
            endpoint: "gw".to_owned(),
            message: "refused".to_owned(),
        }),
    )?;
    assert_eq!(
        gateway.summary(),
        format!(
            "launch 01hzy3k5m7p9r2t4v6w8x0abcd failed at step provider: {}",
            LaunchError::Gateway(GatewayError::Unreachable {
                endpoint: "gw".to_owned(),
                message: "refused".to_owned(),
            })
        )
    );
    assert_eq!(gateway.next_step(), None);
    Ok(())
}

#[test]
fn a_stale_sandbox_is_never_offered_a_retry_under_the_same_launch() -> Fallible<()> {
    let image = ImageLabel::parse(&"ab".repeat(16))?;
    let stale = failure(
        LaunchStep::Sandbox,
        Some(sandbox()?),
        LaunchError::StaleSandbox {
            launch: launch()?,
            sandbox: sandbox()?,
            found_image: image.clone(),
            wanted_image: image,
            found_docker: DockerLabel::Off,
            wanted_docker: DockerLabel::Off,
        },
    )?;
    assert_eq!(stale.next_step(), None);
    assert!(
        stale
            .summary()
            .contains("start a new launch with a new launch ID"),
        "{}",
        stale.summary()
    );
    Ok(())
}
