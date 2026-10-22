//! Required covers execution/setup jobs; the bounded root seed alone tolerates failure.
use super::job_needs;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeSet;

const SEED: &str = "dependency-seed";
const SEED_CONDITION: &str = "github.event_name == 'push' && github.ref == format('refs/heads/{0}', github.event.repository.default_branch) && github.ref_protected == true";

pub(super) fn assert_inventory(workflow: &Value, needs: &BTreeSet<String>) -> Result<()> {
    let jobs = workflow["jobs"].as_object().context("CI has jobs")?;
    let seed = jobs
        .get(SEED)
        .context("root workspace has a dependency seed")?;
    ensure!(
        seed["name"] == "Dependency seed"
            && seed["runs-on"] == "ubuntu-26.04"
            && seed["env"].get("VELNOR_INTERNAL_OP").is_none()
            && seed["if"] == SEED_CONDITION
            && seed["timeout-minutes"] == 10
            && job_needs(seed)? == BTreeSet::from(["plan".to_owned()])
            && seed.get("outputs").is_none()
            && seed.get("permissions").is_none()
            && seed.get("environment").is_none()
            && seed.get("continue-on-error") == Some(&Value::Bool(true)),
        "only the bounded protected-main dependency seed is outside Required"
    );
    let steps = seed["steps"].as_array().context("seed has steps")?;
    ensure!(
        !steps.iter().any(|step| {
            step["env"].get("VELNOR_INTERNAL_OP").is_some()
                || step["with"].get("digest").is_some()
                || step["run"].as_str().is_some_and(|run| {
                    run.contains("velnor-actions") || run.contains("task-report")
                })
                || step["uses"].as_str().is_some_and(|uses| {
                    uses.contains("declared-task-") || uses.contains("upload-artifact")
                })
        }),
        "optional dependency seed cannot own task execution or report artifacts"
    );
    assert_compile_lanes(steps)?;
    for (id, job) in jobs {
        if id != SEED {
            ensure!(
                job.get("continue-on-error").is_none(),
                "only the optional dependency seed may tolerate job failure: {id}"
            );
            if job.get("needs").is_some() {
                ensure!(
                    !job_needs(job)?.contains(SEED) && job["needs"].as_str() != Some(SEED),
                    "execution/setup job cannot depend on optional seed: {id}"
                );
            }
        }
    }
    let expected = jobs
        .keys()
        .filter(|id| !matches!(id.as_str(), "required" | "required-gate" | SEED))
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        *needs == expected,
        "Required covers every execution/setup job and excludes exactly the dependency seed"
    );
    Ok(())
}

fn assert_compile_lanes(steps: &[Value]) -> Result<()> {
    for (name, argv) in [
        (
            "Compile test executables (seed)",
            "nextest list --list-type binaries-only --locked --offline --manifest-path Cargo.toml --workspace",
        ),
        (
            "Compile Clippy lane (seed)",
            "clippy --locked --offline --manifest-path Cargo.toml --workspace --all-targets --target-dir target/velnor-clippy",
        ),
        (
            "Compile documentation lane (seed)",
            "doc --locked --offline --manifest-path Cargo.toml --workspace --no-deps --target-dir target/velnor-doc",
        ),
    ] {
        let matching = steps
            .iter()
            .filter(|step| step["name"] == name)
            .collect::<Vec<_>>();
        ensure!(matching.len() == 1, "exactly one seed compile lane: {name}");
        ensure!(
            matching[0]["run"]
                .as_str()
                .and_then(|run| run.strip_suffix(argv))
                .is_some_and(|prefix| prefix.ends_with("-- mbx ")),
            "seed lane is the bounded workspace compile command: {name}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::{repository_root, workflow_jobs};
    use serde_json::json;

    fn actual() -> Result<(Value, BTreeSet<String>)> {
        let (workflow, _) = workflow_jobs(&repository_root())?;
        let needs = job_needs(&workflow["jobs"]["required"])?;
        Ok((workflow, needs))
    }

    #[test]
    fn seed_exception_does_not_hide_missing_execution_or_setup_jobs() -> Result<()> {
        let (workflow, needs) = actual()?;
        assert_inventory(&workflow, &needs)?;
        for id in [
            "plan",
            "actionlint",
            "check-repository-policy",
            "rust-jackin-image",
        ] {
            let mut missing = needs.clone();
            ensure!(missing.remove(id), "actual Required includes {id}");
            ensure!(
                assert_inventory(&workflow, &missing).is_err(),
                "missing {id}"
            );
        }
        let mut extra = workflow.clone();
        extra["jobs"]["dependency-seed-forged"] = json!({"name": "Dependency seed"});
        ensure!(
            assert_inventory(&extra, &needs).is_err(),
            "no prefix exemption"
        );
        let mut required_seed = needs;
        required_seed.insert(SEED.to_owned());
        ensure!(assert_inventory(&workflow, &required_seed).is_err());
        Ok(())
    }

    #[test]
    fn only_seed_job_may_tolerate_failure() -> Result<()> {
        let (workflow, needs) = actual()?;
        ensure!(
            workflow["jobs"][SEED].get("continue-on-error") == Some(&Value::Bool(true)),
            "root seed requires job-level failure tolerance"
        );
        let seed_steps = workflow["jobs"][SEED]["steps"]
            .as_array()
            .context("seed steps")?;
        ensure!(
            seed_steps
                .iter()
                .any(|step| step["continue-on-error"] == true),
            "step-level cache-save failure handling remains separate"
        );

        let mut missing = workflow.clone();
        missing["jobs"][SEED]
            .as_object_mut()
            .context("seed job")?
            .remove("continue-on-error");
        ensure!(
            assert_inventory(&missing, &needs).is_err(),
            "missing seed failure tolerance must not validate"
        );
        for value in [json!(false), json!("true")] {
            let mut changed = workflow.clone();
            changed["jobs"][SEED]["continue-on-error"] = value;
            ensure!(
                assert_inventory(&changed, &needs).is_err(),
                "only boolean true is accepted for the seed"
            );
        }

        for id in ["rust-jackin-image", "plan", "required"] {
            let mut changed = workflow.clone();
            changed["jobs"][id]["continue-on-error"] = json!(true);
            ensure!(
                assert_inventory(&changed, &needs).is_err(),
                "mandatory crate/setup/verdict job cannot tolerate job failure: {id}"
            );
        }
        Ok(())
    }

    #[test]
    fn seed_exception_requires_protected_push_and_no_report_authority() -> Result<()> {
        let (workflow, needs) = actual()?;
        for (key, value) in [
            ("if", json!("${{ always() }}")),
            ("runs-on", json!("self-hosted")),
            ("timeout-minutes", json!(35)),
            ("needs", json!([])),
            ("outputs", json!({"task_report": "untrusted"})),
            ("permissions", json!({"contents": "write"})),
            ("environment", json!("release")),
        ] {
            let mut changed = workflow.clone();
            changed["jobs"][SEED][key] = value;
            ensure!(assert_inventory(&changed, &needs).is_err(), "changed {key}");
        }
        for uses in [
            "./.github/actions/declared-task-0",
            "actions/upload-artifact@unexpected",
        ] {
            let mut changed = workflow.clone();
            changed["jobs"][SEED]["steps"]
                .as_array_mut()
                .context("steps")?
                .push(json!({"uses": uses}));
            ensure!(
                assert_inventory(&changed, &needs).is_err(),
                "report step {uses}"
            );
        }
        let mut operation = workflow.clone();
        operation["jobs"][SEED]["steps"]
            .as_array_mut()
            .context("steps")?
            .push(json!({"run": "velnor-actions execute-task --digest forged"}));
        ensure!(assert_inventory(&operation, &needs).is_err());
        let mut dependent = workflow;
        dependent["jobs"]["rust-jackin-image"]["needs"]
            .as_array_mut()
            .context("needs")?
            .push(json!(SEED));
        ensure!(assert_inventory(&dependent, &needs).is_err());
        dependent["jobs"]["rust-jackin-image"]["needs"] = json!(SEED);
        ensure!(assert_inventory(&dependent, &needs).is_err());
        Ok(())
    }

    #[test]
    fn seed_exception_requires_all_compile_lanes_and_no_hidden_native_operation() -> Result<()> {
        let (workflow, needs) = actual()?;
        let mut empty = workflow.clone();
        empty["jobs"][SEED]["steps"] = json!([]);
        ensure!(assert_inventory(&empty, &needs).is_err());
        for name in [
            "Compile test executables (seed)",
            "Compile Clippy lane (seed)",
            "Compile documentation lane (seed)",
        ] {
            let mut missing = workflow.clone();
            missing["jobs"][SEED]["steps"]
                .as_array_mut()
                .context("steps")?
                .retain(|step| step["name"] != name);
            ensure!(
                assert_inventory(&missing, &needs).is_err(),
                "missing {name}"
            );
        }
        let mut hidden = workflow;
        hidden["jobs"][SEED]["steps"]
            .as_array_mut()
            .context("steps")?
            .push(json!({"env": {"VELNOR_INTERNAL_OP": "execute-v1"}, "run": "$SDK"}));
        ensure!(assert_inventory(&hidden, &needs).is_err());
        Ok(())
    }
}
