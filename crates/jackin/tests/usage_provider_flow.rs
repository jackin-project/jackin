// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(all(unix, feature = "test-support"))]
#![expect(
    clippy::disallowed_methods,
    reason = "integration driver controls isolated child processes and local sockets"
)]

use std::fs::{self, OpenOptions};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tempfile::TempDir;

#[path = "usage_provider_flow/auth_outcomes.rs"]
mod auth_outcomes;

const FIRST_TOKEN: &str = "synthetic-claude-access-token-one";
const KEYCHAIN_SERVICE: &str = "synthetic-claude-keychain-service";
const INITIAL_WALL_EPOCH: i64 = 1_700_000_000;
const INITIAL_MONOTONIC_MILLIS: u64 = 1_000;
const FIRST_FIVE_HOUR_RESET: i64 = INITIAL_WALL_EPOCH + 6_400;
const SEVEN_DAY_RESET: i64 = INITIAL_WALL_EPOCH + 604_800;
const MAX_COORDINATOR_STATE_BYTES: u64 = 512 * 1024;
const MAX_MONITOR_STATE_BYTES: u64 = 32 * 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const BROKER_LEASE_STALE_AFTER_SECONDS: i64 = 30;

static PROVIDER_FLOW_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn acquire_provider_flow_test_lock() -> MutexGuard<'static, ()> {
    PROVIDER_FLOW_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one integration scenario keeps the real CLI lifecycle and restart assertions in order"
)]
fn cli_auth_binding_watch_and_natural_provider_poll_survive_restart() -> Result<()> {
    let _test_guard = acquire_provider_flow_test_lock();
    let mut fixture = Fixture::new()?;
    let stale_lease_pid = fixture.seed_stale_v8_lease()?;
    fixture.write_credential(FIRST_TOKEN)?;
    fixture.start_auth()?;

    let service = fixture.wait_service_ready()?;
    fixture.assert_v9_lease_replaced(stale_lease_pid)?;
    let first_credential_reads = fixture.wait_ticker_ready()?;
    ensure!(
        first_credential_reads == 1,
        "fake keychain reader was not used exactly once before the first ticker step"
    );
    let source_id = service["status"]["experimental_collector_source"]
        .as_str()
        .context("foreground service did not expose its selected source ID")?
        .to_owned();

    let binding_reply = fixture.run_pty_json(&[
        "binding",
        "confirm",
        "--provider",
        "claude",
        "--provider-account",
        &source_id,
        "--account",
        "local-claude-account",
        "--approve-experimental-collector",
        "--operator-label",
        "synthetic provider-flow test",
        "--confirm",
    ])?;
    ensure!(
        binding_reply["result"] == "account_bound",
        "binding confirmation did not return account_bound"
    );
    let binding = &binding_reply["binding"];
    ensure!(
        binding["provider_account_id"].as_str() == Some(source_id.as_str()),
        "binding did not retain the exact foreground source ID"
    );
    ensure!(
        binding["experimental_collector_approved"] == true,
        "binding did not retain explicit collector approval"
    );
    let binding_id = binding["binding_id"]
        .as_str()
        .context("binding response omitted its ID")?
        .to_owned();
    let binding_revision = binding["revision"]
        .as_u64()
        .context("binding response omitted its revision")?;

    let observe_reply = fixture.run_json(&[
        "monitor",
        "observe",
        "--provider",
        "claude",
        "--binding",
        &binding_id,
        "--binding-revision",
        &binding_revision.to_string(),
        "--idempotency-key",
        "synthetic-provider-flow-observer-v1",
        "--experimental-collector",
    ])?;
    ensure!(
        observe_reply["result"] == "started",
        "observer command did not return started"
    );
    let monitor_id = observe_reply["status"]["monitor_id"]
        .as_str()
        .context("observer response omitted its monitor ID")?
        .to_owned();

    let first_request_wall_epoch = fixture.initial_wall_epoch + 1;
    let first_request_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 1_000;
    ensure!(
        fixture.set_clock(first_request_wall_epoch, first_request_monotonic_millis)? == 1,
        "fake keychain reader was not used exactly once before provider collection"
    );

    // The fake endpoint holds the provider response while the real CLI watch
    // attaches. Releasing it must produce a new durable event visible to watch.
    let first_request = next_provider_request(
        &fixture,
        &monitor_id,
        &source_id,
        "first",
        first_credential_reads,
    )?;
    assert_provider_request(&first_request)?;
    let first_in_flight_timing = fixture.wait_for_coordinator_attempt(
        &source_id,
        first_request_wall_epoch,
        Duration::from_secs(5),
    )?;
    let mut first_watch = fixture.start_watch(&monitor_id)?;
    let first_sequence = first_watch.wait_for_event(&monitor_id, Duration::from_secs(10))?;
    let first_five_hour_reset = FIRST_FIVE_HOUR_RESET;
    let first_seven_day_reset = SEVEN_DAY_RESET;
    let first_five_hour_reset_at = rfc3339_utc_from_epoch(first_five_hour_reset);
    let first_seven_day_reset_at = rfc3339_utc_from_epoch(first_seven_day_reset);
    first_request.respond.send(HttpResponse::ok(
        37,
        23,
        &first_five_hour_reset_at,
        &first_seven_day_reset_at,
    ))?;
    fixture.wait_for_coordinator_completion(
        &source_id,
        CoordinatorCompletionExpectation {
            generation: first_in_flight_timing.generation,
            provider_invoked_at_epoch: first_request_wall_epoch,
            fetched_at_epoch: first_request_wall_epoch,
            session_used_label: "37% used",
            weekly_used_label: "23% used",
            timeout: Duration::from_secs(10),
        },
    )?;
    let first_publication_wall_epoch = fixture.initial_wall_epoch + 2;
    let first_publication_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 2_000;
    let first_quota_expectation = QuotaExpectation {
        five_hour_used: 3_700,
        seven_day_used: 2_300,
        five_hour_reset: first_five_hour_reset,
        seven_day_reset: first_seven_day_reset,
        evidence_at_epoch: first_request_wall_epoch,
        received_at_min_epoch: first_request_wall_epoch,
        received_at_max_epoch: first_publication_wall_epoch,
    };
    ensure!(
        fixture.set_clock(
            first_publication_wall_epoch,
            first_publication_monotonic_millis
        )? == 1,
        "fake keychain reader was not used exactly once before first publication"
    );
    if let Err(watch_error) = first_watch.wait_for_quota_after(
        &monitor_id,
        first_sequence,
        first_quota_expectation,
        Duration::from_secs(15),
    ) {
        let status_args = ["status", "--monitor", &monitor_id];
        let diagnostic_status = match fixture.status(&monitor_id) {
            Ok(status) => json!({ "result": "status", "status": status }),
            Err(_) => fixture.diagnostic_json(&status_args),
        };
        let diagnostic_service = fixture.diagnostic_json(&["service", "status"]);
        let auth_child_exit_code = fixture
            .auth
            .as_mut()
            .and_then(|auth| auth.child.try_wait().ok().flatten())
            .and_then(|status| status.code());
        let diagnostic = json!({
            "monitor_status": diagnostic_status,
            "service_status": diagnostic_service,
            "http_request_count": fixture.http.request_count(),
            "auth_child_exit_code": auth_child_exit_code,
        });
        bail!(
            "CLI watch did not receive first provider quota update ({watch_error}); diagnostic state: {diagnostic}"
        );
    }
    first_watch.wait_for_exit(Duration::from_secs(5))?;
    drop(first_watch);

    let first_status = fixture.status(&monitor_id)?;
    assert_provider_quota(&first_status, first_quota_expectation)?;

    // Advance to +299s through the private clock socket and prove the natural
    // ticker still has not issued a second provider request.
    let pre_deadline_wall_epoch = fixture.initial_wall_epoch + 299;
    let pre_deadline_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 299_000;
    ensure!(
        fixture.set_clock(pre_deadline_wall_epoch, pre_deadline_monotonic_millis)? == 1,
        "fake keychain reader was not used exactly once during bootstrap"
    );
    thread::sleep(Duration::from_millis(750));
    ensure!(
        fixture.http.request_count() == 1,
        "provider polled before the 300-second interval elapsed"
    );
    let first_coordinator_timing = fixture.coordinator_timing(&source_id)?;
    assert_successful_coordinator_timing(&first_coordinator_timing)?;
    // Seed a reset-only statusline observation one second beyond the broker's
    // weekly reset. The next provider result has a newer source observation
    // but a reset one second earlier; it must still publish its own fields
    // without lowering the account-wide statusline watermark. This sample is
    // later pruned through ordinary CLI ingress to prove the watermark is
    // durable independently of its source session.
    let statusline_watermark_epoch = SEVEN_DAY_RESET + 1;
    let statusline_watermark_payload = serde_json::to_vec(&json!({
        "session_id": "synthetic-statusline-watermark",
        "rate_limits": {
            "seven_day": {
                "resets_at": statusline_watermark_epoch,
            }
        }
    }))?;
    let binding_revision_arg = binding_revision.to_string();
    let statusline_watermark_args = [
        "statusline",
        "ingest",
        "--binding",
        binding_id.as_str(),
        "--binding-revision",
        binding_revision_arg.as_str(),
    ];
    let prune_trigger_payload = serde_json::to_vec(&json!({
        "session_id": "synthetic-statusline-prune-trigger",
        "rate_limits": {},
    }))?;
    let statusline_watermark_wall_epoch = fixture.initial_wall_epoch + 300;
    let statusline_watermark_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 300_000;
    ensure!(
        fixture.set_clock(
            statusline_watermark_wall_epoch,
            statusline_watermark_monotonic_millis
        )? == 1,
        "fake keychain reader was not used exactly once before the statusline watermark sample"
    );
    let statusline_watermark_receipt =
        fixture.run_json_with_stdin(&statusline_watermark_args, &statusline_watermark_payload)?;
    ensure!(
        statusline_watermark_receipt["result"] == "ingested"
            && statusline_watermark_receipt["scope"]["scope"] == "bound_account"
            && statusline_watermark_receipt["account_id"] == "local-claude-account",
        "bound reset-only statusline sample was not ingested for the selected account"
    );
    assert_known_weekly_reset_watermark(
        &fixture.monitor_account("local-claude-account")?,
        statusline_watermark_epoch,
        statusline_watermark_wall_epoch,
    )?;
    ensure!(
        fixture.http.request_count() == 1,
        "provider polled at +300 before restart"
    );
    fixture.stop_auth()?;

    let restart_wall_epoch = fixture.initial_wall_epoch + 301;
    let restart_monotonic_millis = INITIAL_MONOTONIC_MILLIS
        .checked_add(301_000)
        .context("fixture monotonic clock overflow")?;
    fixture.write_fixture_config(restart_wall_epoch, restart_monotonic_millis)?;
    fixture.start_auth()?;
    fixture.wait_service_ready()?;
    let second_credential_reads = fixture.wait_ticker_ready()?;
    ensure!(
        second_credential_reads == 1,
        "fake keychain reader was not used exactly once before the restarted ticker step"
    );
    let recovered_coordinator_timing = fixture.coordinator_timing(&source_id)?;
    assert_successful_coordinator_timing(&recovered_coordinator_timing)?;
    ensure!(
        recovered_coordinator_timing == first_coordinator_timing,
        "restart changed the persisted successful provider attempt timing"
    );
    ensure!(
        fixture.http.request_count() == 1,
        "provider issued another request during restart recovery"
    );

    let restart_cooldown_wall_epoch = fixture.initial_wall_epoch + 600;
    let restart_cooldown_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 600_000;
    ensure!(
        fixture.set_clock(
            restart_cooldown_wall_epoch,
            restart_cooldown_monotonic_millis
        )? == 1,
        "fake keychain reader was not used exactly once before the recovered cooldown elapsed"
    );
    thread::sleep(Duration::from_millis(750));
    ensure!(
        fixture.http.request_count() == 1,
        "provider polled before the recovered 300-second restart cooldown elapsed"
    );

    let second_evidence_at_epoch = fixture.initial_wall_epoch + 601;
    let second_request_wall_epoch = second_evidence_at_epoch;
    let second_request_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 601_000;
    ensure!(
        fixture.set_clock(second_request_wall_epoch, second_request_monotonic_millis)? == 1,
        "fake keychain reader was not used exactly once after service restart"
    );

    let second_request = next_provider_request(
        &fixture,
        &monitor_id,
        &source_id,
        "second",
        second_credential_reads,
    )?;
    assert_provider_request(&second_request)?;
    let second_in_flight_timing = fixture.wait_for_coordinator_attempt(
        &source_id,
        second_request_wall_epoch,
        Duration::from_secs(5),
    )?;
    let mut second_watch = fixture.start_watch(&monitor_id)?;
    let second_sequence = second_watch.wait_for_event(&monitor_id, Duration::from_secs(10))?;
    // The reset-only source session is now 302 seconds old. A no-quota input
    // through the CLI triggers the real session-pruning path while the
    // provider HTTP response remains held.
    let prune_wall_epoch = fixture.initial_wall_epoch + 602;
    let prune_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 602_000;
    ensure!(
        fixture.set_clock(prune_wall_epoch, prune_monotonic_millis)? == 1,
        "fake keychain reader was not used exactly once before session pruning"
    );
    let prune_receipt =
        fixture.run_json_with_stdin(&statusline_watermark_args, &prune_trigger_payload)?;
    ensure!(
        prune_receipt["result"] == "ingested"
            && prune_receipt["scope"]["scope"] == "bound_account"
            && prune_receipt["account_id"] == "local-claude-account",
        "no-quota CLI callback did not reach the selected bound account"
    );
    let pruned_account = fixture.monitor_account("local-claude-account")?;
    ensure!(
        pruned_account["sessions"]["synthetic-statusline-watermark"].is_null(),
        "ordinary CLI ingress did not prune the expired statusline source session"
    );
    assert_known_weekly_reset_watermark(
        &pruned_account,
        statusline_watermark_epoch,
        statusline_watermark_wall_epoch,
    )?;
    let second_five_hour_reset = second_request_wall_epoch + 18_000;
    let second_seven_day_reset = SEVEN_DAY_RESET;
    let second_five_hour_reset_at = rfc3339_utc_from_epoch(second_five_hour_reset);
    let second_seven_day_reset_at = rfc3339_utc_from_epoch(second_seven_day_reset);
    second_request.respond.send(HttpResponse::ok(
        52,
        41,
        &second_five_hour_reset_at,
        &second_seven_day_reset_at,
    ))?;
    fixture.wait_for_coordinator_completion(
        &source_id,
        CoordinatorCompletionExpectation {
            generation: second_in_flight_timing.generation,
            provider_invoked_at_epoch: second_request_wall_epoch,
            fetched_at_epoch: second_request_wall_epoch + 1,
            session_used_label: "52% used",
            weekly_used_label: "41% used",
            timeout: Duration::from_secs(10),
        },
    )?;
    let second_publication_wall_epoch = fixture.initial_wall_epoch + 603;
    let second_publication_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 603_000;
    let second_quota_expectation = QuotaExpectation {
        five_hour_used: 5_200,
        seven_day_used: 4_100,
        five_hour_reset: second_five_hour_reset,
        seven_day_reset: second_seven_day_reset,
        evidence_at_epoch: fixture.initial_wall_epoch + 602,
        received_at_min_epoch: fixture.initial_wall_epoch + 602,
        received_at_max_epoch: fixture.initial_wall_epoch + 603,
    };
    ensure!(
        fixture.set_clock(
            second_publication_wall_epoch,
            second_publication_monotonic_millis
        )? == 1,
        "fake keychain reader was not used exactly once before second publication"
    );
    second_watch.wait_for_quota_after(
        &monitor_id,
        second_sequence,
        second_quota_expectation,
        Duration::from_secs(15),
    )?;
    second_watch.wait_for_exit(Duration::from_secs(5))?;
    drop(second_watch);

    let second_status = fixture.status(&monitor_id)?;
    assert_provider_quota(&second_status, second_quota_expectation)?;
    assert_known_weekly_reset_watermark(
        &fixture.monitor_account("local-claude-account")?,
        statusline_watermark_epoch,
        statusline_watermark_wall_epoch,
    )?;
    ensure!(
        second_status["five_hour"]["used_evidence"]["evidence_at_epoch"].as_i64()
            == Some(fixture.initial_wall_epoch + 602)
            && second_status["five_hour"]["used_evidence"]["evidence_received_at_epoch"]
                .as_i64()
                .is_some_and(|received| {
                    (fixture.initial_wall_epoch + 602..=fixture.initial_wall_epoch + 603)
                        .contains(&received)
                })
            && second_status["seven_day"]["used_evidence"]["evidence_at_epoch"].as_i64()
                == Some(fixture.initial_wall_epoch + 602)
            && second_status["seven_day"]["used_evidence"]["evidence_received_at_epoch"]
                .as_i64()
                .is_some_and(|received| {
                    (fixture.initial_wall_epoch + 602..=fixture.initial_wall_epoch + 603)
                        .contains(&received)
                }),
        "second provider response was not observed at +602 or received by +603"
    );
    // Stop before the successful poll's 300-second deadline and restart. The
    // retained last-good display must become stale without changing its
    // source or receipt evidence, and replaying a no-quota callback cannot
    // make it look fresh.
    let before_second_deadline = fixture.initial_wall_epoch + 901;
    ensure!(
        fixture.set_clock(before_second_deadline, INITIAL_MONOTONIC_MILLIS + 901_000)? == 1,
        "fake keychain reader was not used exactly once before the stale-display restart"
    );
    assert_request_count_remains(
        &fixture.http,
        2,
        Duration::from_millis(750),
        "provider polled before the second successful cooldown elapsed",
    )?;
    fixture.stop_auth()?;
    let stale_restart_epoch = fixture.initial_wall_epoch + 903;
    fixture.write_fixture_config(stale_restart_epoch, INITIAL_MONOTONIC_MILLIS + 903_000)?;
    fixture.start_auth()?;
    fixture.wait_service_ready()?;
    ensure!(
        fixture.wait_ticker_ready()? == 1,
        "fake keychain reader was not used exactly once before stale-display observation"
    );
    let stale_observation_epoch = fixture.initial_wall_epoch + 904;
    ensure!(
        fixture.set_clock(stale_observation_epoch, INITIAL_MONOTONIC_MILLIS + 904_000)? == 1,
        "fake keychain reader was not used exactly once before stale status"
    );
    let stale_status = fixture.status(&monitor_id)?;
    assert_retained_quota_is_stale(&stale_status, &second_status)?;
    let cached_statusline_receipt =
        fixture.run_json_with_stdin(&statusline_watermark_args, &prune_trigger_payload)?;
    ensure!(
        cached_statusline_receipt["result"] == "ingested",
        "replayed no-quota callback was not handled by the CLI ingress"
    );
    let after_cached_statusline = fixture.status(&monitor_id)?;
    assert_retained_quota_is_stale(&after_cached_statusline, &second_status)?;
    assert_known_weekly_reset_watermark(
        &fixture.monitor_account("local-claude-account")?,
        statusline_watermark_epoch,
        statusline_watermark_wall_epoch,
    )?;
    assert_request_count_remains(
        &fixture.http,
        2,
        Duration::from_millis(750),
        "provider polled before the restarted success cooldown elapsed",
    )?;

    fixture.stop_auth()?;
    Ok(())
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one migration scenario asserts the real CLI restart and cutoff repair in order"
)]
fn cli_v5_migration_admits_new_provider_reset_without_statusline_adapter() -> Result<()> {
    let _test_guard = acquire_provider_flow_test_lock();
    let mut flow = ProviderFlow::new("v5-reset-cutoff")?;
    seed_provider_quota(&flow)?;

    // Persist a +300 local observation epoch, just before the original
    // success cooldown expires. The synthetic v5 conversion below preserves
    // this exact legacy cutoff while removing all v6 reset metadata.
    let cutoff_epoch = INITIAL_WALL_EPOCH + 300;
    flow.set_clock(cutoff_epoch)?;
    let before_migration = flow.fixture.status(&flow.monitor_id)?;
    ensure!(
        before_migration["readiness"]["quota"] == "ready",
        "seed provider quota became stale before the legacy migration fixture"
    );
    let local_account_id = "local-claude-account-v5-reset-cutoff";
    let before_migration_account = flow.fixture.monitor_account(local_account_id)?;
    let prior_weekly_reset_barrier = before_migration_account["reset_barriers"][1].clone();
    ensure!(
        before_migration_account["latest_reset_epochs"][1].as_i64() == Some(SEVEN_DAY_RESET)
            && before_migration_account["statusline_reset_watermarks"][1] == "known_none",
        "provider-only seed did not establish a reset with no statusline adapter"
    );
    let prior_state = flow.fixture.monitor_state()?;
    ensure!(
        prior_state["last_now_epoch"].as_i64() == Some(cutoff_epoch),
        "monitor state did not persist the intended v5 migration cutoff"
    );
    flow.fixture.stop_auth()?;

    let (legacy_snapshot, migrated_cutoff) = flow.fixture.rewrite_monitor_snapshot_as_v5()?;
    ensure!(
        migrated_cutoff == cutoff_epoch
            && legacy_snapshot["schema_version"] == 5
            && legacy_snapshot["accounts"][local_account_id]["statusline_reset_watermarks"]
                .is_null()
            && legacy_snapshot["accounts"][local_account_id]["legacy_reset_admission_after_epochs"]
                .is_null(),
        "synthetic v5 source did not strip both v6 account fields and preserve its cutoff"
    );
    ensure!(
        legacy_snapshot["monitors"]
            .as_object()
            .is_some_and(|monitors| monitors.values().all(|monitor| {
                monitor["events"].as_array().is_some_and(|events| {
                    !events.is_empty()
                        && events
                            .iter()
                            .all(|event| event["status"]["schema_version"] == 5)
                })
            })),
        "synthetic v5 source did not restamp every nested event status"
    );

    let restart_epoch = INITIAL_WALL_EPOCH + 301;
    flow.fixture
        .write_fixture_config(restart_epoch, monotonic_millis_at(restart_epoch)?)?;
    flow.fixture.start_auth()?;
    let restarted_service = match flow.fixture.wait_service_ready() {
        Ok(service) => service,
        Err(error) => {
            let service_status = flow.fixture.diagnostic_json(&["service", "status"]);
            let auth_exit_code = flow
                .fixture
                .auth
                .as_mut()
                .and_then(|auth| auth.child.try_wait().ok().flatten())
                .and_then(|status| status.code());
            let migration_state = match flow.fixture.monitor_state() {
                Ok(state) => {
                    let account = &state["accounts"][local_account_id];
                    let session_count = account["sessions"].as_object().map(serde_json::Map::len);
                    json!({
                        "schema_version": state["schema_version"],
                        "last_now_epoch": state["last_now_epoch"],
                        "latest_reset_epochs": account["latest_reset_epochs"],
                        "statusline_reset_watermarks": account["statusline_reset_watermarks"],
                        "legacy_reset_admission_after_epochs": account["legacy_reset_admission_after_epochs"],
                        "session_count": session_count,
                    })
                }
                Err(_) => json!({ "readable": false }),
            };
            let preserved_state = flow
                .fixture
                .preserve_monitor_state_diagnostic()
                .map_or_else(
                    |_| json!({ "available": false }),
                    |path| json!({ "available": true, "path": path.display().to_string() }),
                );
            bail!(
                "foreground auth service did not become ready after v5 migration ({error}); sanitized diagnostics: {}",
                json!({
                    "service_status": service_status,
                    "auth_child_exit_code": auth_exit_code,
                    "auth_prepare_json": flow.fixture.auth_prepare_json_diagnostic(),
                    "migration_state": migration_state,
                    "preserved_state": preserved_state,
                    "http_counters": flow.fixture.http.diagnostic_json(),
                })
            )
        }
    };
    ensure!(
        restarted_service["status"]["experimental_collector_source"].as_str()
            == Some(flow.source_id.as_str()),
        "legacy migration restart changed the selected provider source"
    );
    flow.credential_reads = flow.fixture.wait_ticker_ready()?;
    ensure!(
        flow.credential_reads == 1,
        "fake keychain reader was not used once after legacy migration"
    );
    let migrated = flow.fixture.monitor_state()?;
    ensure!(
        migrated["schema_version"] == 6,
        "foreground broker did not migrate the v5 snapshot to v6"
    );
    ensure!(
        migrated["monitors"]
            .as_object()
            .is_some_and(|monitors| monitors.values().all(|monitor| {
                monitor["events"].as_array().is_some_and(|events| {
                    !events.is_empty()
                        && events
                            .iter()
                            .all(|event| event["status"]["schema_version"] == 6)
                })
            })),
        "foreground migration did not restamp nested event statuses to v6"
    );
    let migrated_account = &migrated["accounts"][local_account_id];
    ensure!(
        migrated_account["latest_reset_epochs"][1].as_i64() == Some(SEVEN_DAY_RESET)
            && migrated_account["statusline_reset_watermarks"][1] == "unknown"
            && migrated_account["legacy_reset_admission_after_epochs"][1].as_i64()
                == Some(cutoff_epoch)
            && migrated_account["reset_barriers"][1] == prior_weekly_reset_barrier
            && migrated_account["sessions"]
                .as_object()
                .is_some_and(serde_json::Map::is_empty),
        "v5 migration lost the provider-only global reset or derived metadata from statusline data"
    );

    let recovered_floor_minus_one = INITIAL_WALL_EPOCH + 600;
    flow.set_clock(recovered_floor_minus_one)?;
    assert_request_count_remains(
        &flow.fixture.http,
        1,
        Duration::from_millis(750),
        "provider polled before the restart recovery floor elapsed",
    )?;
    let lower_repair_source_epoch = INITIAL_WALL_EPOCH + 601;
    flow.set_clock(lower_repair_source_epoch)?;
    let request = next_provider_request(
        &flow.fixture,
        &flow.monitor_id,
        &flow.source_id,
        "v5 lower-reset repair",
        flow.credential_reads,
    )?;
    assert_provider_request(&request)?;
    let attempt = flow.fixture.wait_for_coordinator_attempt(
        &flow.source_id,
        lower_repair_source_epoch,
        Duration::from_secs(5),
    )?;
    let mut watch = flow.fixture.start_watch(&flow.monitor_id)?;
    let sequence = watch.wait_for_event(&flow.monitor_id, Duration::from_secs(10))?;
    let lower_five_hour_reset = lower_repair_source_epoch + 18_000;
    let lower_weekly_reset = SEVEN_DAY_RESET - 1;
    let lower_five_hour_reset_at = rfc3339_utc_from_epoch(lower_five_hour_reset);
    let lower_weekly_reset_at = rfc3339_utc_from_epoch(lower_weekly_reset);
    request.respond.send(HttpResponse::ok(
        52,
        41,
        &lower_five_hour_reset_at,
        &lower_weekly_reset_at,
    ))?;
    flow.fixture.wait_for_coordinator_completion(
        &flow.source_id,
        CoordinatorCompletionExpectation {
            generation: attempt.generation,
            provider_invoked_at_epoch: lower_repair_source_epoch,
            fetched_at_epoch: lower_repair_source_epoch,
            session_used_label: "52% used",
            weekly_used_label: "41% used",
            timeout: Duration::from_secs(10),
        },
    )?;
    let publication_epoch = INITIAL_WALL_EPOCH + 603;
    flow.set_clock(publication_epoch)?;
    let expected = QuotaExpectation {
        five_hour_used: 5_200,
        seven_day_used: 4_100,
        five_hour_reset: lower_five_hour_reset,
        seven_day_reset: lower_weekly_reset,
        evidence_at_epoch: lower_repair_source_epoch,
        received_at_min_epoch: lower_repair_source_epoch,
        received_at_max_epoch: publication_epoch,
    };
    watch.wait_for_quota_after(
        &flow.monitor_id,
        sequence,
        expected,
        Duration::from_secs(15),
    )?;
    watch.wait_for_exit(Duration::from_secs(5))?;
    let status = flow.fixture.status(&flow.monitor_id)?;
    assert_provider_quota(&status, expected)?;

    let account = flow.fixture.monitor_account(local_account_id)?;
    let weekly = &account["broker_windows"][1];
    ensure!(
        account["latest_reset_epochs"][1].as_i64() == Some(SEVEN_DAY_RESET)
            && account["statusline_reset_watermarks"][1] == "unknown"
            && account["legacy_reset_admission_after_epochs"][1].as_i64() == Some(cutoff_epoch)
            && account["reset_barriers"][1] == prior_weekly_reset_barrier
            && weekly["reset"]["value"].as_i64() == Some(lower_weekly_reset)
            && weekly["used"]["value"].as_i64() == Some(4_100)
            && weekly["used"]["reset_at_epoch"].as_i64() == Some(lower_weekly_reset)
            && weekly["paired"].is_null(),
        "provider-only lower reset did not repair display while retaining the v5 global fence"
    );
    for field in ["used", "reset"] {
        ensure!(
            weekly[field]["evidence_at_epoch"]
                .as_i64()
                .is_some_and(|observed| observed > cutoff_epoch)
                && weekly[field]["received_at_epoch"]
                    .as_i64()
                    .is_some_and(|received| received > cutoff_epoch),
            "accepted provider {field} evidence did not cross both strict legacy cutoff checks"
        );
    }
    ensure!(
        weekly["used"]["evidence_at_epoch"] == weekly["reset"]["evidence_at_epoch"]
            && weekly["used"]["received_at_epoch"] == weekly["reset"]["received_at_epoch"],
        "provider-only lower reset was not stored as one coherent evidence pair"
    );
    flow.fixture.stop_auth()?;
    Ok(())
}

#[test]
fn cli_http_429_retry_after_delta_seconds_survives_restart() -> Result<()> {
    let _test_guard = acquire_provider_flow_test_lock();
    run_retry_after_restart_case(RetryAfterCase {
        label: "delta-seconds",
        header: "600",
        deadline_epoch: INITIAL_WALL_EPOCH + 1_202,
    })
}

#[test]
fn cli_http_429_retry_after_date_survives_restart() -> Result<()> {
    let _test_guard = acquire_provider_flow_test_lock();
    run_retry_after_restart_case(RetryAfterCase {
        label: "http-date",
        header: "Tue, 14 Nov 2023 22:38:22 GMT",
        deadline_epoch: INITIAL_WALL_EPOCH + 1_502,
    })
}

struct Fixture {
    root: TempDir,
    home: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    fixture_config: PathBuf,
    credential_payload: PathBuf,
    clock_socket: PathBuf,
    clock_checkpoint: Mutex<ClockCheckpoint>,
    initial_wall_epoch: i64,
    http: FakeHttpServer,
    auth: Option<AuthProcess>,
}

#[derive(Clone, Copy)]
struct ClockCheckpoint {
    wall_epoch: i64,
    monotonic_millis: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CoordinatorTiming {
    generation: u64,
    phase: String,
    terminal_error_kind: Option<String>,
    has_terminal_result: bool,
    last_good_fetched_at_epoch: Option<i64>,
    last_good_session_used_label: Option<String>,
    last_good_weekly_used_label: Option<String>,
    provider_invoked_at_epoch: Option<i64>,
    completed_at_epoch: Option<i64>,
    success_deadline_epoch: Option<i64>,
    retry_deadline_epoch: Option<i64>,
    rate_limit_deadline_epoch: Option<i64>,
}

#[derive(Clone, Copy)]
struct CoordinatorCompletionExpectation<'a> {
    generation: u64,
    provider_invoked_at_epoch: i64,
    fetched_at_epoch: i64,
    session_used_label: &'a str,
    weekly_used_label: &'a str,
    timeout: Duration,
}

#[derive(Debug, Clone, Copy)]
struct QuotaExpectation {
    five_hour_used: i64,
    seven_day_used: i64,
    five_hour_reset: i64,
    seven_day_reset: i64,
    evidence_at_epoch: i64,
    received_at_min_epoch: i64,
    received_at_max_epoch: i64,
}

impl QuotaExpectation {
    fn matches(self, status: &Value) -> bool {
        status["five_hour"]["used_percentage_basis_points"].as_i64() == Some(self.five_hour_used)
            && status["seven_day"]["used_percentage_basis_points"].as_i64()
                == Some(self.seven_day_used)
            && self.window_matches(
                status,
                "five_hour",
                "five_hour",
                self.five_hour_used,
                self.five_hour_reset,
            )
            && self.window_matches(
                status,
                "seven_day",
                "seven_day",
                self.seven_day_used,
                self.seven_day_reset,
            )
    }

    fn window_matches(
        self,
        status: &Value,
        field: &str,
        name: &str,
        used: i64,
        reset: i64,
    ) -> bool {
        let window = &status[field];
        if window["reset_at_epoch"].as_i64() != Some(reset)
            || window["used_evidence"]["evidence_at_epoch"].as_i64() != Some(self.evidence_at_epoch)
            || !self.receipt_matches(&window["used_evidence"]["evidence_received_at_epoch"])
            || window["reset_evidence"]["evidence_at_epoch"].as_i64()
                != Some(self.evidence_at_epoch)
            || !self.receipt_matches(&window["reset_evidence"]["evidence_received_at_epoch"])
        {
            return false;
        }
        let Some(used_sequence) = window["used_evidence"]["evidence_sequence"].as_u64() else {
            return false;
        };
        let Some(reset_sequence) = window["reset_evidence"]["evidence_sequence"].as_u64() else {
            return false;
        };
        status["evidence"].as_array().is_some_and(|evidence| {
            evidence.iter().any(|entry| {
                entry["sequence"].as_u64() == Some(used_sequence)
                    && entry["source"] == "broker_projection"
                    && entry["evidence_at_epoch"].as_i64() == Some(self.evidence_at_epoch)
                    && self.receipt_matches(&entry["evidence_received_at_epoch"])
                    && entry["value"]["kind"] == "quota_used_percentage"
                    && entry["value"]["window"] == name
                    && entry["value"]["used_percentage_basis_points"].as_i64() == Some(used)
            }) && evidence.iter().any(|entry| {
                entry["sequence"].as_u64() == Some(reset_sequence)
                    && entry["source"] == "broker_projection"
                    && entry["evidence_at_epoch"].as_i64() == Some(self.evidence_at_epoch)
                    && self.receipt_matches(&entry["evidence_received_at_epoch"])
                    && entry["value"]["kind"] == "quota_reset"
                    && entry["value"]["window"] == name
                    && entry["value"]["reset_at_epoch"].as_i64() == Some(reset)
            })
        })
    }

    fn receipt_matches(self, value: &Value) -> bool {
        value.as_i64().is_some_and(|received| {
            (self.received_at_min_epoch..=self.received_at_max_epoch).contains(&received)
        })
    }
}

impl CoordinatorTiming {
    fn diagnostic_json(&self) -> Value {
        json!({
            "generation": self.generation,
            "phase": self.phase,
            "terminal_error_kind": self.terminal_error_kind,
            "has_terminal_result": self.has_terminal_result,
            "last_good_fetched_at_epoch": self.last_good_fetched_at_epoch,
            "last_good_session_used_label": self.last_good_session_used_label,
            "last_good_weekly_used_label": self.last_good_weekly_used_label,
            "provider_invoked_at_epoch": self.provider_invoked_at_epoch,
            "completed_at_epoch": self.completed_at_epoch,
            "success_deadline_epoch": self.success_deadline_epoch,
            "retry_deadline_epoch": self.retry_deadline_epoch,
            "rate_limit_deadline_epoch": self.rate_limit_deadline_epoch,
        })
    }
}

fn assert_successful_coordinator_timing(timing: &CoordinatorTiming) -> Result<()> {
    let invoked_at = timing
        .provider_invoked_at_epoch
        .context("successful coordinator state omitted provider invocation time")?;
    let completed_at = timing
        .completed_at_epoch
        .context("successful coordinator state omitted completion time")?;
    let success_deadline = timing
        .success_deadline_epoch
        .context("successful coordinator state omitted its cooldown deadline")?;
    let invocation_floor = invoked_at
        .checked_add(300)
        .context("provider invocation cooldown overflowed")?;
    let completion_floor = completed_at
        .checked_add(300)
        .context("provider completion cooldown overflowed")?;
    ensure!(
        timing.generation > 0
            && timing.phase == "completed"
            && timing.has_terminal_result
            && timing.last_good_fetched_at_epoch.is_some()
            && timing.last_good_session_used_label.is_some()
            && timing.last_good_weekly_used_label.is_some()
            && completed_at >= invoked_at
            && success_deadline >= invocation_floor.max(completion_floor)
            && timing.retry_deadline_epoch.is_none()
            && timing.rate_limit_deadline_epoch.is_none(),
        "coordinator state did not preserve a successful provider result and cooldown"
    );
    Ok(())
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir_in("/tmp").context("create private flow-test directory")?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))?;
        let home = root.path().join("home");
        let config_dir = root.path().join("config");
        let data_dir = root.path().join("usage-data");
        fs::create_dir_all(&home)?;
        fs::create_dir_all(&config_dir)?;
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .context("bind fake Claude endpoint to loopback")?;
        let http = FakeHttpServer::from_listener(listener, FIRST_TOKEN.to_owned())?;
        let fixture_config = root.path().join("private-fixture-config.json");
        let credential_payload = root.path().join("private-credential-payload.json");
        let clock_socket = root.path().join("fixture-clock.sock");
        let fixture = Self {
            root,
            home,
            config_dir,
            data_dir,
            fixture_config,
            credential_payload,
            clock_socket,
            clock_checkpoint: Mutex::new(ClockCheckpoint {
                wall_epoch: INITIAL_WALL_EPOCH,
                monotonic_millis: INITIAL_MONOTONIC_MILLIS,
            }),
            initial_wall_epoch: INITIAL_WALL_EPOCH,
            http,
            auth: None,
        };
        fixture.write_fixture_config(INITIAL_WALL_EPOCH, INITIAL_MONOTONIC_MILLIS)?;
        Ok(fixture)
    }

    fn write_endpoint_config(
        &self,
        endpoint: &str,
        wall_epoch: i64,
        monotonic_millis: u64,
    ) -> Result<()> {
        write_private_json(
            &self.fixture_config,
            &json!({
                "credential_payload_path": self.credential_payload,
                "claude_usage_endpoint": endpoint,
                "clock_control_socket": self.clock_socket,
                "keychain_service": KEYCHAIN_SERVICE,
                "initial_wall_epoch": wall_epoch,
                "initial_monotonic_millis": monotonic_millis,
            }),
        )?;
        *self
            .clock_checkpoint
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = ClockCheckpoint {
            wall_epoch,
            monotonic_millis,
        };
        Ok(())
    }

    fn write_fixture_config(&self, wall_epoch: i64, monotonic_millis: u64) -> Result<()> {
        let endpoint = self.http.endpoint();
        self.write_endpoint_config(&endpoint, wall_epoch, monotonic_millis)
    }

    fn write_credential(&self, token: &str) -> Result<()> {
        write_private_json(
            &self.credential_payload,
            &json!({
                "claudeAiOauth": {
                    "accessToken": token,
                    "subscriptionType": "max"
                },
                "oauthAccount": {
                    "emailAddress": "provider-flow@example.test",
                    "organizationType": "claude_max"
                }
            }),
        )
    }

    fn seed_stale_v8_lease(&self) -> Result<u32> {
        let broker_dir = self.data_dir.join("usage-broker");
        let run_dir = broker_dir.join("run");
        fs::create_dir_all(&run_dir)?;
        fs::set_permissions(&broker_dir, fs::Permissions::from_mode(0o700))?;
        fs::set_permissions(&run_dir, fs::Permissions::from_mode(0o700))?;

        let socket_path = run_dir.join("usage-broker.sock");
        ensure!(
            !socket_path.exists(),
            "stale-v8 fixture expected an unowned broker socket path"
        );

        let stale_pid = exited_child_pid()?;
        let wall_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock predates the Unix epoch")?
            .as_secs();
        let wall_epoch = i64::try_from(wall_epoch).context("wall epoch exceeds i64")?;
        let renewed_at_epoch = wall_epoch
            .checked_sub(BROKER_LEASE_STALE_AFTER_SECONDS + 1)
            .context("stale lease epoch underflowed")?;
        let lease_path = run_dir.join("leader.pid");
        let instance_id = stale_v8_instance_id();
        write_private_json(
            &lease_path,
            &json!({
                "instance_id": instance_id,
                "process_id": stale_pid,
                "protocol_version": "v8",
                "build_id": "synthetic-old-build",
                "renewed_at_epoch": renewed_at_epoch,
            }),
        )?;
        Ok(stale_pid)
    }

    fn assert_v9_lease_replaced(&self, stale_pid: u32) -> Result<()> {
        let lease_path = self
            .data_dir
            .join("usage-broker")
            .join("run")
            .join("leader.pid");
        let bytes = fs::read(&lease_path).context("foreground broker did not write a lease")?;
        let lease: Value =
            serde_json::from_slice(&bytes).context("foreground broker lease was not valid JSON")?;
        let live_pid = lease["process_id"]
            .as_u64()
            .context("foreground broker lease omitted its process ID")?;
        let live_instance_id = lease["instance_id"]
            .as_str()
            .context("foreground broker lease omitted its instance ID")?;
        ensure!(
            lease["protocol_version"] == "v9"
                && is_sha256_instance_id(live_instance_id)
                && live_instance_id != stale_v8_instance_id()
                && live_pid != u64::from(stale_pid),
            "foreground auth prepare did not replace the stale v8 lease"
        );
        Ok(())
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jackin"));
        self.apply_environment(&mut command);
        command
            .arg("usage")
            .arg("--data-dir")
            .arg(&self.data_dir)
            .arg("--format")
            .arg("json");
        command
    }

    fn run_json(&self, args: &[&str]) -> Result<Value> {
        let mut command = self.command();
        command.args(args);
        let output = output_with_timeout(command, COMMAND_TIMEOUT, "run isolated usage CLI")?;
        ensure!(
            output.status.success(),
            "usage CLI command failed with exit status {:?}",
            output.status.code()
        );
        json_from_output(&output.stdout).context("usage CLI returned no JSON reply")
    }

    fn run_json_with_stdin(&self, args: &[&str], stdin_bytes: &[u8]) -> Result<Value> {
        let mut command = self.command();
        command.args(args);
        let output = output_with_stdin_timeout(
            command,
            stdin_bytes,
            COMMAND_TIMEOUT,
            "run isolated usage CLI with statusline input",
        )?;
        let cli_error = json_from_output(&output.stdout).map(|reply| {
            json!({
                "code": reply["error"]["code"],
                "message": reply["error"]["message"],
            })
        });
        ensure!(
            output.status.success(),
            "usage CLI statusline command failed with exit status {:?}; error: {:?}; stderr: {}",
            output.status.code(),
            cli_error,
            redact_diagnostic_stderr(&output.stderr)
        );
        json_from_output(&output.stdout).context("usage CLI returned no JSON reply")
    }

    fn diagnostic_json(&self, args: &[&str]) -> Value {
        let mut command = self.command();
        command.args(args);
        let Ok(output) =
            output_with_timeout(command, COMMAND_TIMEOUT, "collect watch-failure diagnostic")
        else {
            return json!({ "diagnostic_unavailable": true });
        };
        let parsed_reply = json_from_output(&output.stdout);
        if let Some(reply) = parsed_reply.as_ref() {
            let expected_observe_only_status = args.first() == Some(&"status")
                && reply["result"] == "status"
                && output.status.code() == Some(2);
            if output.status.success() || expected_observe_only_status {
                return reply.clone();
            }
        }
        json!({
            "exit_code": output.status.code(),
            "json_error_code": parsed_reply.as_ref().and_then(|reply| reply["error"]["code"].as_str()),
            "json_error_message": parsed_reply.as_ref().and_then(safe_json_error_message),
            "stderr": redact_diagnostic_stderr(&output.stderr),
        })
    }

    fn auth_prepare_json_diagnostic(&self) -> Value {
        const MAX_TRANSCRIPT_BYTES: u64 = 16 * 1024;
        let path = self.root.path().join("auth-prepare-transcript.txt");
        let Ok(file) = fs::File::open(path) else {
            return json!({ "available": false });
        };
        let Ok(metadata) = file.metadata() else {
            return json!({ "available": false });
        };
        if !metadata.is_file() || metadata.len() > MAX_TRANSCRIPT_BYTES {
            return json!({ "available": false, "bounded_regular_file": false });
        }
        let mut bytes = Vec::new();
        if file
            .take(MAX_TRANSCRIPT_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_TRANSCRIPT_BYTES
        {
            return json!({ "available": false, "bounded_regular_file": false });
        }
        let Some(reply) = json_from_output(&bytes) else {
            return json!({ "available": true, "json_reply": false });
        };
        json!({
            "available": true,
            "json_reply": true,
            "result": reply["result"].as_str(),
            "error_code": reply["error"]["code"].as_str(),
            "error_message": safe_json_error_message(&reply),
        })
    }

    fn run_pty_json(&self, args: &[&str]) -> Result<Value> {
        let sequence = NEXT_SCRIPT_FILE.fetch_add(1, Ordering::Relaxed);
        let transcript = self
            .root
            .path()
            .join(format!("pty-transcript-{sequence}.txt"));
        let invocation = std::iter::once(env!("CARGO_BIN_EXE_jackin"))
            .chain(self.cli_args().iter().map(String::as_str))
            .chain(args.iter().copied())
            .map(shell_quote)
            .collect::<Vec<_>>()
            .join(" ");
        let full = format!("stty cols 120 rows 40 >/dev/null 2>&1; exec {invocation}");
        let mut command = Command::new("script");
        if cfg!(target_os = "macos") {
            command
                .arg("-q")
                .arg(&transcript)
                .arg("sh")
                .arg("-lc")
                .arg(&full);
        } else {
            command.args(["-q", "-e", "-c", &full]).arg("/dev/null");
        }
        self.apply_environment(&mut command);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = output_with_timeout(command, COMMAND_TIMEOUT, "run terminal CLI command")?;
        ensure!(
            output.status.success(),
            "terminal CLI command failed with exit status {:?}",
            output.status.code()
        );
        let bytes = if cfg!(target_os = "macos") {
            fs::read(&transcript).context("read terminal transcript")?
        } else {
            output.stdout
        };
        json_from_output(&bytes).context("terminal CLI returned no JSON reply")
    }

    fn cli_args(&self) -> Vec<String> {
        vec![
            "usage".to_owned(),
            "--data-dir".to_owned(),
            self.data_dir.to_string_lossy().into_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ]
    }

    fn apply_environment(&self, command: &mut Command) {
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("JACKIN_HOME_DIR", &self.home)
            .env("JACKIN_CONFIG_DIR", &self.config_dir)
            .env(
                "JACKIN_USAGE_BROKER_BIN",
                env!("CARGO_BIN_EXE_jackin-usage-broker-fixture"),
            )
            .env("JACKIN_USAGE_TEST_FIXTURE_CONFIG", &self.fixture_config)
            .env(
                "PATH",
                std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
            )
            .env("TERM", "xterm-256color")
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("HTTP_PROXY", "http://127.0.0.1:9")
            .env("HTTPS_PROXY", "http://127.0.0.1:9")
            .env("ALL_PROXY", "http://127.0.0.1:9")
            .env("http_proxy", "http://127.0.0.1:9")
            .env("https_proxy", "http://127.0.0.1:9")
            .env("all_proxy", "http://127.0.0.1:9")
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .current_dir(&self.home);
    }

    fn start_auth(&mut self) -> Result<()> {
        ensure!(
            self.auth.is_none(),
            "foreground auth service is already running"
        );
        let mut command = Command::new("script");
        let args = [
            env!("CARGO_BIN_EXE_jackin").to_owned(),
            "usage".to_owned(),
            "--data-dir".to_owned(),
            self.data_dir.to_string_lossy().into_owned(),
            "auth".to_owned(),
            "prepare".to_owned(),
            "--provider".to_owned(),
            "claude".to_owned(),
            "--keychain-service".to_owned(),
            KEYCHAIN_SERVICE.to_owned(),
        ];
        let invocation = args
            .iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ");
        let full = format!("stty cols 120 rows 40 >/dev/null 2>&1; exec {invocation}");
        let transcript = self.root.path().join("auth-prepare-transcript.txt");
        if cfg!(target_os = "macos") {
            command
                .arg("-q")
                .arg(transcript)
                .arg("sh")
                .arg("-lc")
                .arg(full);
        } else {
            command.args(["-q", "-e", "-c", &full]).arg("/dev/null");
        }
        self.apply_environment(&mut command);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command
            .spawn()
            .context("spawn auth prepare inside a real PTY")?;
        self.auth = Some(AuthProcess { child });
        Ok(())
    }

    fn wait_service_ready(&self) -> Result<Value> {
        wait_until(Duration::from_secs(15), || {
            let Ok(reply) = self.run_json(&["service", "status"]) else {
                return None;
            };
            if reply["result"] == "service_status"
                && reply["status"]["running"] == true
                && reply["status"]["experimental_collector_source"].is_string()
            {
                Some(reply)
            } else {
                None
            }
        })
        .context("foreground auth service did not become ready")
    }

    fn status(&self, monitor_id: &str) -> Result<Value> {
        let mut command = self.command();
        command.args(["status", "--monitor", monitor_id]);
        let output = output_with_timeout(command, COMMAND_TIMEOUT, "read isolated monitor status")?;
        ensure!(
            matches!(output.status.code(), Some(0 | 2)),
            "monitor status command exited with code {:?}",
            output.status.code()
        );
        let reply =
            json_from_output(&output.stdout).context("status CLI returned no JSON reply")?;
        ensure!(
            reply["result"] == "status",
            "status command returned an unexpected reply"
        );
        Ok(reply["status"].clone())
    }

    fn coordinator_timing(&self, source_id: &str) -> Result<CoordinatorTiming> {
        let mut components = Path::new(source_id).components();
        ensure!(
            matches!(components.next(), Some(std::path::Component::Normal(_)))
                && components.next().is_none(),
            "provider source ID is not a safe account-state path component"
        );
        let path = self
            .data_dir
            .join("usage-broker")
            .join("accounts")
            .join(format!("claude-{source_id}.json"));
        let metadata = fs::metadata(&path).context("read provider coordinator-state metadata")?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_COORDINATOR_STATE_BYTES,
            "provider coordinator-state file exceeded its bounded regular-file contract"
        );
        let mut bytes = Vec::new();
        fs::File::open(&path)?
            .take(MAX_COORDINATOR_STATE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_COORDINATOR_STATE_BYTES,
            "provider coordinator-state file grew beyond its read bound"
        );
        let envelope: Value =
            serde_json::from_slice(&bytes).context("decode provider coordinator state")?;
        ensure!(
            envelope["capability"]["surface_id"] == "claude"
                && envelope["capability"]["account_id"].as_str() == Some(source_id),
            "provider coordinator state did not match the selected source"
        );
        let optional_epoch = |field: &str| -> Result<Option<i64>> {
            if envelope[field].is_null() {
                Ok(None)
            } else {
                envelope[field]
                    .as_i64()
                    .map(Some)
                    .with_context(|| format!("coordinator state field {field} was not an epoch"))
            }
        };
        let last_good = &envelope["last_good"];
        let used_label_for_slot = |slot: &str| {
            last_good["buckets"]
                .as_array()?
                .iter()
                .find(|bucket| bucket["status_slot"] == slot)?["used_label"]
                .as_str()
                .map(str::to_owned)
        };
        Ok(CoordinatorTiming {
            generation: envelope["generation"]
                .as_u64()
                .context("coordinator state omitted generation")?,
            phase: envelope["phase"]
                .as_str()
                .context("coordinator state omitted phase")?
                .to_owned(),
            terminal_error_kind: envelope["terminal_error"]["kind"]
                .as_str()
                .map(str::to_owned),
            has_terminal_result: envelope["terminal_result"].is_object(),
            last_good_fetched_at_epoch: last_good["fetched_at_epoch"].as_i64(),
            last_good_session_used_label: used_label_for_slot("session"),
            last_good_weekly_used_label: used_label_for_slot("weekly"),
            provider_invoked_at_epoch: optional_epoch("provider_invoked_at_epoch")?,
            completed_at_epoch: optional_epoch("completed_at_epoch")?,
            success_deadline_epoch: optional_epoch("success_deadline_epoch")?,
            retry_deadline_epoch: optional_epoch("retry_deadline_epoch")?,
            rate_limit_deadline_epoch: optional_epoch("rate_limit_deadline_epoch")?,
        })
    }

    fn monitor_state_path(&self) -> PathBuf {
        self.data_dir
            .join("usage-broker")
            .join("monitor")
            .join("state.json")
    }

    fn monitor_state(&self) -> Result<Value> {
        let path = self.monitor_state_path();
        let file = fs::File::open(&path).context("open read-only monitor state snapshot")?;
        let metadata = file.metadata().context("read monitor state metadata")?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_MONITOR_STATE_BYTES,
            "monitor state snapshot exceeded its bounded regular-file contract"
        );
        let mut bytes = Vec::new();
        file.take(MAX_MONITOR_STATE_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("read bounded monitor state snapshot")?;
        ensure!(
            bytes.len() as u64 <= MAX_MONITOR_STATE_BYTES,
            "monitor state snapshot grew beyond its read bound"
        );
        serde_json::from_slice(&bytes).context("decode read-only monitor state snapshot")
    }

    fn preserve_monitor_state_diagnostic(&self) -> Result<PathBuf> {
        let source_path = self.monitor_state_path();
        let source_metadata = fs::symlink_metadata(&source_path)
            .context("inspect synthetic monitor state for failure diagnosis")?;
        ensure!(
            source_metadata.is_file() && source_metadata.len() <= MAX_MONITOR_STATE_BYTES,
            "synthetic monitor state exceeded its bounded regular-file contract"
        );
        let source = fs::File::open(&source_path)
            .context("open synthetic monitor state for failure diagnosis")?;
        let mut bytes = Vec::new();
        source
            .take(MAX_MONITOR_STATE_BYTES + 1)
            .read_to_end(&mut bytes)
            .context("read synthetic monitor state for failure diagnosis")?;
        ensure!(
            bytes.len() as u64 <= MAX_MONITOR_STATE_BYTES,
            "synthetic monitor state exceeded its bounded read contract"
        );

        let private_dir = tempfile::Builder::new()
            .prefix("jackin-v5-monitor-state-")
            .tempdir()
            .context("create private directory for synthetic state diagnosis")?;
        let private_state_path = private_dir.path().join("state.json");
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&private_state_path)
            .context("create private synthetic state diagnosis copy")?;
        destination
            .write_all(&bytes)
            .context("copy synthetic monitor state for diagnosis")?;
        destination
            .sync_all()
            .context("sync synthetic monitor state diagnosis copy")?;
        fs::set_permissions(&private_state_path, fs::Permissions::from_mode(0o600))
            .context("set private synthetic state diagnosis permissions")?;
        Ok(private_dir.keep().join("state.json"))
    }

    fn monitor_account(&self, account_id: &str) -> Result<Value> {
        self.monitor_state()?["accounts"][account_id]
            .as_object()
            .cloned()
            .map(Value::Object)
            .context("monitor state omitted the selected account")
    }

    fn rewrite_monitor_snapshot_as_v5(&self) -> Result<(Value, i64)> {
        let mut state = self.monitor_state()?;
        ensure!(
            state["schema_version"] == 6,
            "legacy fixture conversion requires a current v6 monitor snapshot"
        );
        let cutoff = state["last_now_epoch"]
            .as_i64()
            .context("current monitor snapshot omitted its persisted clock epoch")?;
        let accounts = state["accounts"]
            .as_object_mut()
            .context("current monitor snapshot omitted account records")?;
        for account in accounts.values_mut() {
            let account = account
                .as_object_mut()
                .context("current account snapshot was not an object")?;
            ensure!(
                account.remove("statusline_reset_watermarks").is_some()
                    && account
                        .remove("legacy_reset_admission_after_epochs")
                        .is_some(),
                "current account omitted a v6 reset metadata field"
            );
        }
        let monitors = state["monitors"]
            .as_object_mut()
            .context("current monitor snapshot omitted monitor records")?;
        for monitor in monitors.values_mut() {
            let events = monitor["events"]
                .as_array_mut()
                .context("current monitor record omitted its event history")?;
            for event in events {
                ensure!(
                    event["status"]["schema_version"] == 6,
                    "current monitor event had a non-v6 status schema"
                );
                event["status"]["schema_version"] = json!(5);
            }
        }
        state["schema_version"] = json!(5);
        let encoded = serde_json::to_vec(&state).context("encode synthetic v5 monitor snapshot")?;
        fs::write(self.monitor_state_path(), encoded)
            .context("write synthetic v5 monitor snapshot fixture")?;
        Ok((state, cutoff))
    }

    fn wait_for_coordinator_attempt(
        &self,
        source_id: &str,
        evidence_at_epoch: i64,
        timeout: Duration,
    ) -> Result<CoordinatorTiming> {
        wait_until(timeout, || {
            self.coordinator_timing(source_id).ok().filter(|timing| {
                timing.phase == "updating"
                    && timing.provider_invoked_at_epoch == Some(evidence_at_epoch)
            })
        })
        .context("provider request did not reach its durable updating generation")
    }

    fn wait_for_coordinator_completion(
        &self,
        source_id: &str,
        expected: CoordinatorCompletionExpectation<'_>,
    ) -> Result<CoordinatorTiming> {
        if let Some(timing) = wait_until(expected.timeout, || {
            self.coordinator_timing(source_id).ok().filter(|timing| {
                timing.generation == expected.generation
                    && timing.phase == "completed"
                    && timing.has_terminal_result
                    && timing.provider_invoked_at_epoch == Some(expected.provider_invoked_at_epoch)
                    && timing.last_good_fetched_at_epoch == Some(expected.fetched_at_epoch)
                    && timing.last_good_session_used_label.as_deref()
                        == Some(expected.session_used_label)
                    && timing.last_good_weekly_used_label.as_deref()
                        == Some(expected.weekly_used_label)
            })
        }) {
            return Ok(timing);
        }
        let coordinator = self.coordinator_timing(source_id).map_or_else(
            |_| json!({ "unavailable": true }),
            |timing| timing.diagnostic_json(),
        );
        bail!(
            "provider result did not reach its expected durable completion before the bound; sanitized coordinator state: {coordinator}; fake HTTP counters: {}",
            self.http.diagnostic_json()
        )
    }

    fn wait_for_coordinator_terminal(
        &self,
        source_id: &str,
        expected_generation: u64,
        provider_invoked_at_epoch: i64,
        timeout: Duration,
    ) -> Result<CoordinatorTiming> {
        wait_until(timeout, || {
            self.coordinator_timing(source_id).ok().filter(|timing| {
                let terminal = match timing.phase.as_str() {
                    "completed" => timing.has_terminal_result,
                    "failed" => timing.terminal_error_kind.is_some(),
                    _ => false,
                };
                timing.generation == expected_generation
                    && timing.provider_invoked_at_epoch == Some(provider_invoked_at_epoch)
                    && timing.completed_at_epoch.is_some()
                    && terminal
            })
        })
        .context("provider attempt did not reach a durable terminal generation before the bound")
    }

    fn start_watch(&self, monitor_id: &str) -> Result<WatchProcess> {
        let mut command = self.command();
        command
            .args(["watch", "--monitor", monitor_id, "--timeout-secs", "3"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().context("spawn real CLI monitor watch")?;
        let stdout = child
            .stdout
            .take()
            .context("watch stdout was not captured")?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(WatchProcess { child, receiver })
    }

    fn set_clock(&self, wall_epoch: i64, monotonic_millis: u64) -> Result<usize> {
        let previous = *self
            .clock_checkpoint
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut socket = UnixStream::connect(&self.clock_socket)
            .context("connect to private fixture clock control socket")?;
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        socket.set_write_timeout(Some(Duration::from_secs(3)))?;
        serde_json::to_writer(
            &mut socket,
            &json!({
                "op": "set_clock",
                "wall_epoch": wall_epoch,
                "monotonic_millis": monotonic_millis,
            }),
        )?;
        socket.write_all(b"\n")?;
        socket.flush()?;
        let mut reply = String::new();
        BufReader::new(socket).read_line(&mut reply)?;
        let reply: Value = serde_json::from_str(&reply).context("decode fixture clock reply")?;
        ensure!(
            reply["ok"] == true,
            "fixture clock rejected request wall_epoch={wall_epoch}, monotonic_millis={monotonic_millis}; last configured or acknowledged sample was wall_epoch={}, monotonic_millis={}",
            previous.wall_epoch,
            previous.monotonic_millis
        );
        *self
            .clock_checkpoint
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = ClockCheckpoint {
            wall_epoch,
            monotonic_millis,
        };
        reply["credential_reads"]
            .as_u64()
            .and_then(|reads| usize::try_from(reads).ok())
            .context("fixture clock reply omitted its credential-read count")
    }

    fn wait_ticker_ready(&self) -> Result<usize> {
        let mut socket = UnixStream::connect(&self.clock_socket)
            .context("connect to private fixture ticker control socket")?;
        socket.set_read_timeout(Some(Duration::from_secs(7)))?;
        socket.set_write_timeout(Some(Duration::from_secs(7)))?;
        serde_json::to_writer(
            &mut socket,
            &json!({
                "op": "wait_ticker_ready",
                "timeout_millis": 5_000,
            }),
        )?;
        socket.write_all(b"\n")?;
        socket.flush()?;
        let mut reply = String::new();
        BufReader::new(socket).read_line(&mut reply)?;
        let reply: Value =
            serde_json::from_str(&reply).context("decode fixture ticker-ready control reply")?;
        ensure!(
            reply["ok"] == true && reply["ticker_first_step_completed"] == true,
            "fixture ticker did not complete its first step before the control deadline"
        );
        reply["credential_reads"]
            .as_u64()
            .and_then(|reads| usize::try_from(reads).ok())
            .context("fixture ticker reply omitted its credential-read count")
    }

    fn stop_auth(&mut self) -> Result<()> {
        if self.auth.is_none() {
            return Ok(());
        }
        let reply = self.run_json(&["service", "stop"])?;
        ensure!(
            reply["result"] == "service_stopped",
            "service stop returned an unexpected reply"
        );
        let Some(mut auth) = self.auth.take() else {
            return Ok(());
        };
        auth.wait_for_exit(Duration::from_secs(10))
            .context("auth prepare PTY did not exit after broker stop")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.auth.is_some() {
            let _stop_result = self.run_json(&["service", "stop"]);
            if let Some(mut auth) = self.auth.take() {
                let _wait_result = auth.wait_for_exit(Duration::from_secs(3));
            }
        }
    }
}

#[derive(Clone, Copy)]
struct RetryAfterCase {
    label: &'static str,
    header: &'static str,
    deadline_epoch: i64,
}

struct ProviderFlow {
    fixture: Fixture,
    source_id: String,
    monitor_id: String,
    credential_reads: usize,
}

struct SeededQuota {
    expectation: QuotaExpectation,
    status: Value,
}

struct RateLimitedQuota {
    timing: CoordinatorTiming,
    status: Value,
}

impl ProviderFlow {
    fn new(label: &str) -> Result<Self> {
        let mut fixture = Fixture::new()?;
        fixture.write_credential(FIRST_TOKEN)?;
        fixture.start_auth()?;
        let service = fixture.wait_service_ready()?;
        let credential_reads = fixture.wait_ticker_ready()?;
        ensure!(
            credential_reads == 1,
            "fake keychain reader was not used once before the initial ticker step"
        );
        let source_id = service["status"]["experimental_collector_source"]
            .as_str()
            .context("foreground service omitted its selected source ID")?
            .to_owned();
        let binding_reply = fixture.run_pty_json(&[
            "binding",
            "confirm",
            "--provider",
            "claude",
            "--provider-account",
            &source_id,
            "--account",
            &format!("local-claude-account-{label}"),
            "--approve-experimental-collector",
            "--operator-label",
            "synthetic provider rate-limit test",
            "--confirm",
        ])?;
        ensure!(
            binding_reply["result"] == "account_bound",
            "binding confirmation did not return account_bound"
        );
        let binding = &binding_reply["binding"];
        let binding_id = binding["binding_id"]
            .as_str()
            .context("binding response omitted its ID")?
            .to_owned();
        let binding_revision = binding["revision"]
            .as_u64()
            .context("binding response omitted its revision")?;
        let observer_reply = fixture.run_json(&[
            "monitor",
            "observe",
            "--provider",
            "claude",
            "--binding",
            &binding_id,
            "--binding-revision",
            &binding_revision.to_string(),
            "--idempotency-key",
            &format!("synthetic-provider-rate-limit-{label}-v1"),
            "--experimental-collector",
        ])?;
        ensure!(
            observer_reply["result"] == "started",
            "provider observer did not start"
        );
        let monitor_id = observer_reply["status"]["monitor_id"]
            .as_str()
            .context("observer response omitted its monitor ID")?
            .to_owned();
        Ok(Self {
            fixture,
            source_id,
            monitor_id,
            credential_reads,
        })
    }

    fn set_clock(&self, wall_epoch: i64) -> Result<()> {
        let monotonic_millis = monotonic_millis_at(wall_epoch)?;
        ensure!(
            self.fixture.set_clock(wall_epoch, monotonic_millis)? == 1,
            "fake keychain reader was not used exactly once"
        );
        Ok(())
    }

    fn restart_at(&mut self, wall_epoch: i64) -> Result<()> {
        self.fixture.stop_auth()?;
        self.fixture
            .write_fixture_config(wall_epoch, monotonic_millis_at(wall_epoch)?)?;
        self.fixture.start_auth()?;
        let service = self.fixture.wait_service_ready()?;
        ensure!(
            service["status"]["experimental_collector_source"].as_str()
                == Some(self.source_id.as_str()),
            "broker restart changed the selected provider source"
        );
        self.credential_reads = self.fixture.wait_ticker_ready()?;
        ensure!(
            self.credential_reads == 1,
            "fake keychain reader was not used once before the restarted ticker step"
        );
        Ok(())
    }
}

fn monotonic_millis_at(wall_epoch: i64) -> Result<u64> {
    let elapsed_seconds = u64::try_from(
        wall_epoch
            .checked_sub(INITIAL_WALL_EPOCH)
            .context("fake wall clock moved before its initial sample")?,
    )
    .context("fake clock interval did not fit monotonic seconds")?;
    INITIAL_MONOTONIC_MILLIS
        .checked_add(
            elapsed_seconds
                .checked_mul(1_000)
                .context("fake clock interval overflowed monotonic milliseconds")?,
        )
        .context("fake monotonic clock overflowed")
}

fn run_retry_after_restart_case(case: RetryAfterCase) -> Result<()> {
    let mut flow = ProviderFlow::new(case.label)?;
    let seeded = seed_provider_quota(&flow)?;
    let limited = publish_rate_limit_after_restart(&mut flow, case, &seeded)?;
    restart_before_retry_deadline(&mut flow, case, &seeded, &limited)?;
    complete_natural_retry(&mut flow, case)?;
    flow.fixture.stop_auth()?;
    Ok(())
}

fn seed_provider_quota(flow: &ProviderFlow) -> Result<SeededQuota> {
    let request_epoch = INITIAL_WALL_EPOCH + 1;
    flow.set_clock(request_epoch)?;
    let request = next_provider_request(
        &flow.fixture,
        &flow.monitor_id,
        &flow.source_id,
        "rate-limit seed",
        flow.credential_reads,
    )?;
    assert_provider_request(&request)?;
    let timing = flow.fixture.wait_for_coordinator_attempt(
        &flow.source_id,
        request_epoch,
        Duration::from_secs(5),
    )?;
    let mut watch = flow.fixture.start_watch(&flow.monitor_id)?;
    let sequence = watch.wait_for_event(&flow.monitor_id, Duration::from_secs(10))?;
    let five_hour_reset = INITIAL_WALL_EPOCH + 6_400;
    let weekly_reset = INITIAL_WALL_EPOCH + 604_800;
    let five_hour_reset_at = rfc3339_utc_from_epoch(five_hour_reset);
    let weekly_reset_at = rfc3339_utc_from_epoch(weekly_reset);
    request.respond.send(HttpResponse::ok(
        37,
        23,
        &five_hour_reset_at,
        &weekly_reset_at,
    ))?;
    flow.fixture.wait_for_coordinator_completion(
        &flow.source_id,
        CoordinatorCompletionExpectation {
            generation: timing.generation,
            provider_invoked_at_epoch: request_epoch,
            fetched_at_epoch: request_epoch,
            session_used_label: "37% used",
            weekly_used_label: "23% used",
            timeout: Duration::from_secs(10),
        },
    )?;
    let expectation = QuotaExpectation {
        five_hour_used: 3_700,
        seven_day_used: 2_300,
        five_hour_reset,
        seven_day_reset: weekly_reset,
        evidence_at_epoch: request_epoch,
        received_at_min_epoch: request_epoch,
        received_at_max_epoch: INITIAL_WALL_EPOCH + 2,
    };
    flow.set_clock(INITIAL_WALL_EPOCH + 2)?;
    watch.wait_for_quota_after(
        &flow.monitor_id,
        sequence,
        expectation,
        Duration::from_secs(15),
    )?;
    watch.wait_for_exit(Duration::from_secs(5))?;
    let status = flow.fixture.status(&flow.monitor_id)?;
    assert_provider_quota(&status, expectation)?;
    assert_successful_coordinator_timing(&flow.fixture.coordinator_timing(&flow.source_id)?)?;
    Ok(SeededQuota {
        expectation,
        status,
    })
}

fn publish_rate_limit_after_restart(
    flow: &mut ProviderFlow,
    case: RetryAfterCase,
    seeded: &SeededQuota,
) -> Result<RateLimitedQuota> {
    let first_timing = flow.fixture.coordinator_timing(&flow.source_id)?;
    assert_successful_coordinator_timing(&first_timing)?;
    flow.set_clock(INITIAL_WALL_EPOCH + 299)?;
    assert_request_count_remains(
        &flow.fixture.http,
        1,
        Duration::from_millis(750),
        "provider polled before the first restart",
    )?;
    flow.restart_at(INITIAL_WALL_EPOCH + 301)?;
    ensure!(
        flow.fixture.coordinator_timing(&flow.source_id)? == first_timing
            && flow.fixture.http.request_count() == 1,
        "first successful provider result changed during the first restart"
    );
    flow.set_clock(INITIAL_WALL_EPOCH + 600)?;
    assert_request_count_remains(
        &flow.fixture.http,
        1,
        Duration::from_millis(750),
        "provider polled before the recovered success cooldown elapsed",
    )?;

    let request_epoch = INITIAL_WALL_EPOCH + 601;
    flow.set_clock(request_epoch)?;
    let request = next_provider_request(
        &flow.fixture,
        &flow.monitor_id,
        &flow.source_id,
        case.label,
        flow.credential_reads,
    )?;
    assert_provider_request(&request)?;
    let attempt = flow.fixture.wait_for_coordinator_attempt(
        &flow.source_id,
        request_epoch,
        Duration::from_secs(5),
    )?;
    let mut watch = flow.fixture.start_watch(&flow.monitor_id)?;
    let sequence = watch.wait_for_event(&flow.monitor_id, Duration::from_secs(10))?;
    let response_epoch = INITIAL_WALL_EPOCH + 602;
    flow.set_clock(response_epoch)?;
    request
        .respond
        .send(HttpResponse::too_many_requests(case.header))?;
    let timing = flow.fixture.wait_for_coordinator_terminal(
        &flow.source_id,
        attempt.generation,
        request_epoch,
        Duration::from_secs(10),
    )?;
    assert_rate_limited_coordinator_timing(
        &timing,
        case.deadline_epoch,
        request_epoch,
        response_epoch,
        seeded.expectation.evidence_at_epoch,
    )?;
    flow.set_clock(response_epoch + 1)?;
    let watched_status = watch.wait_for_rate_limited_after(
        &flow.monitor_id,
        sequence,
        seeded.expectation,
        case.deadline_epoch,
        Duration::from_secs(15),
    )?;
    assert_provider_rate_limited_quota(
        &watched_status,
        &seeded.status,
        seeded.expectation,
        case.deadline_epoch,
    )?;
    watch.wait_for_exit(Duration::from_secs(5))?;
    let status = flow.fixture.status(&flow.monitor_id)?;
    assert_provider_rate_limited_quota(
        &status,
        &seeded.status,
        seeded.expectation,
        case.deadline_epoch,
    )?;
    ensure!(
        flow.fixture.coordinator_timing(&flow.source_id)? == timing
            && flow.fixture.http.request_count() == 2,
        "rate-limit failure changed or duplicated its durable provider attempt"
    );
    Ok(RateLimitedQuota { timing, status })
}

fn assert_rate_limited_coordinator_timing(
    timing: &CoordinatorTiming,
    retry_deadline_epoch: i64,
    provider_invoked_at_epoch: i64,
    completed_at_epoch: i64,
    last_good_fetched_at_epoch: i64,
) -> Result<()> {
    ensure!(
        timing.generation >= 2
            && timing.phase == "failed"
            && timing.terminal_error_kind.as_deref() == Some("rate_limited")
            && !timing.has_terminal_result
            && timing.provider_invoked_at_epoch == Some(provider_invoked_at_epoch)
            && timing.completed_at_epoch == Some(completed_at_epoch)
            && timing.last_good_fetched_at_epoch == Some(last_good_fetched_at_epoch)
            && timing.last_good_session_used_label.as_deref() == Some("37% used")
            && timing.last_good_weekly_used_label.as_deref() == Some("23% used")
            && timing.success_deadline_epoch.is_none()
            && timing.retry_deadline_epoch == Some(retry_deadline_epoch)
            && timing.rate_limit_deadline_epoch == Some(retry_deadline_epoch),
        "durable coordinator state did not retain the typed 429 and exact retry deadline"
    );
    Ok(())
}

fn restart_before_retry_deadline(
    flow: &mut ProviderFlow,
    case: RetryAfterCase,
    seeded: &SeededQuota,
    limited: &RateLimitedQuota,
) -> Result<()> {
    let restart_epoch = case
        .deadline_epoch
        .checked_sub(300)
        .context("retry deadline was too small for the restart recovery floor")?;
    flow.restart_at(restart_epoch)?;
    let recovered_timing = flow.fixture.coordinator_timing(&flow.source_id)?;
    ensure!(
        recovered_timing == limited.timing,
        "broker restart changed the persisted provider rate-limit attempt"
    );
    let restored_status = flow.fixture.status(&flow.monitor_id)?;
    assert_provider_rate_limited_quota(
        &restored_status,
        &seeded.status,
        seeded.expectation,
        case.deadline_epoch,
    )?;
    ensure!(
        restored_status["issues"] == limited.status["issues"],
        "broker restart changed the exact monitor provider rate-limit issue"
    );
    ensure!(
        flow.fixture.http.request_count() == 2,
        "broker restart retried before the persisted provider deadline"
    );
    let before_deadline = case
        .deadline_epoch
        .checked_sub(1)
        .context("retry deadline underflowed before the pre-deadline check")?;
    flow.set_clock(before_deadline)?;
    assert_request_count_remains(
        &flow.fixture.http,
        2,
        Duration::from_millis(750),
        "provider retried before the exact Retry-After deadline",
    )?;
    let early_status = flow.fixture.status(&flow.monitor_id)?;
    assert_provider_rate_limited_quota(
        &early_status,
        &seeded.status,
        seeded.expectation,
        case.deadline_epoch,
    )?;
    Ok(())
}

fn complete_natural_retry(flow: &mut ProviderFlow, case: RetryAfterCase) -> Result<()> {
    let request_epoch = case.deadline_epoch;
    flow.set_clock(request_epoch)?;
    let request = next_provider_request(
        &flow.fixture,
        &flow.monitor_id,
        &flow.source_id,
        "retry at persisted deadline",
        flow.credential_reads,
    )?;
    assert_provider_request(&request)?;
    ensure!(
        flow.fixture.http.request_count() == 3,
        "exactly one natural provider request was not admitted at Retry-After"
    );
    let attempt = flow.fixture.wait_for_coordinator_attempt(
        &flow.source_id,
        request_epoch,
        Duration::from_secs(5),
    )?;
    let mut watch = flow.fixture.start_watch(&flow.monitor_id)?;
    let sequence = watch.wait_for_event(&flow.monitor_id, Duration::from_secs(10))?;
    let five_hour_reset = request_epoch + 18_000;
    let weekly_reset = INITIAL_WALL_EPOCH + 604_800;
    let five_hour_reset_at = rfc3339_utc_from_epoch(five_hour_reset);
    let weekly_reset_at = rfc3339_utc_from_epoch(weekly_reset);
    request.respond.send(HttpResponse::ok(
        52,
        41,
        &five_hour_reset_at,
        &weekly_reset_at,
    ))?;
    flow.fixture.wait_for_coordinator_completion(
        &flow.source_id,
        CoordinatorCompletionExpectation {
            generation: attempt.generation,
            provider_invoked_at_epoch: request_epoch,
            fetched_at_epoch: request_epoch,
            session_used_label: "52% used",
            weekly_used_label: "41% used",
            timeout: Duration::from_secs(10),
        },
    )?;
    let expectation = QuotaExpectation {
        five_hour_used: 5_200,
        seven_day_used: 4_100,
        five_hour_reset,
        seven_day_reset: weekly_reset,
        evidence_at_epoch: request_epoch,
        received_at_min_epoch: request_epoch,
        received_at_max_epoch: request_epoch + 1,
    };
    flow.set_clock(request_epoch + 1)?;
    watch.wait_for_quota_after(
        &flow.monitor_id,
        sequence,
        expectation,
        Duration::from_secs(15),
    )?;
    watch.wait_for_exit(Duration::from_secs(5))?;
    let status = flow.fixture.status(&flow.monitor_id)?;
    assert_provider_quota(&status, expectation)?;
    ensure!(
        !status["issues"].as_array().is_some_and(|issues| {
            issues
                .iter()
                .any(|issue| issue["code"] == "provider_rate_limited")
        }),
        "successful retry did not clear the provider rate-limit issue"
    );
    assert_request_count_remains(
        &flow.fixture.http,
        3,
        Duration::from_millis(750),
        "provider duplicated the successful natural retry",
    )?;
    Ok(())
}

struct AuthProcess {
    child: Child,
}

impl AuthProcess {
    fn wait_for_exit(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .context("check auth prepare process")?
            {
                ensure!(
                    status.success(),
                    "auth prepare exited with status code {:?}",
                    status.code()
                );
                return Ok(());
            }
            if Instant::now() >= deadline {
                let _kill_result = self.child.kill();
                let _wait_result = self.child.wait();
                bail!("auth prepare process did not exit before its deadline");
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for AuthProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _kill_result = self.child.kill();
            let _wait_result = self.child.wait();
        }
    }
}

struct WatchProcess {
    child: Child,
    receiver: Receiver<String>,
}

impl WatchProcess {
    fn wait_for_exit(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().context("check CLI watch exit")? {
                ensure!(
                    status.success(),
                    "CLI watch exited with status code {:?}",
                    status.code()
                );
                return Ok(());
            }
            if Instant::now() >= deadline {
                let _kill_result = self.child.kill();
                let _wait_result = self.child.wait();
                bail!("CLI watch did not exit before its bounded deadline");
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn wait_for_event(&mut self, monitor_id: &str, timeout: Duration) -> Result<u64> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.receiver.recv_timeout(remaining) {
                Ok(line) if event_monitor_id(&line).as_deref() == Some(monitor_id) => {
                    return serde_json::from_str::<Value>(&line)?["sequence"]
                        .as_u64()
                        .context("watch event omitted its sequence");
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        bail!("CLI watch did not receive the current monitor event")
    }

    fn wait_for_quota_after(
        &mut self,
        monitor_id: &str,
        after_sequence: u64,
        expected: QuotaExpectation,
        timeout: Duration,
    ) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.receiver.recv_timeout(remaining) {
                Ok(line) => {
                    let Ok(event) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    let status = &event["status"];
                    if event["sequence"]
                        .as_u64()
                        .is_some_and(|sequence| sequence > after_sequence)
                        && status["monitor_id"].as_str() == Some(monitor_id)
                        && expected.matches(status)
                    {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        bail!("CLI watch did not receive the expected quota update")
    }

    fn wait_for_rate_limited_after(
        &mut self,
        monitor_id: &str,
        after_sequence: u64,
        expected: QuotaExpectation,
        retry_deadline_epoch: i64,
        timeout: Duration,
    ) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.receiver.recv_timeout(remaining) {
                Ok(line) => {
                    let Ok(event) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    let status = &event["status"];
                    if event["sequence"]
                        .as_u64()
                        .is_some_and(|sequence| sequence > after_sequence)
                        && status["monitor_id"].as_str() == Some(monitor_id)
                        && expected.matches(status)
                        && has_provider_rate_limit_issue(status, retry_deadline_epoch)
                    {
                        return Ok(status.clone());
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        bail!("CLI watch did not receive the expected provider rate-limit issue")
    }
}

impl Drop for WatchProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _kill_result = self.child.kill();
            let _wait_result = self.child.wait();
        }
    }
}

fn event_monitor_id(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("status")?
        .get("monitor_id")?
        .as_str()
        .map(str::to_owned)
}

fn assert_provider_quota(status: &Value, expected: QuotaExpectation) -> Result<()> {
    ensure!(
        status["purpose"] == "observe_only",
        "provider quota status did not remain an observe-only monitor"
    );
    ensure!(
        status["runnable"] == false,
        "observe-only provider monitor unexpectedly became runnable"
    );
    ensure!(
        status["readiness"]["dispatch"] == "not_authorized",
        "observe-only provider monitor unexpectedly gained dispatch authorization"
    );
    ensure!(
        status["readiness"]["quota"] == "ready",
        "provider quota readiness was not ready independently of dispatch authorization"
    );
    ensure!(
        status["readiness"]["provider"] == "current",
        "provider readiness was not current after a successful collection"
    );
    ensure!(
        expected.matches(status),
        "monitor status did not contain provider quota and reset evidence from the expected fetch"
    );
    Ok(())
}

fn assert_known_weekly_reset_watermark(
    account: &Value,
    expected_reset_epoch: i64,
    expected_received_at_epoch: i64,
) -> Result<()> {
    ensure!(
        account["latest_reset_epochs"][1].as_i64() == Some(expected_reset_epoch)
            && account["statusline_reset_watermarks"][1]
                == json!({
                    "known": {
                        "reset_at_epoch": expected_reset_epoch,
                        "received_at_epoch": expected_received_at_epoch,
                    }
                }),
        "durable weekly reset watermark did not retain its reset and original receipt epochs"
    );
    Ok(())
}

fn assert_retained_quota_is_stale(status: &Value, before: &Value) -> Result<()> {
    ensure!(
        status["purpose"] == "observe_only"
            && status["runnable"] == false
            && status["readiness"]["dispatch"] == "not_authorized"
            && status["readiness"]["quota"] == "stale",
        "stale last-good quota was not shown as non-runnable and stale"
    );
    for window in ["five_hour", "seven_day"] {
        ensure!(
            status[window]["used_percentage_basis_points"]
                == before[window]["used_percentage_basis_points"]
                && status[window]["reset_at_epoch"] == before[window]["reset_at_epoch"]
                && status[window]["used_evidence"]["freshness"] == "stale",
            "stale display did not retain the last-good {window} quota and mark used evidence stale"
        );
        for evidence_kind in ["used_evidence", "reset_evidence"] {
            for evidence_field in [
                "evidence_sequence",
                "evidence_at_epoch",
                "evidence_received_at_epoch",
            ] {
                ensure!(
                    status[window][evidence_kind][evidence_field]
                        == before[window][evidence_kind][evidence_field],
                    "stale display changed {window} {evidence_kind} evidence identity"
                );
            }
        }
    }
    Ok(())
}

fn has_provider_rate_limit_issue(status: &Value, retry_deadline_epoch: i64) -> bool {
    status["issues"].as_array().is_some_and(|issues| {
        let matching = issues
            .iter()
            .filter(|issue| issue["code"] == "provider_rate_limited")
            .collect::<Vec<_>>();
        matching.len() == 1
            && matching[0]["message"] == "provider rate limited the usage refresh"
            && matching[0]["retry_at_epoch"].as_i64() == Some(retry_deadline_epoch)
    })
}

fn assert_provider_rate_limited_quota(
    status: &Value,
    before_failure: &Value,
    expected: QuotaExpectation,
    retry_deadline_epoch: i64,
) -> Result<()> {
    ensure!(
        status["purpose"] == "observe_only" && status["runnable"] == false,
        "rate-limited provider quota did not remain observe-only"
    );
    ensure!(
        status["readiness"]["dispatch"] == "not_authorized",
        "rate-limited observe-only monitor gained dispatch authorization"
    );
    ensure!(
        status["readiness"]["provider"] == "rate_limited",
        "monitor status did not preserve rate-limited provider readiness"
    );
    ensure!(
        status["readiness"]["quota"] == "stale",
        "rate-limited provider made aged last-good quota appear ready"
    );
    ensure!(
        expected.matches(status),
        "rate-limited status did not retain the last-good quota and evidence"
    );
    ensure!(
        has_provider_rate_limit_issue(status, retry_deadline_epoch),
        "monitor status did not expose exactly one provider rate-limit issue at the exact retry deadline"
    );
    for window in ["five_hour", "seven_day"] {
        ensure!(
            status[window]["used_percentage_basis_points"]
                == before_failure[window]["used_percentage_basis_points"]
                && status[window]["reset_at_epoch"] == before_failure[window]["reset_at_epoch"],
            "rate-limited provider replaced the last-good {window} quota"
        );
        ensure!(
            status[window]["used_evidence"]["freshness"] == "stale",
            "rate-limited {window} used evidence did not become stale"
        );
        for evidence_kind in ["used_evidence", "reset_evidence"] {
            for evidence_field in [
                "evidence_sequence",
                "evidence_at_epoch",
                "evidence_received_at_epoch",
            ] {
                ensure!(
                    status[window][evidence_kind][evidence_field]
                        == before_failure[window][evidence_kind][evidence_field],
                    "rate-limited provider changed last-good {window} {evidence_kind} identity"
                );
            }
        }
    }
    Ok(())
}

fn assert_request_count_remains(
    server: &FakeHttpServer,
    expected_count: usize,
    timeout: Duration,
    reason: &str,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        ensure!(
            server.request_count() == expected_count,
            "{reason}: fake provider request count changed"
        );
        if Instant::now() >= deadline {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn next_provider_request(
    fixture: &Fixture,
    monitor_id: &str,
    source_id: &str,
    cycle: &str,
    credential_reads: usize,
) -> Result<FakeRequest> {
    match fixture.http.next_request(PROVIDER_REQUEST_TIMEOUT) {
        Ok(request) => Ok(request),
        Err(request_error) => {
            let status_args = ["status", "--monitor", monitor_id];
            let monitor_status = match fixture.status(monitor_id) {
                Ok(status) => json!({ "result": "status", "status": status }),
                Err(_) => fixture.diagnostic_json(&status_args),
            };
            let service_status = fixture.diagnostic_json(&["service", "status"]);
            let coordinator_timing = match fixture.coordinator_timing(source_id) {
                Ok(timing) => timing.diagnostic_json(),
                Err(_) => json!({ "unavailable": true }),
            };
            let diagnostics = json!({
                "cycle": cycle,
                "monitor_status": monitor_status,
                "service_status": service_status,
                "http_request_count": fixture.http.request_count(),
                "fake_server": fixture.http.diagnostic_json(),
                "credential_reads": credential_reads,
                "coordinator_timing": coordinator_timing,
            });
            bail!(
                "{cycle} natural provider request was not admitted ({request_error}); diagnostics: {diagnostics}"
            )
        }
    }
}

fn assert_provider_request(request: &FakeRequest) -> Result<()> {
    ensure!(
        request.method == "GET",
        "fake provider received a non-GET request"
    );
    ensure!(
        request.path == "/api/oauth/usage",
        "fake provider received a request for an unexpected path"
    );
    ensure!(
        request.authorization_matches_expected,
        "fake provider Authorization did not match the private synthetic credential"
    );
    Ok(())
}

struct FakeHttpServer {
    endpoint: String,
    requests: Receiver<FakeRequest>,
    counters: Arc<FakeHttpCounters>,
    expected_token: Arc<Mutex<String>>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct FakeHttpCounters {
    accepted_connections: AtomicUsize,
    accept_failures: AtomicUsize,
    read_failures: AtomicUsize,
    partial_requests: AtomicUsize,
    malformed_requests: AtomicUsize,
    write_failures: AtomicUsize,
    requests: AtomicUsize,
    last_request_bytes_read: AtomicUsize,
    last_header_read_elapsed_millis: AtomicU64,
    last_header_read_error_kind: Mutex<Option<String>>,
}

struct FakeHttpCounterSnapshot {
    accepted_connections: usize,
    accept_failures: usize,
    read_failures: usize,
    partial_requests: usize,
    malformed_requests: usize,
    write_failures: usize,
    requests: usize,
    last_request_bytes_read: usize,
    last_header_read_elapsed_millis: u64,
    last_header_read_error_kind: Option<String>,
}

impl FakeHttpCounters {
    fn snapshot(&self) -> FakeHttpCounterSnapshot {
        FakeHttpCounterSnapshot {
            accepted_connections: self.accepted_connections.load(Ordering::Acquire),
            accept_failures: self.accept_failures.load(Ordering::Acquire),
            read_failures: self.read_failures.load(Ordering::Acquire),
            partial_requests: self.partial_requests.load(Ordering::Acquire),
            malformed_requests: self.malformed_requests.load(Ordering::Acquire),
            write_failures: self.write_failures.load(Ordering::Acquire),
            requests: self.requests.load(Ordering::Acquire),
            last_request_bytes_read: self.last_request_bytes_read.load(Ordering::Acquire),
            last_header_read_elapsed_millis: self
                .last_header_read_elapsed_millis
                .load(Ordering::Acquire),
            last_header_read_error_kind: self
                .last_header_read_error_kind
                .lock()
                .ok()
                .and_then(|kind| kind.clone()),
        }
    }

    fn record_header_read(
        &self,
        bytes_read: usize,
        elapsed: Duration,
        error_kind: Option<std::io::ErrorKind>,
    ) {
        self.last_request_bytes_read
            .store(bytes_read, Ordering::Release);
        self.last_header_read_elapsed_millis.store(
            elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
            Ordering::Release,
        );
        if let Ok(mut last_kind) = self.last_header_read_error_kind.lock() {
            *last_kind = error_kind.map(|kind| format!("{kind:?}"));
        }
    }
}

impl FakeHttpCounterSnapshot {
    fn diagnostic_json(&self) -> Value {
        json!({
            "accepted_connections": self.accepted_connections,
            "accept_failures": self.accept_failures,
            "read_failures": self.read_failures,
            "partial_requests": self.partial_requests,
            "malformed_requests": self.malformed_requests,
            "write_failures": self.write_failures,
            "complete_requests": self.requests,
            "last_request_bytes_read": self.last_request_bytes_read,
            "last_header_read_elapsed_millis": self.last_header_read_elapsed_millis,
            "last_header_read_error_kind": self.last_header_read_error_kind.as_deref(),
        })
    }

    fn timeout_kind(&self) -> &'static str {
        if self.accepted_connections == 0 {
            "no TCP connection reached the fake provider"
        } else if self.partial_requests > 0 {
            "fake provider accepted a connection but received partial request headers"
        } else if self.malformed_requests > 0 {
            "fake provider accepted a connection but received malformed request headers"
        } else if self.read_failures > 0 {
            "fake provider accepted a connection but failed while reading request headers"
        } else {
            "fake provider accepted a connection but did not publish a complete request"
        }
    }
}

impl FakeHttpServer {
    fn from_listener(listener: TcpListener, expected_token: String) -> Result<Self> {
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let endpoint = format!("http://{address}/api/oauth/usage");
        let (sender, requests) = mpsc::channel();
        let counters = Arc::new(FakeHttpCounters::default());
        let counters_for_thread = Arc::clone(&counters);
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_for_thread = Arc::clone(&shutdown);
        let token = Arc::new(Mutex::new(expected_token));
        let token_for_thread = Arc::clone(&token);
        let thread = thread::spawn(move || {
            while !shutdown_for_thread.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        counters_for_thread
                            .accepted_connections
                            .fetch_add(1, Ordering::AcqRel);
                        let _serve_result = serve_provider_request(
                            stream,
                            &sender,
                            &counters_for_thread,
                            &token_for_thread,
                            &shutdown_for_thread,
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        counters_for_thread
                            .accept_failures
                            .fetch_add(1, Ordering::AcqRel);
                        break;
                    }
                }
            }
        });
        Ok(Self {
            endpoint,
            requests,
            counters,
            expected_token: token,
            shutdown,
            thread: Some(thread),
        })
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    fn next_request(&self, timeout: Duration) -> Result<FakeRequest> {
        match self.requests.recv_timeout(timeout) {
            Ok(request) => Ok(request),
            Err(RecvTimeoutError::Timeout) => {
                let snapshot = self.counters.snapshot();
                bail!(
                    "{} before the deadline; bounded fake-server counters: {}",
                    snapshot.timeout_kind(),
                    snapshot.diagnostic_json()
                )
            }
            Err(RecvTimeoutError::Disconnected) => {
                let snapshot = self.counters.snapshot();
                bail!(
                    "fake provider server stopped before publishing a request; bounded fake-server counters: {}",
                    snapshot.diagnostic_json()
                )
            }
        }
    }

    fn request_count(&self) -> usize {
        self.counters.requests.load(Ordering::Acquire)
    }

    fn set_expected_token(&self, token: &str) -> Result<()> {
        *self
            .expected_token
            .lock()
            .map_err(|_| anyhow::anyhow!("fake provider credential state was poisoned"))? =
            token.to_owned();
        Ok(())
    }

    fn diagnostic_json(&self) -> Value {
        self.counters.snapshot().diagnostic_json()
    }
}

impl Drop for FakeHttpServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _join_result = thread.join();
        }
    }
}

struct FakeRequest {
    method: String,
    path: String,
    authorization_matches_expected: bool,
    respond: SyncSender<HttpResponse>,
}

struct HttpResponse {
    status: u16,
    reason: &'static str,
    retry_after: Option<String>,
    body: String,
}

impl HttpResponse {
    fn ok(five_hour: u8, seven_day: u8, five_hour_reset: &str, seven_day_reset: &str) -> Self {
        Self {
            status: 200,
            reason: "OK",
            retry_after: None,
            body: json!({
                "five_hour": {"utilization": five_hour, "resets_at": five_hour_reset},
                "seven_day": {"utilization": seven_day, "resets_at": seven_day_reset},
            })
            .to_string(),
        }
    }

    fn too_many_requests(retry_after: &str) -> Self {
        Self {
            status: 429,
            reason: "Too Many Requests",
            retry_after: Some(retry_after.to_owned()),
            body: String::new(),
        }
    }

    fn unauthorized(body: Option<&str>) -> Self {
        Self {
            status: 401,
            reason: "Unauthorized",
            retry_after: None,
            body: body.unwrap_or_default().to_owned(),
        }
    }

    fn forbidden(body: Option<&str>) -> Self {
        Self {
            status: 403,
            reason: "Forbidden",
            retry_after: None,
            body: body.unwrap_or_default().to_owned(),
        }
    }
}

fn serve_provider_request(
    mut stream: TcpStream,
    requests: &mpsc::Sender<FakeRequest>,
    counters: &FakeHttpCounters,
    expected_token: &Mutex<String>,
    shutdown: &AtomicBool,
) -> Result<()> {
    let parsed_request = match parse_request(&mut stream, expected_token) {
        Ok(request) => request,
        Err(failure) => {
            let stats = match failure {
                RequestParseFailure::Read { stats, .. }
                | RequestParseFailure::Partial { stats }
                | RequestParseFailure::Malformed { stats } => stats,
            };
            counters.record_header_read(stats.bytes_read, stats.elapsed, failure.error_kind());
            match failure {
                RequestParseFailure::Read { kind, stats } => {
                    counters.read_failures.fetch_add(1, Ordering::AcqRel);
                    bail!(
                        "fake provider header read failed ({kind:?}) after {}ms with {} bytes",
                        stats.elapsed.as_millis(),
                        stats.bytes_read
                    );
                }
                RequestParseFailure::Partial { stats } => {
                    counters.partial_requests.fetch_add(1, Ordering::AcqRel);
                    bail!(
                        "fake provider received partial request headers after {}ms with {} bytes",
                        stats.elapsed.as_millis(),
                        stats.bytes_read
                    );
                }
                RequestParseFailure::Malformed { stats } => {
                    counters.malformed_requests.fetch_add(1, Ordering::AcqRel);
                    bail!(
                        "fake provider received malformed request headers after {}ms with {} bytes",
                        stats.elapsed.as_millis(),
                        stats.bytes_read
                    );
                }
            }
        }
    };
    counters.record_header_read(
        parsed_request.stats.bytes_read,
        parsed_request.stats.elapsed,
        None,
    );
    let (respond, response) = mpsc::sync_channel(1);
    counters.requests.fetch_add(1, Ordering::AcqRel);
    requests.send(FakeRequest {
        method: parsed_request.method,
        path: parsed_request.path,
        authorization_matches_expected: parsed_request.authorization_matches_expected,
        respond,
    })?;
    loop {
        if shutdown.load(Ordering::Acquire) {
            return Ok(());
        }
        match response.recv_timeout(Duration::from_millis(100)) {
            Ok(response) => {
                let bytes = response.body.as_bytes();
                let retry_after_header = response
                    .retry_after
                    .as_ref()
                    .map_or_else(String::new, |retry_after| {
                        format!("Retry-After: {retry_after}\r\n")
                    });
                let header = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\n{retry_after_header}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    response.status,
                    response.reason,
                    bytes.len()
                );
                let write_result = stream
                    .write_all(header.as_bytes())
                    .and_then(|()| stream.write_all(bytes))
                    .and_then(|()| stream.flush());
                if write_result.is_err() {
                    counters.write_failures.fetch_add(1, Ordering::AcqRel);
                    bail!("fake provider failed while writing its response");
                }
                return Ok(());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

const FAKE_HEADER_READ_DEADLINE: Duration = Duration::from_secs(12);
const FAKE_HEADER_READ_POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_FAKE_HEADER_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy)]
struct HeaderReadStats {
    bytes_read: usize,
    elapsed: Duration,
}

struct ParsedRequest {
    method: String,
    path: String,
    authorization_matches_expected: bool,
    stats: HeaderReadStats,
}

#[derive(Clone, Copy)]
enum RequestParseFailure {
    Read {
        kind: std::io::ErrorKind,
        stats: HeaderReadStats,
    },
    Partial {
        stats: HeaderReadStats,
    },
    Malformed {
        stats: HeaderReadStats,
    },
}

impl RequestParseFailure {
    fn error_kind(self) -> Option<std::io::ErrorKind> {
        match self {
            Self::Read { kind, .. } => Some(kind),
            Self::Partial { .. } | Self::Malformed { .. } => None,
        }
    }
}

fn parse_request(
    stream: &mut TcpStream,
    expected_token: &Mutex<String>,
) -> std::result::Result<ParsedRequest, RequestParseFailure> {
    let read_started_at = Instant::now();
    let read_deadline = read_started_at + FAKE_HEADER_READ_DEADLINE;
    let mut bytes = Vec::with_capacity(2048);
    let mut chunk = [0_u8; 1024];
    while bytes.len() < MAX_FAKE_HEADER_BYTES {
        let remaining = read_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(RequestParseFailure::Read {
                kind: std::io::ErrorKind::TimedOut,
                stats: header_read_stats(&bytes, read_started_at),
            });
        }
        let read_timeout = remaining.min(FAKE_HEADER_READ_POLL_INTERVAL);
        if let Err(error) = stream.set_read_timeout(Some(read_timeout)) {
            return Err(RequestParseFailure::Read {
                kind: error.kind(),
                stats: header_read_stats(&bytes, read_started_at),
            });
        }
        let read_capacity = (MAX_FAKE_HEADER_BYTES - bytes.len()).min(chunk.len());
        let count = match stream.read(&mut chunk[..read_capacity]) {
            Ok(count) => count,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) && Instant::now() < read_deadline =>
            {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(error) => {
                return Err(RequestParseFailure::Read {
                    kind: error.kind(),
                    stats: header_read_stats(&bytes, read_started_at),
                });
            }
        };
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let Some(header_end) = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|delimiter_start| delimiter_start + 4)
    else {
        return Err(RequestParseFailure::Partial {
            stats: header_read_stats(&bytes, read_started_at),
        });
    };
    let stats = header_read_stats(&bytes, read_started_at);
    let header_bytes = &bytes[..header_end];
    let text =
        std::str::from_utf8(header_bytes).map_err(|_| RequestParseFailure::Malformed { stats })?;
    let mut lines = text.lines();
    let request_line = lines
        .next()
        .ok_or(RequestParseFailure::Malformed { stats })?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or(RequestParseFailure::Malformed { stats })?
        .to_owned();
    let path = parts
        .next()
        .ok_or(RequestParseFailure::Malformed { stats })?
        .to_owned();
    let version = parts
        .next()
        .ok_or(RequestParseFailure::Malformed { stats })?;
    if parts.next().is_some() || !version.starts_with("HTTP/") {
        return Err(RequestParseFailure::Malformed { stats });
    }
    let expected_token =
        lock_token(expected_token).map_err(|_| RequestParseFailure::Malformed { stats })?;
    let expected = format!("Bearer {expected_token}");
    let authorization_matches_expected =
        lines
            .filter_map(|line| line.split_once(':'))
            .any(|(name, value)| {
                name.eq_ignore_ascii_case("authorization") && value.trim() == expected
            });
    // `bytes` temporarily contained request headers. Do not retain or format
    // them: in particular, never put the bearer value in test diagnostics.
    drop(bytes);
    Ok(ParsedRequest {
        method,
        path,
        authorization_matches_expected,
        stats,
    })
}

fn header_read_stats(bytes: &[u8], read_started_at: Instant) -> HeaderReadStats {
    HeaderReadStats {
        bytes_read: bytes.len(),
        elapsed: read_started_at.elapsed(),
    }
}

fn lock_token(token: &Mutex<String>) -> Result<String> {
    token
        .lock()
        .map(|value| value.clone())
        .map_err(|_| anyhow::anyhow!("fake provider credential state was poisoned"))
}

fn exited_child_pid() -> Result<u32> {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .spawn()
        .context("spawn a short-lived child for the stale lease fixture")?;
    let child_pid = child.id();
    ensure!(
        child.wait()?.success(),
        "short-lived child did not exit successfully"
    );
    let signal_pid = i32::try_from(child_pid).context("child process ID exceeds i32")?;
    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(signal_pid), None) {
        Err(nix::errno::Errno::ESRCH) => Ok(child_pid),
        Ok(()) => bail!("short-lived child PID was reused or remains active"),
        Err(error) => bail!("could not prove short-lived child PID is absent: {error}"),
    }
}

fn is_sha256_instance_id(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn stale_v8_instance_id() -> String {
    jackin_core::account_key_hash("usage-broker-instance-v1", "synthetic-stale-v8-instance")
}

fn write_private_json(path: &Path, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn json_from_output(output: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(output);
    text.lines().rev().find_map(|line| {
        let start = line.find('{')?;
        serde_json::from_str(line[start..].trim()).ok()
    })
}

fn safe_json_error_message(reply: &Value) -> Option<&'static str> {
    match reply["error"]["message"].as_str()? {
        "fixture broker could not start" => Some("fixture broker could not start"),
        "no fixture credential is available" => Some("no fixture credential is available"),
        "fixture credential access was denied" => Some("fixture credential access was denied"),
        "fixture bootstrap requires an attached terminal" => {
            Some("fixture bootstrap requires an attached terminal")
        }
        "fixture credential is invalid" => Some("fixture credential is invalid"),
        "authentication preparation requires an attached terminal" => {
            Some("authentication preparation requires an attached terminal")
        }
        "authentication bootstrap arguments are invalid" => {
            Some("authentication bootstrap arguments are invalid")
        }
        "usage broker is unavailable" => Some("usage broker is unavailable"),
        "monitor state is unavailable or invalid" => {
            Some("monitor state is unavailable or invalid")
        }
        "host usage broker is unavailable or returned an incompatible response" => {
            Some("host usage broker is unavailable or returned an incompatible response")
        }
        "usage broker test fixture configuration is unavailable" => {
            Some("usage broker test fixture configuration is unavailable")
        }
        _ => None,
    }
}

fn redact_diagnostic_stderr(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(|line| {
            let lowercase = line.to_ascii_lowercase();
            if lowercase.contains("authorization:") || lowercase.contains("bearer ") {
                "[redacted sensitive diagnostic line]".to_owned()
            } else {
                line.replace(FIRST_TOKEN, "[redacted]")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn rfc3339_utc_from_epoch(epoch: i64) -> String {
    const SECONDS_PER_DAY: i64 = 86_400;

    let days = epoch.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = epoch.rem_euclid(SECONDS_PER_DAY);
    let shifted_days = days + 719_468;
    let era = shifted_days.div_euclid(146_097);
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }

    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn output_with_timeout(mut command: Command, timeout: Duration, label: &str) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().with_context(|| format!("{label}: spawn"))?;
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .with_context(|| format!("{label}: collect output"));
        }
        if Instant::now() >= deadline {
            let _kill_result = child.kill();
            let _wait_result = child.wait();
            bail!("{label}: timed out");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn output_with_stdin_timeout(
    mut command: Command,
    stdin_bytes: &[u8],
    timeout: Duration,
    label: &str,
) -> Result<Output> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().with_context(|| format!("{label}: spawn"))?;
    let mut stdin = child.stdin.take().context("child stdin was not piped")?;
    if let Err(error) = stdin.write_all(stdin_bytes) {
        drop(stdin);
        let _kill_result = child.kill();
        let _wait_result = child.wait();
        return Err(error).with_context(|| format!("{label}: write input"));
    }
    drop(stdin);
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .with_context(|| format!("{label}: collect output"));
        }
        if Instant::now() >= deadline {
            let _kill_result = child.kill();
            let _wait_result = child.wait();
            bail!("{label}: timed out");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_until<T>(timeout: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(value) = check() {
            return Some(value);
        }
        thread::sleep(Duration::from_millis(100));
    }
    None
}

static NEXT_SCRIPT_FILE: AtomicUsize = AtomicUsize::new(0);
