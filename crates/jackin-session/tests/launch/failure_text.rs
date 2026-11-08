// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fmt;

use jackin_core::{
    AccountName, Agent, DockerLabel, HookKind, ImageId, LaunchId, Phase, PlannedLabels,
    PolicyLabel, RoleName, SandboxName, TerminalText, WorkspaceName,
};
use jackin_openshell::GatewayError;
use jackin_session::{LaunchError, LaunchFailure, LaunchNextStep, LaunchStep, error_chain};

type Fallible<T> = Result<T, Box<dyn Error>>;

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
    let agents = jackin_core::AgentInstances::single(jackin_core::AgentInstance::lead(
        Agent::Codex,
        AccountName::parse("work")?,
    )?)?;
    let planned = PlannedLabels {
        workspace: WorkspaceName::parse("proj")?,
        role: RoleName::parse("backend")?,
        agents_hash: agents.hash(),
        lead_agent: Agent::Codex,
        policy: PolicyLabel::of("found"),
        launch: launch()?,
    };
    let image = ImageId::parse(&format!("sha256:{}", "ab".repeat(32)))?;
    let wanted = PlannedLabels {
        policy: PolicyLabel::of("wanted"),
        ..planned.clone()
    }
    .complete(&image, DockerLabel::Off);
    let found = planned.complete(&image, DockerLabel::Off);
    let stale = failure(
        LaunchStep::Sandbox,
        Some(sandbox()?),
        LaunchError::StaleSandbox {
            launch: launch()?,
            sandbox: sandbox()?,
            differences: found.to_labels().differences(&wanted),
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

#[derive(Debug)]
struct Link {
    text: &'static str,
    cause: Option<Box<Link>>,
}

impl fmt::Display for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text)
    }
}

impl Error for Link {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.cause
            .as_deref()
            .map(|cause| -> &(dyn Error + 'static) { cause })
    }
}

fn chain(texts: &[&'static str]) -> Option<Link> {
    texts.iter().rev().fold(None, |cause, text| {
        Some(Link {
            text,
            cause: cause.map(Box::new),
        })
    })
}

#[test]
fn an_error_chain_prints_each_cause_once_and_drops_only_a_repeat_of_the_line_before() -> Fallible<()>
{
    let cases: [(&[&'static str], &str); 8] = [
        (&["x"], "x"),
        (&["x", "x"], "x"),
        (&["a", "a", "a"], "a"),
        (&["a", "b", "c"], "a: b: c"),
        (&["x", "x", "y"], "x: y"),
        (&["x", "y", "x"], "x: y: x"),
        (&["a", "b", "b"], "a: b"),
        (&["ab", "b"], "ab: b"),
    ];
    for (texts, expected) in cases {
        let error = chain(texts).ok_or("an empty chain")?;
        assert_eq!(error_chain(&error), expected, "{texts:?}");
    }
    Ok(())
}
