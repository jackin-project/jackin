// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

const SECOND_TOKEN: &str = "synthetic-claude-access-token-two";
const PROVIDER_ATTEMPT_OFFSET_SECONDS: i64 = 1;
const PROVIDER_ATTEMPT_OFFSET_MILLIS: u64 = 1_000;
const TERMINAL_TIMEOUT: Duration = Duration::from_secs(15);

#[test]
fn cli_provider_auth_outcomes_keep_the_selected_source_and_typed_terminal() -> Result<()> {
    let _test_guard = acquire_provider_flow_test_lock();
    unchanged_401_does_not_retry()?;
    changed_401_retries_only_with_the_exact_source_payload()?;
    run_non_401_failure(Non401Failure::Forbidden)?;
    run_non_401_failure(Non401Failure::Transport)?;
    run_non_401_failure(Non401Failure::Timeout)?;
    Ok(())
}

struct BoundScenario {
    fixture: Fixture,
    source_id: String,
    binding_id: String,
    monitor_id: String,
    credential_reads: usize,
    provider_invoked_at_epoch: i64,
}

impl BoundScenario {
    fn new() -> Result<Self> {
        let mut fixture = Fixture::new()?;
        fixture.write_credential(FIRST_TOKEN)?;
        fixture.start_auth()?;

        let service = fixture.wait_service_ready()?;
        let credential_reads = fixture.wait_ticker_ready()?;
        ensure!(
            credential_reads == 1,
            "fixture did not bootstrap exactly one selected credential source"
        );
        let source_id = service["status"]["experimental_collector_source"]
            .as_str()
            .context("foreground service omitted its selected Claude source")?
            .to_owned();
        ensure!(
            source_id.len() == 64
                && source_id
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
            "foreground service source ID was not the expected hash of the Keychain service"
        );

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
            "synthetic provider auth-outcome test",
            "--confirm",
        ])?;
        ensure!(
            binding_reply["result"] == "account_bound"
                && binding_reply["binding"]["provider_account_id"].as_str()
                    == Some(source_id.as_str())
                && binding_reply["binding"]["experimental_collector_approved"] == true,
            "CLI binding did not retain the exact hashed foreground source"
        );
        let binding_id = binding_reply["binding"]["binding_id"]
            .as_str()
            .context("CLI binding reply omitted its ID")?
            .to_owned();
        let binding_revision = binding_reply["binding"]["revision"]
            .as_u64()
            .context("CLI binding reply omitted its revision")?;
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
            "synthetic-auth-outcomes-observer-v1",
            "--experimental-collector",
        ])?;
        ensure!(
            observe_reply["result"] == "started",
            "CLI did not start the bound observe-only monitor"
        );
        let monitor_id = observe_reply["status"]["monitor_id"]
            .as_str()
            .context("CLI observer reply omitted its monitor ID")?
            .to_owned();

        let provider_invoked_at_epoch =
            fixture.initial_wall_epoch + PROVIDER_ATTEMPT_OFFSET_SECONDS;
        ensure!(
            fixture.set_clock(
                provider_invoked_at_epoch,
                INITIAL_MONOTONIC_MILLIS + PROVIDER_ATTEMPT_OFFSET_MILLIS,
            )? == credential_reads,
            "provider admission unexpectedly reread the selected credential"
        );

        Ok(Self {
            fixture,
            source_id,
            binding_id,
            monitor_id,
            credential_reads,
            provider_invoked_at_epoch,
        })
    }

    fn first_request(&self, label: &str) -> Result<(FakeRequest, CoordinatorTiming)> {
        let request = next_provider_request(
            &self.fixture,
            &self.monitor_id,
            &self.source_id,
            label,
            self.credential_reads,
        )?;
        assert_provider_request(&request)?;
        let timing = self.fixture.wait_for_coordinator_attempt(
            &self.source_id,
            self.provider_invoked_at_epoch,
            Duration::from_secs(5),
        )?;
        Ok((request, timing))
    }

    fn wait_for_terminal(
        &self,
        timing: &CoordinatorTiming,
        error_kind: &str,
    ) -> Result<CoordinatorTiming> {
        let terminal = self.fixture.wait_for_coordinator_terminal(
            &self.source_id,
            timing.generation,
            self.provider_invoked_at_epoch,
            TERMINAL_TIMEOUT,
        )?;
        ensure!(
            terminal.generation == timing.generation
                && terminal.phase == "failed"
                && terminal.terminal_error_kind.as_deref() == Some(error_kind)
                && !terminal.has_terminal_result
                && terminal.provider_invoked_at_epoch == Some(self.provider_invoked_at_epoch),
            "provider failure did not reach the expected durable typed terminal"
        );
        Ok(terminal)
    }

    fn assert_selected_source_is_stable(&self) -> Result<()> {
        let service = self.fixture.run_json(&["service", "status"])?;
        ensure!(
            service["status"]["experimental_collector_source"].as_str()
                == Some(self.source_id.as_str()),
            "provider outcome changed the selected hashed Keychain service source"
        );
        let status = self.fixture.status(&self.monitor_id)?;
        ensure!(
            status["scope"]["scope"] == "bound_account"
                && status["scope"]["binding_id"].as_str() == Some(self.binding_id.as_str()),
            "provider outcome changed the monitor's selected account binding"
        );
        // coordinator_timing validates the durable capability's account ID
        // against source_id while reading the source-scoped state file.
        let _timing = self.fixture.coordinator_timing(&self.source_id)?;
        Ok(())
    }

    fn assert_credential_reads(&self, expected_reads: usize) -> Result<()> {
        let reads = self.fixture.set_clock(
            self.provider_invoked_at_epoch,
            INITIAL_MONOTONIC_MILLIS + PROVIDER_ATTEMPT_OFFSET_MILLIS,
        )?;
        ensure!(
            reads == expected_reads,
            "fake Keychain reader performed an unexpected number of exact-source reads"
        );
        Ok(())
    }

    fn assert_request_count_after_quiet_interval(&self, expected: usize) -> Result<()> {
        thread::sleep(Duration::from_millis(500));
        ensure!(
            self.fixture.http.request_count() == expected,
            "provider issued an unexpected additional HTTP request"
        );
        Ok(())
    }
}

fn unchanged_401_does_not_retry() -> Result<()> {
    let mut scenario = BoundScenario::new()?;
    let (request, timing) = scenario.first_request("401 unchanged")?;
    request.respond.send(HttpResponse::unauthorized(None))?;
    scenario.wait_for_terminal(&timing, "needs_secret")?;

    scenario.assert_credential_reads(2)?;
    ensure!(
        scenario.fixture.http.request_count() == 1,
        "unchanged credential after HTTP 401 caused a second provider request"
    );
    scenario.assert_selected_source_is_stable()?;
    scenario.assert_request_count_after_quiet_interval(1)?;
    scenario.fixture.stop_auth()?;
    Ok(())
}

fn changed_401_retries_only_with_the_exact_source_payload() -> Result<()> {
    let mut scenario = BoundScenario::new()?;
    let (first_request, timing) = scenario.first_request("401 changed")?;

    // Change only the payload file already selected for this exact service.
    // The matcher stores only the expected synthetic token and reports a
    // boolean from the parsed Authorization header; it never retains headers.
    scenario.fixture.write_credential(SECOND_TOKEN)?;
    scenario.fixture.http.set_expected_token(SECOND_TOKEN)?;
    first_request
        .respond
        .send(HttpResponse::unauthorized(None))?;

    let retry_request = scenario
        .fixture
        .http
        .next_request(Duration::from_secs(5))
        .context("changed exact-source credential did not issue its one retry")?;
    assert_provider_request(&retry_request)?;
    ensure!(
        scenario.fixture.http.request_count() == 2,
        "changed credential did not produce exactly one retry request"
    );
    let five_hour_reset = rfc3339_utc_from_epoch(scenario.provider_invoked_at_epoch + 6_400);
    let seven_day_reset = rfc3339_utc_from_epoch(scenario.provider_invoked_at_epoch + 604_800);
    retry_request
        .respond
        .send(HttpResponse::ok(37, 23, &five_hour_reset, &seven_day_reset))?;
    let completed = scenario.fixture.wait_for_coordinator_completion(
        &scenario.source_id,
        timing.generation,
        scenario.provider_invoked_at_epoch,
        "37% used",
        "23% used",
        Duration::from_secs(10),
    )?;
    ensure!(
        completed.phase == "completed" && completed.has_terminal_result,
        "changed credential retry did not persist its provider result"
    );
    scenario.assert_credential_reads(2)?;
    scenario.assert_selected_source_is_stable()?;
    scenario.assert_request_count_after_quiet_interval(2)?;
    scenario.fixture.stop_auth()?;
    Ok(())
}

#[derive(Clone, Copy)]
enum Non401Failure {
    Forbidden,
    Transport,
    Timeout,
}

impl Non401Failure {
    const fn terminal_kind(self) -> &'static str {
        match self {
            Self::Forbidden => "unauthorized",
            Self::Transport => "provider_unavailable",
            Self::Timeout => "provider_timeout",
        }
    }
}

fn run_non_401_failure(failure: Non401Failure) -> Result<()> {
    let mut scenario = BoundScenario::new()?;
    let (request, timing) = scenario.first_request(match failure {
        Non401Failure::Forbidden => "403",
        Non401Failure::Transport => "transport failure",
        Non401Failure::Timeout => "provider timeout",
    })?;

    let terminal = match failure {
        Non401Failure::Forbidden => {
            request.respond.send(HttpResponse::forbidden(None))?;
            scenario.wait_for_terminal(&timing, failure.terminal_kind())?
        }
        Non401Failure::Transport => {
            drop(request);
            scenario.wait_for_terminal(&timing, failure.terminal_kind())?
        }
        Non401Failure::Timeout => {
            // Keep the fake server response channel open. The real provider
            // HTTP client's ten-second request deadline must end this probe.
            let terminal = scenario.wait_for_terminal(&timing, failure.terminal_kind())?;
            drop(request);
            terminal
        }
    };
    ensure!(
        terminal.terminal_error_kind.as_deref() == Some(failure.terminal_kind()),
        "non-401 provider outcome was recorded under the wrong terminal kind"
    );
    scenario.assert_credential_reads(1)?;
    ensure!(
        scenario.fixture.http.request_count() == 1,
        "non-401 provider outcome caused an unexpected retry"
    );
    scenario.assert_selected_source_is_stable()?;
    scenario.assert_request_count_after_quiet_interval(1)?;
    scenario.fixture.stop_auth()?;
    Ok(())
}
