// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    io::Cursor,
    ops::Range,
    path::Path,
    process::{Command, Output, Stdio},
};

use anyhow::{Context, Result, ensure};

const WORKFLOW_PATH: &str = ".github/workflows/ci.yml";

#[test]
fn required_needs_include_every_job_except_the_exact_exclusions() -> Result<()> {
    let snapshot = published_snapshot()?;
    let workflow = read_workflow(snapshot.path())?;

    for missing_job in [
        "plan",
        "jactionlint",
        "check-repository-policy",
        "rust-jackin-image",
    ] {
        let changed = replace_in_job(
            &workflow,
            "required",
            &format!("      - {missing_job}\n"),
            "",
        )?;
        write_workflow(snapshot.path(), &changed)?;
        assert_rejected(
            run_strict_lint(snapshot.path())?,
            &["required.needs", missing_job],
        );
    }

    let with_seed_dependency = replace_in_job(
        &workflow,
        "required",
        "    needs:\n",
        "    needs:\n      - dependency-seed\n",
    )?;
    write_workflow(snapshot.path(), &with_seed_dependency)?;
    assert_rejected(
        run_strict_lint(snapshot.path())?,
        &["required.needs", "dependency-seed"],
    );

    let mut with_shadow_job = workflow;
    with_shadow_job.push_str(
        "\n  dependency-seed-shadow:\n    name: Dependency seed shadow\n    runs-on: ubuntu-26.04\n    steps:\n      - run: echo shadow\n",
    );
    write_workflow(snapshot.path(), &with_shadow_job)?;
    assert_rejected(
        run_strict_lint(snapshot.path())?,
        &["required.needs", "dependency-seed-shadow"],
    );
    Ok(())
}

#[test]
fn only_dependency_seed_may_tolerate_job_failure() -> Result<()> {
    let snapshot = published_snapshot()?;
    let workflow = read_workflow(snapshot.path())?;
    assert_seed_has_step_level_tolerance(&workflow)?;

    let with_explicit_false = insert_job_property(&workflow, "required", "false")?;
    write_workflow(snapshot.path(), &with_explicit_false)?;
    let output = run_strict_lint(snapshot.path())?;
    assert!(
        output.status.success(),
        "strict lint rejected an explicit false job tolerance:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    for value in [None, Some("false"), Some("\"true\"")] {
        let changed = match value {
            None => replace_in_job(
                &workflow,
                "dependency-seed",
                "    continue-on-error: true\n",
                "",
            )?,
            Some(value) => replace_in_job(
                &workflow,
                "dependency-seed",
                "    continue-on-error: true\n",
                &format!("    continue-on-error: {value}\n"),
            )?,
        };
        write_workflow(snapshot.path(), &changed)?;
        assert_rejected(
            run_strict_lint(snapshot.path())?,
            &["dependency-seed", "continue-on-error"],
        );
    }

    let without_seed = replace_in_job(
        &workflow,
        "dependency-seed",
        "  dependency-seed:\n",
        "  dependency-seed-removed:\n",
    )?;
    let with_renamed_seed_needed = replace_in_job(
        &without_seed,
        "required",
        "    needs:\n",
        "    needs:\n      - dependency-seed-removed\n",
    )?;
    write_workflow(snapshot.path(), &with_renamed_seed_needed)?;
    assert_rejected(
        run_strict_lint(snapshot.path())?,
        &["must define the dependency-seed job"],
    );

    for job_id in ["rust-jackin-image", "plan", "required"] {
        let changed = insert_job_property(&workflow, job_id, "true")?;
        write_workflow(snapshot.path(), &changed)?;
        assert_rejected(
            run_strict_lint(snapshot.path())?,
            &[job_id, "continue-on-error"],
        );
    }

    for value in ["\"true\"", "${{ always() }}"] {
        let changed = insert_job_property(&workflow, "required", value)?;
        write_workflow(snapshot.path(), &changed)?;
        assert_rejected(
            run_strict_lint(snapshot.path())?,
            &["required", "continue-on-error"],
        );
    }
    Ok(())
}

fn published_snapshot() -> Result<tempfile::TempDir> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .context("xtask crate path should be inside the repository")?;
    let mut archive_command = Command::new("git");
    archive_command
        .args(["archive", "--format=tar", "HEAD"])
        .current_dir(repository)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let archive = archive_command
        .spawn()?
        .wait_with_output()
        .context("creating the tracked source archive")?;
    ensure!(
        archive.status.success(),
        "git archive failed: {}",
        String::from_utf8_lossy(&archive.stderr)
    );
    let snapshot = tempfile::tempdir().context("creating an isolated source snapshot")?;
    tar::Archive::new(Cursor::new(archive.stdout))
        .unpack(snapshot.path())
        .context("extracting the tracked source archive")?;
    Ok(snapshot)
}

fn run_strict_lint(snapshot: &Path) -> Result<Output> {
    let mut lint_command = Command::new("mise");
    lint_command
        .args([
            "exec",
            "--locked",
            "github:open-telemetry/weaver",
            "--",
            env!("CARGO_BIN_EXE_jackin-xtask"),
            "lint",
            "--strict",
        ])
        .current_dir(snapshot)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    lint_command
        .spawn()?
        .wait_with_output()
        .context("running the public strict-lint CLI through pinned Mise tools")
}

fn assert_rejected(output: Output, expected: &[&str]) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "strict lint unexpectedly accepted the workflow:\n{stdout}\n{stderr}"
    );
    for expected_text in expected {
        assert!(
            stderr.contains(expected_text),
            "strict lint failed without the expected `{expected_text}` diagnostic:\n{stdout}\n{stderr}"
        );
    }
}

fn assert_seed_has_step_level_tolerance(workflow: &str) -> Result<()> {
    let document = jackin_xtask::workflow_yaml::parse(workflow.as_bytes())
        .context("the published workflow should parse")?;
    let seed_steps = document
        .get("jobs")
        .and_then(serde_json::Value::as_object)
        .and_then(|jobs| jobs.get("dependency-seed"))
        .and_then(serde_json::Value::as_object)
        .and_then(|seed| seed.get("steps"))
        .and_then(serde_json::Value::as_array)
        .context("the dependency-seed job should define steps")?;
    ensure!(
        seed_steps
            .iter()
            .any(|step| { step.get("continue-on-error") == Some(&serde_json::Value::Bool(true)) }),
        "the published dependency-seed job should retain a step-level true tolerance"
    );
    Ok(())
}

fn read_workflow(snapshot: &Path) -> Result<String> {
    std::fs::read_to_string(snapshot.join(WORKFLOW_PATH)).context("reading archived CI workflow")
}

fn write_workflow(snapshot: &Path, workflow: &str) -> Result<()> {
    std::fs::write(snapshot.join(WORKFLOW_PATH), workflow).context("writing test workflow variant")
}

fn replace_in_job(workflow: &str, job_id: &str, from: &str, to: &str) -> Result<String> {
    let range = job_range(workflow, job_id)?;
    let job = &workflow[range.clone()];
    let mut matches = 0;
    let mut changed = String::new();
    for line in job.split_inclusive('\n') {
        if line == from {
            matches += 1;
            changed.push_str(to);
        } else {
            changed.push_str(line);
        }
    }
    ensure!(matches == 1, "expected one matching line in `{job_id}` job");
    Ok(replace_range(workflow, range, &changed))
}

fn insert_job_property(workflow: &str, job_id: &str, value: &str) -> Result<String> {
    let header = format!("  {job_id}:\n");
    let replacement = format!("{header}    continue-on-error: {value}\n");
    replace_in_job(workflow, job_id, &header, &replacement)
}

fn job_range(workflow: &str, job_id: &str) -> Result<Range<usize>> {
    let header = format!("  {job_id}:\n");
    let Some(start) = workflow.find(&header) else {
        anyhow::bail!("missing top-level `{job_id}` job");
    };
    let body_start = start + header.len();
    let mut offset = body_start;
    for line in workflow[body_start..].split_inclusive('\n') {
        if line.starts_with("  ") && !line.starts_with("   ") {
            return Ok(start..offset);
        }
        offset += line.len();
    }
    Ok(start..workflow.len())
}

fn replace_range(workflow: &str, range: Range<usize>, replacement: &str) -> String {
    let mut changed = workflow.to_owned();
    changed.replace_range(range, replacement);
    changed
}
