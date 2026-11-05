// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::{
    BranchName, CommitId, EnvName, HostPath, ManifestError, Phase, ProviderName, RepoPath,
    SandboxName, SupervisorReading, TerminalText,
};
use jackin_openshell::{ClosureViolation, RuleKey};
use jackin_session::{
    AttachError, CredentialError, EnvFileProblem, EnvQuestion, ExportError, HostGitError,
    LaunchError, LaunchFailure, LaunchStep, PlanError, ProjectStep, PurgeError, RoleError,
    Unexported, UnknownReason, UnsupportedProject, WorkState,
};

use super::support::{TestResult, launch_id};

fn sandbox() -> Result<SandboxName, Box<dyn Error>> {
    Ok(SandboxName::parse("jackin-work-0001")?)
}

#[test]
fn launch_and_project_steps_print_as_words() {
    let launch = [
        "image",
        "provider",
        "sandbox",
        "ready",
        "credentials",
        "upload",
        "supervisor",
        "hooks",
        "start",
    ];
    for (step, word) in LaunchStep::ALL.into_iter().zip(launch) {
        assert_eq!(step.to_string(), word);
    }
    assert_eq!(ProjectStep::SetHead.to_string(), "set head");
    assert_eq!(ProjectStep::RemoveBundle.to_string(), "remove bundle");
    assert_eq!(ProjectStep::TreeList.to_string(), "tree list");
}

#[test]
fn launch_errors_name_sandboxes_phases_durations_and_launches_plainly() -> TestResult {
    let sandbox = sandbox()?;
    let first = launch_id(1);
    let other = SandboxName::parse("jackin-work-0002")?;
    let rows = vec![
        (
            LaunchError::ReadyTimeout {
                sandbox: sandbox.clone(),
                phase: Phase::Starting,
                after: Duration::from_secs(30),
            },
            "sandbox jackin-work-0001 was starting after 30s".to_owned(),
        ),
        (
            LaunchError::SupervisorTimeout {
                sandbox: sandbox.clone(),
                after: Duration::from_millis(250),
            },
            "the supervisor of jackin-work-0001 did not report within 250ms".to_owned(),
        ),
        (
            LaunchError::AmbiguousLaunch {
                launch: first.clone(),
                sandboxes: vec![sandbox.clone(), other.clone()],
            },
            format!("launch {first} matches several sandboxes: jackin-work-0001, jackin-work-0002"),
        ),
        (
            LaunchError::NameCollision {
                name: sandbox.clone(),
                other: Some(launch_id(2)),
            },
            format!(
                "sandbox name jackin-work-0001 belongs to another launch {}",
                launch_id(2)
            ),
        ),
        (
            LaunchError::NameCollision {
                name: sandbox.clone(),
                other: None,
            },
            "sandbox name jackin-work-0001 belongs to a sandbox with no launch ID".to_owned(),
        ),
        (
            LaunchError::AttemptTimeout {
                sandbox: sandbox.clone(),
                attempt: jackin_core::AttemptId::FIRST,
                last: SupervisorReading::Unreadable,
            },
            "attempt 1 of jackin-work-0001 did not start; last reading unreadable".to_owned(),
        ),
        (
            LaunchError::ProjectFailed {
                step: ProjectStep::SetHead,
                exit_code: 1,
                output: TerminalText::sanitize(b"out"),
            },
            "project step set head failed with exit code 1: out".to_owned(),
        ),
        (
            LaunchError::HookFailed {
                hook: jackin_core::HookKind::Setup,
                exit_code: 3,
                output: TerminalText::sanitize(b"boom"),
            },
            "the setup hook exited with 3: boom".to_owned(),
        ),
    ];
    for (error, expected) in rows {
        assert_eq!(error.to_string(), expected);
    }
    let failure = LaunchFailure {
        launch_id: first.clone(),
        step: LaunchStep::Credentials,
        sandbox: None,
        error: Box::new(LaunchError::SandboxSuspended { sandbox }),
    };
    assert_eq!(
        failure.to_string(),
        format!("launch {first} failed at credentials")
    );
    assert_eq!(
        failure.source().map(ToString::to_string).as_deref(),
        Some("sandbox jackin-work-0001 is suspended")
    );
    Ok(())
}

#[test]
fn env_file_and_work_state_problems_print_as_sentences() -> TestResult {
    let sandbox = sandbox()?;
    for (problem, expected) in [
        (
            EnvFileProblem::NoLaunchLabel,
            "the env file of jackin-work-0001 is unusable: no launch label",
        ),
        (
            EnvFileProblem::Missing,
            "the env file of jackin-work-0001 is unusable: missing",
        ),
    ] {
        let error = AttachError::EnvFile {
            sandbox: sandbox.clone(),
            problem,
        };
        assert_eq!(error.to_string(), expected);
    }
    let commit = CommitId::parse(&"a".repeat(40))?;
    let unexported = WorkState::Unexported(Unexported {
        uncommitted: 3,
        branches: vec![BranchName::parse("main")?],
        detached: Some(commit.clone()),
        worktrees: vec![BranchName::parse("side")?],
        unusable: vec![TerminalText::sanitize(b"odd\x1b[2J\nname")],
    });
    let states = [
        (WorkState::Exported, "exported".to_owned()),
        (
            unexported,
            format!(
                "uncommitted changes, branch main not exported, detached HEAD {commit} not exported, worktree side not exported, branch odd\\nname cannot be exported"
            ),
        ),
        (
            WorkState::Unknown(UnknownReason::NoRepository),
            "cannot be inspected (no repository)".to_owned(),
        ),
        (
            WorkState::Unknown(UnknownReason::NotRunning(Phase::Stopped)),
            "cannot be inspected (stopped)".to_owned(),
        ),
    ];
    for (state, expected) in states {
        assert_eq!(state.to_string(), expected);
        let purge = PurgeError::WorkNotExported {
            sandbox: sandbox.clone(),
            state: Box::new(state),
        };
        assert_eq!(
            purge.to_string(),
            format!("the work in jackin-work-0001 is not exported: {expected}")
        );
    }
    let replaced = PurgeError::Replaced {
        sandbox: sandbox.clone(),
    }
    .to_string();
    assert!(replaced.contains("jackin-work-0001"), "{replaced}");
    assert!(replaced.contains("nothing was deleted"), "{replaced}");
    Ok(())
}

#[test]
fn lists_in_errors_join_with_commas_and_an_empty_list_reads_none() -> TestResult {
    let bundle = |extra: &[&str], missing: &[&str]| {
        ExportError::BundleHeads {
            extra: extra
                .iter()
                .map(|text| TerminalText::sanitize(text.as_bytes()))
                .collect(),
            missing: missing.iter().map(|text| (*text).to_owned()).collect(),
        }
        .to_string()
    };
    let head = "the bundle heads differ from the requested set: ";
    assert_eq!(
        bundle(&["a", "b"], &[]),
        format!("{head}extra a, b; missing none")
    );
    assert_eq!(
        bundle(&[], &["a\u{1b}[2J"]),
        format!("{head}extra none; missing a\\u{{1b}}[2J")
    );
    let needed = PlanError::EnvNeeded(vec![
        EnvQuestion {
            name: EnvName::parse("API_KEY")?,
            prompt: None,
            default: None,
        },
        EnvQuestion {
            name: EnvName::parse("TOKEN")?,
            prompt: Some("token".to_owned()),
            default: None,
        },
    ]);
    assert_eq!(
        needed.to_string(),
        "environment values are needed: API_KEY, TOKEN"
    );
    let undeclared = PlanError::EnvUndeclared(vec![EnvName::parse("EXTRA")?]);
    assert_eq!(
        undeclared.to_string(),
        "environment names are not declared by the role: EXTRA"
    );
    let submodules = UnsupportedProject::PopulatedSubmodules(vec![
        RepoPath::parse("vendor/a")?,
        RepoPath::parse("vendor/b")?,
    ]);
    assert_eq!(
        submodules.to_string(),
        "the project has populated submodules vendor/a, vendor/b; run `git submodule deinit --all`"
    );
    let tip = ExportError::TipMismatch {
        host_ref: "refs/heads/x".to_owned(),
        expected: CommitId::parse(&"b".repeat(40))?,
        found: None,
    };
    assert_eq!(
        tip.to_string(),
        format!(
            "refs/heads/x expected tip {} but found no commit",
            "b".repeat(40)
        )
    );
    let closure = RoleError::ProviderHostClosure(vec![ClosureViolation::InvalidPort {
        rule: RuleKey::provider_rule(&ProviderName::parse("jackin-work-codex")?),
        port: 0,
    }]);
    assert!(
        closure
            .to_string()
            .starts_with("the role policy overlaps provider hosts: rule "),
        "{closure}"
    );
    assert!(!closure.to_string().contains(['[', '"', '{']), "{closure}");
    Ok(())
}

#[test]
fn role_errors_name_the_directory_the_file_and_leave_the_cause_to_the_source() -> TestResult {
    let root = HostPath::parse(std::path::Path::new("/work/my role"))?;
    let rows = [
        (
            RoleError::RootMissing { path: root.clone() },
            "/work/my role does not exist",
        ),
        (
            RoleError::RootNotDirectory { path: root.clone() },
            "/work/my role is not a directory",
        ),
        (
            RoleError::ManifestMissing { root: root.clone() },
            "/work/my role has no jackin.role.toml",
        ),
        (
            RoleError::FileMissing {
                path: RepoPath::parse("build/Dockerfile")?,
            },
            "missing file build/Dockerfile",
        ),
    ];
    for (error, expected) in rows {
        assert_eq!(error.to_string(), expected);
    }
    let manifest = RoleError::Manifest {
        path: root,
        source: ManifestError::Syntax {
            line: 1,
            column: 2,
            message: "boom".to_owned(),
        },
    };
    assert_eq!(
        manifest.to_string(),
        "invalid role manifest at /work/my role"
    );
    assert_eq!(
        manifest.source().map(ToString::to_string).as_deref(),
        Some("syntax error at line 1, column 2: boom")
    );
    Ok(())
}

#[test]
fn string_fields_of_session_errors_are_shown_without_control_characters() -> TestResult {
    let hostile = || "a\u{1b}[2J\nb".to_owned();
    let shown = "a\\u{1b}[2J\\nb";
    let sandbox = sandbox()?;
    let rows: Vec<(String, String)> = vec![
        (
            CredentialError::AuthJsonInvalid { key: hostile() }.to_string(),
            format!("auth.json is invalid at {shown}"),
        ),
        (
            ExportError::HostOutput { message: hostile() }.to_string(),
            format!("git on the host printed unusable output: {shown}"),
        ),
        (
            ExportError::RefConflict {
                host_ref: hostile(),
                output: TerminalText::sanitize(b"out"),
            }
            .to_string(),
            format!("ref conflict at {shown}: out"),
        ),
        (
            ExportError::RefCheckedOut {
                host_ref: hostile(),
                worktree: HostPath::parse(std::path::Path::new("/work/tree"))?,
            }
            .to_string(),
            format!("{shown} is checked out in the worktree /work/tree"),
        ),
        (
            ExportError::TipMismatch {
                host_ref: hostile(),
                expected: CommitId::parse(&"b".repeat(40))?,
                found: None,
            }
            .to_string(),
            format!(
                "{shown} expected tip {} but found no commit",
                "b".repeat(40)
            ),
        ),
        (
            ExportError::UnusableBranch {
                branch: TerminalText::sanitize(b"a\nb"),
            }
            .to_string(),
            "the sandbox branch a\\nb cannot be exported".to_owned(),
        ),
        (
            LaunchError::ProjectOutput {
                step: ProjectStep::Head,
                message: hostile(),
            }
            .to_string(),
            format!("project step head produced unusable output: {shown}"),
        ),
        (
            LaunchError::ProbeInvalid { message: hostile() }.to_string(),
            format!("the sandbox probe is invalid: {shown}"),
        ),
        (
            LaunchError::PlaceholderMissing {
                sandbox,
                key: hostile(),
            }
            .to_string(),
            format!("the sandbox jackin-work-0001 has no value for {shown}"),
        ),
        (
            HostGitError::Unusable { detail: hostile() }.to_string(),
            format!("host git cannot be used: {shown}"),
        ),
    ];
    for (message, expected) in rows {
        assert_eq!(message, expected);
    }
    Ok(())
}
