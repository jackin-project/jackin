// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::LabelSelector;
use jackin_openshell::{Gateway as _, SandboxQuery};
use jackin_xtask::StepId;

use super::scenario::{ExportMode, J05Fault, Ran, Scenario};

async fn survivors(scenario: &Scenario) -> (Vec<String>, Vec<String>) {
    let query = SandboxQuery::Labels(LabelSelector::managed());
    let views = scenario.gateway.find_sandboxes(&query).await.expect("find");
    let mut sandboxes: Vec<String> = views.iter().map(|view| view.name.to_string()).collect();
    sandboxes.sort();
    let mut providers: Vec<String> = scenario
        .fake
        .providers()
        .iter()
        .map(ToString::to_string)
        .collect();
    providers.sort();
    (sandboxes, providers)
}

async fn only_foreign(case: &str, scenario: &Scenario) {
    let (sandboxes, providers) = survivors(scenario).await;
    assert_eq!(sandboxes, ["jackin-mine-0000"], "{case}: sandboxes");
    assert_eq!(providers, ["jackin-mine-codex"], "{case}: providers");
    assert!(
        !scenario.world.scratch_root.join(&scenario.tag).exists(),
        "{case}: the scratch directory is gone"
    );
    assert!(
        scenario.state().containers.is_empty(),
        "{case}: no engine container"
    );
}

fn clean_line(ran: &Ran) -> String {
    ran.stdout
        .lines()
        .find(|line| line.starts_with("CLEAN"))
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test]
async fn cleanup_leaves_nothing_after_a_failure_in_the_middle_of_the_journey() {
    let scenario = Scenario::new().await;
    scenario.knobs().j05_fault = Some(J05Fault::DropFile);
    let ran = scenario.run().await;
    ran.expect("J05 fails", &[("J05", "FAIL")]);
    assert!(clean_line(&ran).starts_with("CLEAN PASS"), "{}", ran.stdout);
    only_foreign("after a failed step", &scenario).await;
}

#[tokio::test]
async fn cleanup_also_runs_after_timeout_panic_interrupt_and_signals() {
    let mut overrun = Scenario::new().await;
    overrun.budgets.step_caps[StepId::J04 as usize] = Duration::from_millis(300);
    overrun.budgets.exec = Duration::from_secs(20);
    overrun.knobs().hang_probe = true;
    let ran = overrun.run().await;
    assert_eq!(ran.status_of("J04"), "FAIL");
    assert!(clean_line(&ran).starts_with("CLEAN PASS"), "{}", ran.stdout);
    only_foreign("after a timeout", &overrun).await;

    let panics = Scenario::new().await;
    panics.knobs().panic_in_probe = Some("deliberate".to_owned());
    let ran = panics.run().await;
    assert!(ran.reason_of("J04").contains("panicked"), "{}", ran.stdout);
    assert!(clean_line(&ran).starts_with("CLEAN PASS"));
    only_foreign("after a panic", &panics).await;

    let interrupted = Scenario::new().await;
    interrupted.knobs().cancel_at = Some("ps -eo".to_owned());
    let ran = interrupted.run().await;
    assert_eq!(ran.status.code(), 130, "{}", ran.stdout);
    for id in [
        "J07", "J08", "J09", "J10", "J11", "J12", "J13", "J14", "J15", "J16", "J17",
    ] {
        assert_eq!(ran.status_of(id), "SKIP", "{id}");
        assert_eq!(ran.reason_of(id), "interrupted", "{id}");
    }
    assert!(clean_line(&ran).starts_with("CLEAN PASS"), "{}", ran.stdout);
    only_foreign("after the token fired", &interrupted).await;
}

#[tokio::test]
async fn cleanup_also_runs_after_sigint_and_sigterm() {
    for (name, number) in [("SIGINT", 2), ("SIGTERM", 15)] {
        let scenario = Scenario::new().await;
        scenario.knobs().self_signals = vec![
            ("usage --json".to_owned(), number),
            ("account rm".to_owned(), number),
        ];
        let ran = scenario.run().await;
        assert_eq!(ran.status.code(), 130, "{name}: {}", ran.stdout);
        assert!(
            ran.stderr.contains("cleanup in progress"),
            "{name}: a second signal during cleanup is ignored: {}",
            ran.stderr
        );
        assert!(
            clean_line(&ran).starts_with("CLEAN PASS"),
            "{name}: {}",
            ran.stdout
        );
        only_foreign(name, &scenario).await;
    }
}

#[tokio::test]
async fn a_cleanup_failure_is_a_clean_fail_line_and_does_not_stop_the_rest() {
    let scenario = Scenario::new().await;
    scenario.knobs().fail_delete_after = Some(("usage --json".to_owned(), 2));
    scenario.knobs().stuck_provider = Some("e2e-abcd-fxr".to_owned());
    let ran = scenario.run().await;
    ran.expect("every step passes, cleanup does not", &[("CLEAN", "FAIL")]);
    let reason = ran.reason_of("CLEAN");
    assert!(
        reason.contains("sandbox jackin-") && reason.contains("sandbox delete"),
        "{reason}"
    );
    assert!(
        reason.contains("provider jackin-e2e-abcd-fxr-codex remains"),
        "a second reason in the same line: {reason}"
    );
    let (sandboxes, providers) = survivors(&scenario).await;
    assert_eq!(
        sandboxes.len(),
        2,
        "the foreign sandbox and the one that could not be deleted: {sandboxes:?}"
    );
    assert!(providers.iter().any(|name| name.contains("fxr")));
    assert!(
        !providers
            .iter()
            .any(|name| name.contains("a1") && name.contains("abcd")),
        "the other providers are gone"
    );
}

#[tokio::test]
async fn cleanup_is_idempotent_and_only_touches_this_runs_tag() {
    let mut scenario = Scenario::new().await;
    scenario.knobs().foreign_run_at = Some("load sentinel".to_owned());
    let ran = scenario.run().await;
    ran.expect("a concurrent run appears mid-run", &[]);
    let (sandboxes, providers) = survivors(&scenario).await;
    assert_eq!(
        sandboxes,
        ["jackin-main-ffff", "jackin-mine-0000"],
        "the other run's sandbox survives"
    );
    assert_eq!(
        providers,
        ["jackin-e2e-ffff-a1-codex", "jackin-mine-codex"],
        "the other run's provider survives"
    );

    let refused = scenario.run().await;
    assert_eq!(
        refused.status.code(),
        2,
        "stale resources block a normal run"
    );
    scenario.args.sweep = true;
    scenario.args.role = None;
    let swept = scenario.run().await;
    assert_eq!(swept.status.code(), 0, "{}", swept.stdout);
    scenario.args.sweep = false;
    scenario.args.role = Some(scenario.world.role.clone());
    scenario.tag = "beef".to_owned();
    let again = scenario.run().await;
    again.expect("a later run with a fresh tag", &[]);
    only_foreign("after the second run", &scenario).await;
}

fn seed_stale(scenario: &Scenario) {
    scenario
        .gateway
        .add_sandbox(
            "jackin-main-zzzz",
            vec![
                ("jackin.dev/managed".to_owned(), "true".to_owned()),
                (
                    "jackin.dev/workspace".to_owned(),
                    "e2e-zzzz-main".to_owned(),
                ),
            ],
        )
        .expect("stale sandbox");
}

#[tokio::test]
async fn sweep_removes_only_e2e_leftovers() {
    let mut scenario = Scenario::new().await;
    seed_stale(&scenario);
    scenario
        .gateway
        .put_codex_provider("e2e-zzzz-a1")
        .await
        .expect("stale provider");
    scenario.args.sweep = true;
    scenario.args.role = None;
    std::fs::remove_file(scenario.world.codex_home.join("auth.json")).expect("no login");
    let held = jackin_xtask::A1Lock::acquire(&scenario.world.lock_dir).expect("another holder");
    let ran = scenario.run().await;
    assert_eq!(ran.status.code(), 0, "{} {}", ran.stdout, ran.stderr);
    assert!(ran.stdout.starts_with("SWEEP PASS"), "{}", ran.stdout);
    only_foreign("after the sweep", &scenario).await;
    let again = scenario.run().await;
    assert_eq!(again.status.code(), 0, "nothing to sweep is still success");
    drop(held);

    let failing = Scenario::new().await;
    seed_stale(&failing);
    failing.fake.inject(jackin_testkit::Fault::FailNext {
        op: jackin_openshell::GatewayOp::Delete,
        error: jackin_openshell::GatewayError::Cancelled {
            op: jackin_openshell::GatewayOp::Delete,
            group: None,
        },
    });
    let mut args = failing.args.clone();
    args.sweep = true;
    let mut failing = failing;
    failing.args = args;
    let ran = failing.run().await;
    assert_eq!(ran.status.code(), 1, "{}", ran.stdout);
    assert!(ran.stdout.contains("SWEEP FAIL"), "{}", ran.stdout);
}

#[tokio::test]
async fn a_run_refuses_to_start_over_stale_e2e_resources() {
    let scenario = Scenario::new().await;
    seed_stale(&scenario);
    let ran = scenario.run().await;
    assert_eq!(ran.status.code(), 2);
    assert!(ran.stderr.contains("--sweep"), "{}", ran.stderr);
    assert!(ran.stdout.is_empty());
    let (sandboxes, providers) = survivors(&scenario).await;
    assert_eq!(
        sandboxes,
        ["jackin-main-zzzz", "jackin-mine-0000"],
        "no new resource was created"
    );
    assert_eq!(providers, ["jackin-mine-codex"]);
    assert!(!scenario.world.scratch_root.join("abcd").exists());
}

#[tokio::test]
async fn the_engine_container_is_removed_before_its_sandbox_is_deleted() {
    let scenario = Scenario::new().await;
    let ran = scenario.run().await;
    ran.expect("a clean teardown", &[]);
    assert!(
        scenario.state().containers.is_empty(),
        "the container is gone"
    );

    let stuck = Scenario::new().await;
    stuck.exec_rule("docker rm -f", 1, "");
    let ran = stuck.run().await;
    ran.expect("the removal fails", &[("CLEAN", "FAIL")]);
    ran.reason_has(
        "leak",
        "CLEAN",
        "leaked container e2e-abcd in the engine on port 2376",
    );
}

fn init_role_repo(scenario: &Scenario) {
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "role",
        ],
    ] {
        let status = jackin_testkit::git_command()
            .args(&args)
            .current_dir(&scenario.world.role)
            .status()
            .expect("git");
        assert!(status.success());
    }
}

#[tokio::test]
async fn an_unreadable_role_repository_stops_the_run_before_the_journey() {
    let scenario = Scenario::new().await;
    init_role_repo(&scenario);
    scenario.knobs().git_fail = Some((
        "--format=%(refname) refs/heads/jackin".to_owned(),
        "role".to_owned(),
    ));
    let ran = scenario.run().await;
    assert_eq!(ran.status.code(), 2, "{}", ran.stderr);
    assert!(
        ran.stderr
            .contains("cannot list the refs of the role repository"),
        "{}",
        ran.stderr
    );
}

#[tokio::test]
async fn host_refs_are_deleted_and_the_role_repository_ref_set_is_checked() {
    let clean = Scenario::new().await;
    init_role_repo(&clean);
    let ran = clean.run().await;
    ran.expect("refs only in the fixtures", &[]);
    assert!(
        !clean.world.scratch_root.join("abcd").exists(),
        "the fixture repositories are gone"
    );

    let leaking = Scenario::new().await;
    init_role_repo(&leaking);
    leaking.knobs().export_mode = ExportMode::RoleRepo;
    let ran = leaking.run().await;
    ran.expect(
        "an export wrote into the role repository",
        &[("CLEAN", "FAIL")],
    );
    ran.reason_has("role repository", "CLEAN", "role repository");

    let broken = Scenario::new().await;
    init_role_repo(&broken);
    broken.knobs().git_fail = Some((
        "--format=%(refname) refs/heads/jackin".to_owned(),
        "projects".to_owned(),
    ));
    let ran = broken.run().await;
    assert_eq!(ran.status_of("CLEAN"), "FAIL", "{}", ran.stdout);
    ran.reason_has("host refs unreadable", "CLEAN", "cannot list host refs");
}
