// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn generated_evidence_workflows_are_complete_and_tamper_bound() {
    // Post-#1110 the repo no longer carries the velnor-workflow contract or
    // generator state (main commit 6c389d38e deleted .github-gen/ and
    // .github/ci/ as "legacy velnor-workflow generator files"), and the
    // all-branches consolidation dropped the two evidence-observer workflow
    // stubs because the pinned renderer rejects non-generated files. The
    // tamper-binding assertions below therefore run against frozen fixtures
    // (the last rendered observer workflows) plus a synthesized workflow
    // contract and ownership state.
    let directory = tempfile::tempdir().unwrap();
    let copy = directory.path();
    for (relative, contents) in [
        (
            ".github/workflows/ci-evidence.yml",
            CI_EVIDENCE_WORKFLOW_FIXTURE,
        ),
        (
            ".github/workflows/ci-push-head-ledger.yml",
            CI_PUSH_HEAD_LEDGER_WORKFLOW_FIXTURE,
        ),
    ] {
        let destination = copy.join(relative);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, contents).unwrap();
    }
    let contract = "schema = 2\n\
         \n\
         [generator]\n\
         repository = \"jackin-project/jackin\"\n\
         revision = \"0123456789abcdef0123456789abcdef01234567\"\n\
         \n\
         [workflow]\n\
         default_branch = \"main\"\n\
         providers = [\"github-hosted\"]\n\
         automatic_providers = [\"github-hosted\"]\n\
         \n\
         [[check_profile]]\n\
         id = \"ci-evidence\"\n\
         tasks = [\"ci-evidence\"]\n\
         artifacts = [\"target/ci-evidence/\"]\n\
         runner = \"github\"\n\
         status = \"advisory\"\n\
         env = { GH_TOKEN = \"${{ github.token }}\" }\n\
         \n\
         [[check_profile]]\n\
         id = \"ci-push-head-ledger\"\n\
         tasks = [\"ci-push-head-ledger\"]\n\
         artifacts = [\"target/ci-push-head-ledger/\"]\n\
         runner = \"github\"\n\
         status = \"advisory\"\n\
         env = { GH_TOKEN = \"${{ github.token }}\" }\n\
         \n\
         [[declare]]\n\
         primitive = \"scheduled-checks\"\n\
         file = \"ci-evidence.yml\"\n\
         args = { profiles = [\"ci-evidence\"] }\n\
         \n\
         [[declare]]\n\
         primitive = \"scheduled-checks\"\n\
         file = \"ci-push-head-ledger.yml\"\n\
         args = { profiles = [\"ci-push-head-ledger\"], events = [\"push\"], branches = [\"main\"] }\n";
    let contract_path = copy.join(WORKFLOW_CONTRACT_PATH);
    fs::create_dir_all(contract_path.parent().unwrap()).unwrap();
    fs::write(&contract_path, contract).unwrap();
    let mut state = format!(
        "# Generated ownership state; do not edit.\n\
         schema = {GENERATED_STATE_SCHEMA}\n\
         [inputs]\n\
         config\t{}\n\
         scan\t{}\n\
         generator\t{GENERATED_STATE_GENERATOR}\n\
         [outputs]\n",
        fnv1a_digest(contract.as_bytes()),
        fnv1a_digest(b"fixture-scan"),
    );
    for relative in [
        ".github/workflows/ci-evidence.yml",
        ".github/workflows/ci-push-head-ledger.yml",
    ] {
        let bytes = fs::read(copy.join(relative)).unwrap();
        state.push_str(&format!("{relative}\t{}\n", fnv1a_digest(&bytes)));
    }
    let state_path = copy.join(WORKFLOW_STATE_PATH);
    fs::create_dir_all(state_path.parent().unwrap()).unwrap();
    fs::write(&state_path, &state).unwrap();
    assert!(!copy.join("mise.toml").exists());
    validate_workflow_contract(copy).unwrap();
    let pristine_state = fs::read(&state_path).unwrap();

    let state_path = copy.join(WORKFLOW_STATE_PATH);
    let tampered_state = fs::read_to_string(&state_path).unwrap().replace(
        &format!("generator\t{GENERATED_STATE_GENERATOR}"),
        "generator\tinvalid",
    );
    fs::write(&state_path, tampered_state).unwrap();
    let error = validate_workflow_contract(copy).unwrap_err();
    assert!(
        error.to_string().contains("generator identity"),
        "error: {error:#}"
    );

    fs::write(&state_path, &pristine_state).unwrap();
    let workflow_path = copy.join(".github/workflows/ci-evidence.yml");
    let workflow_bytes = fs::read(&workflow_path).unwrap();
    let workflow_text = String::from_utf8(workflow_bytes.clone()).unwrap();
    let tampered_workflow = workflow_text.replace(
        "  workflow_dispatch:\n",
        "  workflow_dispatch:\n  pull_request:\n",
    );
    assert_ne!(tampered_workflow, workflow_text);
    fs::write(&workflow_path, tampered_workflow.as_bytes()).unwrap();
    let relative = ".github/workflows/ci-evidence.yml";
    let original_hash = fnv1a_digest(&workflow_bytes);
    let tampered_hash = fnv1a_digest(tampered_workflow.as_bytes());
    let expected_row = format!("{relative}\t{original_hash}");
    let replacement_row = format!("{relative}\t{tampered_hash}");
    let state_text = fs::read_to_string(&state_path).unwrap();
    assert!(state_text.contains(&expected_row));
    fs::write(
        &state_path,
        state_text.replacen(&expected_row, &replacement_row, 1),
    )
    .unwrap();
    let error = validate_workflow_contract(copy).unwrap_err();
    assert!(
        error.to_string().contains("unexpected event triggers"),
        "error: {error:#}"
    );

    fs::write(&state_path, &pristine_state).unwrap();
    fs::write(&workflow_path, CI_EVIDENCE_WORKFLOW_FIXTURE).unwrap();
    #[expect(
        clippy::disallowed_methods,
        reason = "the synchronous xtask test edits a generated workflow fixture"
    )]
    fs::OpenOptions::new()
        .append(true)
        .open(copy.join(".github/workflows/ci-evidence.yml"))
        .unwrap()
        .write_all(b"# tampered\n")
        .unwrap();
    let error = validate_workflow_contract(copy).unwrap_err();
    assert!(
        error.to_string().contains("output hash mismatch"),
        "error: {error:#}"
    );
}

#[test]
fn generated_evidence_workflows_reject_skip_controls() {
    let workflow: serde_json::Value =
        serde_yaml_ng::from_slice(CI_EVIDENCE_WORKFLOW_FIXTURE.as_bytes()).unwrap();
    let contract = EvidenceWorkflowContract {
        file: DEFAULT_CI_EVIDENCE_WORKFLOW,
        display_name: "CI first-attempt evidence",
        artifact: CI_EVIDENCE_ARTIFACT_PATH,
        job_id: "ci-evidence",
        command: "MISE_AUTO_INSTALL=false mise exec -- mbx +1.97.1 xtask ci-evidence run",
        timeout: 30,
        schedule: Some("47 4 * * *"),
        push_main: false,
        advisory: true,
        full_history: false,
    };
    validate_generated_workflow_shape(&workflow, &contract).unwrap();

    let mut skipped_job = workflow.clone();
    skipped_job["jobs"]["ci-evidence"]
        .as_object_mut()
        .unwrap()
        .insert("if".to_owned(), serde_json::json!("false"));
    let error = validate_generated_workflow_shape(&skipped_job, &contract).unwrap_err();
    assert!(error.to_string().contains("unexpected fields"), "{error:#}");

    let mut optional_command = workflow;
    optional_command["jobs"]["ci-evidence"]["steps"][2]
        .as_object_mut()
        .unwrap()
        .insert("continue-on-error".to_owned(), serde_json::json!(true));
    let error = validate_generated_workflow_shape(&optional_command, &contract).unwrap_err();
    assert!(error.to_string().contains("unexpected fields"), "{error:#}");
}

#[test]
fn push_head_ledger_requires_full_history_checkout() {
    let workflow: serde_json::Value =
        serde_yaml_ng::from_slice(CI_PUSH_HEAD_LEDGER_WORKFLOW_FIXTURE.as_bytes()).unwrap();
    let contract = EvidenceWorkflowContract {
        file: DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW,
        display_name: "CI push-head ledger",
        artifact: CI_PUSH_HEAD_LEDGER_ARTIFACT_PATH,
        job_id: "ci-push-head-ledger",
        command: "MISE_AUTO_INSTALL=false mise exec -- mbx +1.97.1 xtask ci-evidence record-push",
        timeout: 10,
        schedule: None,
        push_main: true,
        advisory: true,
        full_history: true,
    };
    validate_generated_workflow_shape(&workflow, &contract).unwrap();

    let mut shallow = workflow;
    shallow["jobs"]["ci-push-head-ledger"]["steps"][0]["with"]
        .as_object_mut()
        .unwrap()
        .remove("fetch-depth");
    let error = validate_generated_workflow_shape(&shallow, &contract).unwrap_err();
    assert!(error.to_string().contains("checkout inputs"), "{error:#}");
}

#[test]
fn derived_history_rejects_edited_expected_source() {
    let mut evidence = evidence(
        vec![
            obligation("head", Cohort::CiMain),
            obligation("head", Cohort::Desktop),
        ],
        Vec::new(),
    );
    evidence.expected[0].commit.source = DenominatorSource::PushHeadLedger;
    evidence.expected[0].provenance = ObligationProvenance::PushHeadLedger;
    let error = validate_evidence(&evidence).unwrap_err();
    assert!(error.to_string().contains("not derived"));
}

#[test]
fn rolling_window_merge_prunes_attempts_outside_new_denominator() {
    let old = evidence(
        expected_for_sha("old-head"),
        vec![attempt(
            1,
            1,
            Cohort::CiMain,
            "old-head",
            OutcomeClass::Product,
            "2026-08-01T00:00:00Z",
        )],
    );
    let expected = expected_for_sha("new-head");
    let (denominator, history) = update_denominator(&expected);
    let merged = merge_evidence(
        old,
        EvidenceUpdate {
            expected,
            attempts: Vec::new(),
            unclassified_runs: Vec::new(),
            denominator,
            history,
            push_heads: Vec::new(),
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime: test_runtime(),
            provenance: test_provenance(),
        },
    )
    .unwrap();

    assert!(merged.attempts.is_empty());
    validate_evidence(&merged).unwrap();
}

#[test]
fn merge_rejects_immutable_runtime_replacement() {
    let expected = expected_for_sha("immutable-runtime");
    let (denominator, history) = update_denominator(&expected);
    let mut runtime = test_runtime();
    runtime.contract_digest = Some("f".repeat(64));
    let error = merge_evidence(
        evidence(expected.clone(), Vec::new()),
        EvidenceUpdate {
            expected,
            attempts: Vec::new(),
            unclassified_runs: Vec::new(),
            denominator,
            history,
            push_heads: Vec::new(),
            repository: "example/repo".to_owned(),
            window: TimeWindow {
                since: "2026-09-21T00:00:00Z".to_owned(),
                until: "2026-09-23T00:00:00Z".to_owned(),
            },
            runtime,
            provenance: test_provenance(),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("immutable identity differs"));
}

#[test]
fn api_timestamps_are_canonical_and_plus_safe() {
    assert_eq!(
        api_timestamp("2026-09-22T00:00:00+07:00"),
        "2026-09-21T17:00:00Z"
    );
    assert_eq!(api_timestamp("not-a-timestamp"), "not-a-timestamp");
}
