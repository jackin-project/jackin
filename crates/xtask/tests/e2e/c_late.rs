// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use serde_json::{Value, json};

use jackin_core::CODEX_PLACEHOLDER_ID_TOKEN;

use super::scenario::world::{ACCESS, ID_TOKEN};
use super::scenario::{ExportMode, PtyFault, Scenario};
use super::support::{blocking_fault, fault};

type Tweak = Box<dyn Fn(&Scenario)>;

fn patched(step: &'static str, call: Option<u32>, pointer: &'static str, value: Value) -> Tweak {
    Box::new(move |s: &Scenario| s.patch(step, call, pointer, value.clone()))
}

fn curl_missing() -> Value {
    json!({"url": "https://example.invalid/", "curl_found": false, "curl_exit": null, "http_code": 0})
}

fn canonical_role(scenario: &Scenario) -> String {
    let path = std::fs::canonicalize(&scenario.world.role).expect("role");
    format!("role check {}", path.display())
}

async fn table(step: &str, rows: Vec<(&str, &str, Tweak)>) {
    for (case, needle, tweak) in rows {
        fault(case, step, needle, |scenario| tweak(scenario)).await;
    }
}

#[tokio::test]
async fn j06_hook_faults() {
    table(
        "J06",
        vec![
            (
                "setup ran twice",
                "setup 2",
                patched("J06", None, "/facts/setup_runs", json!(2)),
            ),
            (
                "preflight ran once",
                "preflight 1",
                patched("J06", None, "/facts/preflight_runs", json!(1)),
            ),
            (
                "load 1 exits 0",
                "exited 0",
                Box::new(|s| s.knobs().preflight_fail_exits_zero = true),
            ),
            (
                "hook text lacks the exit code",
                "did not report the preflight hook exiting with code 17",
                Box::new(|s| s.knobs().hook_error_text = Some("launch failed: boom".to_owned())),
            ),
            (
                "another hook exits 17",
                "did not report the preflight hook exiting with code 17",
                Box::new(|s| {
                    s.knobs().hook_error_text = Some("launch failed at step hooks: the setup hook exited with code 17 SENTINEL_FAIL_PREFLIGHT=1".to_owned());
                }),
            ),
            (
                "the preflight hook exits 3",
                "did not report the preflight hook exiting with code 17",
                Box::new(|s| {
                    s.knobs().hook_error_text = Some("launch failed at step hooks: the preflight hook exited with code 3 SENTINEL_FAIL_PREFLIGHT=1".to_owned());
                }),
            ),
            (
                "the failure is not at the hooks step",
                "did not fail at the hooks step",
                Box::new(|s| {
                    s.knobs().hook_error_text = Some("launch failed at step mounts: the preflight hook exited with code 17 SENTINEL_FAIL_PREFLIGHT=1".to_owned());
                }),
            ),
            (
                "phase Error",
                "not Ready after the failed preflight",
                Box::new(|s| s.knobs().error_phase_after_failed_load = true),
            ),
            (
                "export exits 1",
                "jackin export did not work",
                Box::new(|s| s.host_rule("export", Some(1), 1, "nope")),
            ),
            (
                "hook file writable",
                "writable=true",
                patched("J06", None, "/facts/hooks/0/writable", json!(true)),
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn j07_codex_process_faults() {
    let rows: Vec<(&str, &str, Tweak)> = vec![
        (
            "codex ppid is not the main pid",
            "is not the jackin-start pid",
            patched("J07", None, "/facts/processes/codex/0/ppid", json!(1)),
        ),
        (
            "probe and ps disagree",
            "but ps shows",
            patched("J07", None, "/facts/processes/codex/0/pid", json!(9999)),
        ),
        (
            "exec did not answer",
            "did not answer the fixed prompt",
            patched("J07", None, "/facts/codex_exec/answered", json!(false)),
        ),
        (
            "phase Stopped after quit",
            "phase after quit is not Ready",
            Box::new(|s| s.knobs().stopped_after_quit = true),
        ),
        (
            "supervisor still running after quit",
            "not `exited 1 <code>`",
            Box::new(|s| s.knobs().quit_supervisor = Some("running 1".to_owned())),
        ),
        (
            "supervisor not running 2 after restart",
            "not running 2",
            Box::new(|s| s.knobs().restart_supervisor = Some("running 1".to_owned())),
        ),
        (
            "pid unchanged after restart",
            "did not start a new codex process",
            Box::new(|s| s.knobs().restart_same_pid = true),
        ),
    ];
    for (case, needle, tweak) in rows {
        blocking_fault(case, "J07", needle, &["J11", "J12"], |scenario| {
            tweak(scenario);
        })
        .await;
    }
}

#[tokio::test]
async fn j08_credential_faults() {
    let leak = |text: &str| text.to_owned();
    table(
        "J08",
        vec![
            (
                "needle in load stderr",
                "needle found in J03 load 1 stderr",
                Box::new(move |s| s.knobs().leak_in_load_stderr = Some(leak(ACCESS))),
            ),
            (
                "needle in inspect JSON",
                "inspect stdout",
                Box::new(|s| s.knobs().leak_in_inspect = Some(ACCESS.to_owned())),
            ),
            (
                "needle in the env file",
                "J08 env file",
                Box::new(|s| s.knobs().leak_in_env_file = Some(ACCESS.to_owned())),
            ),
            (
                "needle in auth.json",
                "J08 auth.json",
                Box::new(|s| s.knobs().leak_in_auth_file = Some(ACCESS.to_owned())),
            ),
            (
                "needle in a PTY transcript",
                "attach transcript",
                Box::new(|s| s.knobs().leak_in_pty = Some(ACCESS.to_owned())),
            ),
            (
                "an other kind",
                "holds a real value",
                patched("J08", None, "/facts/environment/0/kind", json!("other")),
            ),
            (
                "malformed start record",
                "the start record is malformed",
                patched("J08", None, "/facts/start_record/state", json!("malformed")),
            ),
            (
                "the F3 body holds a needle",
                "J08 reflection response",
                Box::new(|s| {
                    s.knobs().reflect_body = Some(format!("HTTP/1.1 200 OK\r\n\r\n{ACCESS}"));
                }),
            ),
            (
                "the F3 check cannot run",
                "the reflection check cannot run",
                Box::new(|s| s.knobs().reflect_never_done = true),
            ),
            (
                "the F3 command is not a Codex descendant",
                "is not a descendant of Codex",
                Box::new(|s| s.knobs().reflect_not_descendant = true),
            ),
            (
                "the F3 request got no HTTP answer",
                "got no HTTP answer (curl exit 7)",
                Box::new(|s| s.knobs().reflect_done_code = 7),
            ),
            (
                "auth.json holds a real id token",
                "auth.json id_token is not a placeholder",
                Box::new(|s| s.knobs().auth_id_token = Some(ID_TOKEN.to_owned())),
            ),
            (
                "auth.json holds an alg-none token with another signature",
                "auth.json id_token is not a placeholder",
                Box::new(|s| {
                    s.knobs().auth_id_token = Some(
                        CODEX_PLACEHOLDER_ID_TOKEN.replace(".placeholder", ".other-signature"),
                    );
                }),
            ),
            (
                "sandbox get is unreadable",
                "sandbox get -o json is unreadable",
                Box::new(|s| s.host_rule("sandbox get", None, 1, "no")),
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn j08_create_request_faults() {
    table(
        "J08",
        vec![
            (
                "the create request holds a real credential",
                "create-request variable CODEX_AUTH_ACCESS_TOKEN holds a real value",
                Box::new(|s| {
                    s.knobs().create_env_extra = Some((
                        "CODEX_AUTH_ACCESS_TOKEN".to_owned(),
                        "plain-value-without-a-needle".to_owned(),
                    ));
                }),
            ),
            (
                "a needle in the create request",
                "needle found in J08 create environment",
                Box::new(|s| {
                    s.knobs().create_env_extra = Some(("NOTE".to_owned(), ACCESS.to_owned()));
                }),
            ),
            (
                "the launch id of the create request differs",
                "JACKIN_LAUNCH_ID of the create request differs",
                Box::new(|s| {
                    s.knobs().create_env_extra =
                        Some(("JACKIN_LAUNCH_ID".to_owned(), "another".to_owned()));
                }),
            ),
            (
                "the create request carries no launch id",
                "the create request carries no JACKIN_LAUNCH_ID",
                Box::new(|s| s.knobs().create_env_drop_launch = true),
            ),
            (
                "a CODEX_AUTH_ name without a credential word",
                "create-request variable CODEX_AUTH_ACCOUNT holds a real value",
                Box::new(|s| {
                    s.knobs().create_env_extra =
                        Some(("CODEX_AUTH_ACCOUNT".to_owned(), "plain".to_owned()));
                }),
            ),
            (
                "a TOKEN name",
                "create-request variable SERVICE_TOKEN holds a real value",
                Box::new(|s| {
                    s.knobs().create_env_extra =
                        Some(("SERVICE_TOKEN".to_owned(), "plain".to_owned()));
                }),
            ),
            (
                "a KEY name",
                "create-request variable SERVICE_KEY holds a real value",
                Box::new(|s| {
                    s.knobs().create_env_extra =
                        Some(("SERVICE_KEY".to_owned(), "plain".to_owned()));
                }),
            ),
            (
                "a SECRET name",
                "create-request variable SERVICE_SECRET holds a real value",
                Box::new(|s| {
                    s.knobs().create_env_extra =
                        Some(("SERVICE_SECRET".to_owned(), "plain".to_owned()));
                }),
            ),
            (
                "a PASSWORD name",
                "create-request variable SERVICE_PASSWORD holds a real value",
                Box::new(|s| {
                    s.knobs().create_env_extra =
                        Some(("SERVICE_PASSWORD".to_owned(), "plain".to_owned()));
                }),
            ),
            (
                "a placeholder only inside the value",
                "create-request variable CODEX_AUTH_ACCESS_TOKEN holds a real value",
                Box::new(|s| {
                    s.knobs().create_env_extra = Some((
                        "CODEX_AUTH_ACCESS_TOKEN".to_owned(),
                        "x-openshell:resolve:y".to_owned(),
                    ));
                }),
            ),
            (
                "the create request is unreadable",
                "create-request environment is unreadable",
                Box::new(|s| s.knobs().create_env_unreadable = true),
            ),
        ],
    )
    .await;
    for (name, value) in [
        ("NOTE", "plain"),
        (
            "CODEX_AUTH_ACCESS_TOKEN",
            "openshell:resolve:env:CODEX_AUTH_ACCESS_TOKEN",
        ),
    ] {
        let control = Scenario::new().await;
        control.knobs().create_env_extra = Some((name.to_owned(), value.to_owned()));
        let ran = control.run().await;
        ran.expect(&format!("{name} is not a leak"), &[]);
    }
    let quiet = Scenario::new().await;
    quiet.knobs().create_env_extra = Some((
        "CODEX_AUTH_ACCESS_TOKEN".to_owned(),
        "plain-value-without-a-needle".to_owned(),
    ));
    let ran = quiet.run().await;
    assert!(
        !ran.stdout.contains("plain-value-without-a-needle")
            && !ran.stderr.contains("plain-value-without-a-needle"),
        "J08 names a variable and never prints its value: {}",
        ran.stdout
    );
}

#[tokio::test]
async fn j09_egress_faults() {
    table(
        "J09",
        vec![
            (
                "outside host answers",
                "outside host answered HTTP 200",
                patched("J09", None, "/facts/outside_host/http_code", json!(200)),
            ),
            (
                "metadata host answers",
                "metadata host answered HTTP 200",
                patched("J09", None, "/facts/metadata_host/http_code", json!(200)),
            ),
            (
                "outside host fails to resolve",
                "outside host did not resolve (curl exit 6)",
                patched("J09", None, "/facts/outside_host/curl_exit", json!(6)),
            ),
            (
                "metadata host fails to resolve",
                "metadata host did not resolve (curl exit 6)",
                patched("J09", None, "/facts/metadata_host/curl_exit", json!(6)),
            ),
            (
                "outside host times out",
                "outside host gave curl exit 28",
                patched("J09", None, "/facts/outside_host/curl_exit", json!(28)),
            ),
            (
                "curl is missing for the outside host",
                "outside host was not tested",
                patched("J09", None, "/facts/outside_host", curl_missing()),
            ),
            (
                "curl is missing for the metadata host",
                "metadata host was not tested",
                patched("J09", None, "/facts/metadata_host", curl_missing()),
            ),
            (
                "curl is missing for the provider host",
                "provider host was not tested",
                patched("J09", None, "/facts/provider_host", curl_missing()),
            ),
            (
                "provider host answers 204",
                "answered HTTP 204",
                patched("J09", None, "/facts/provider_host/http_code", json!(204)),
            ),
            (
                "no provider host result",
                "the probe reported no provider host result",
                patched("J09", None, "/facts/provider_host", Value::Null),
            ),
            (
                "chatgpt.com rule accepted",
                "accepted a rule for chatgpt.com",
                Box::new(|s| s.host_rule("policy-plain", None, 0, "")),
            ),
            (
                "wildcard rule accepted",
                "accepted a rule for *.chatgpt.com",
                Box::new(|s| s.host_rule("policy-wild", None, 0, "")),
            ),
            (
                "control rejected",
                "rejected the control role",
                Box::new(|s| {
                    let needle = canonical_role(s);
                    s.host_rule(&needle, Some(2), 1, "broken");
                }),
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn j10_filesystem_faults() {
    table(
        "J10",
        vec![
            (
                "a denied write was created",
                "succeeded",
                patched("J10", None, "/facts/writes/0/created", json!(true)),
            ),
            (
                "host exec creates the denied file",
                "a write to /etc/e2e-denied succeeded",
                Box::new(|s| s.exec_rule(": > /etc/e2e-denied", 0, "")),
            ),
            (
                "no denied path was dac writable",
                "no denied path was writable by file mode",
                patched("J10", None, "/facts/writes/0/dac_writable", json!(false)),
            ),
            (
                "both sides agree on the wrong value",
                "a write to /etc/e2e-denied succeeded",
                Box::new(|s| {
                    s.patch("J10", None, "/facts/writes/0/created", json!(true));
                    s.exec_rule(": > /etc/e2e-denied", 0, "");
                }),
            ),
        ],
    )
    .await;
}

fn pty(fault: PtyFault) -> Tweak {
    Box::new(move |s: &Scenario| s.knobs().pty_fault = Some(fault.clone()))
}

#[tokio::test]
async fn j11_detach_faults() {
    let plain = |header: Option<&str>, exit: Option<i32>, never: bool| PtyFault {
        from: 4,
        header: header.map(str::to_owned),
        exit,
        never_exits: never,
    };
    table(
        "J11",
        vec![
            (
                "the pid changed",
                "the Codex pid changed",
                patched("J11", Some(2), "/facts/processes/codex/0/pid", json!(5555)),
            ),
            (
                "pid reuse",
                "is reused",
                patched(
                    "J11",
                    Some(2),
                    "/facts/processes/codex/0/start_ticks",
                    json!(1),
                ),
            ),
            (
                "no header in the replay",
                "did not replay the Codex header",
                pty(plain(Some("nothing here\r\n"), None, false)),
            ),
            (
                "attach exits 1",
                "attach 1 exited 1",
                pty(plain(None, Some(1), false)),
            ),
            (
                "attach does not exit",
                "did not exit after Ctrl-D",
                pty(plain(None, None, true)),
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn j12_shell_faults() {
    table(
        "J12",
        vec![
            (
                "the probe descends from main",
                "descends from the main process",
                patched(
                    "J12",
                    Some(2),
                    "/facts/processes/self_descends_from_main",
                    json!(true),
                ),
            ),
            (
                "same terminal as Codex",
                "shares the terminal",
                patched("J12", Some(2), "/facts/processes/self/tty", json!("pts/0")),
            ),
            (
                "Codex pid changed during the shell",
                "changed during the shell",
                patched("J12", Some(2), "/facts/processes/codex/0/pid", json!(7777)),
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn j13_export_faults() {
    let export =
        |mode: ExportMode| -> Tweak { Box::new(move |s: &Scenario| s.knobs().export_mode = mode) };
    table(
        "J13",
        vec![
            (
                "no ref is created",
                "does not exist on the host",
                export(ExportMode::NoRef),
            ),
            (
                "the ref is created but not listed",
                "did not list",
                export(ExportMode::Silent),
            ),
            (
                "the ref is at the wrong commit",
                "but the sandbox HEAD is",
                patched("J13", None, "/facts/head", json!("ab".repeat(20))),
            ),
            (
                "a host branch tip moved",
                "a host branch tip moved",
                export(ExportMode::MoveTip),
            ),
            (
                "a host file changed",
                "a file of the host working tree changed",
                export(ExportMode::ChangeFile),
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn j14_lifecycle_faults() {
    table(
        "J14",
        vec![
            (
                "same boot id",
                "boot id is the same",
                Box::new(|s| s.knobs().boot_unchanged = true),
            ),
            (
                "the marker id changed",
                "marker id changed",
                patched("J14", Some(2), "/facts/marker/id", json!("other")),
            ),
            (
                "the marker was created again",
                "marker was not created once and kept",
                patched("J14", Some(2), "/facts/marker/created", json!(true)),
            ),
            (
                "the marker was never created",
                "marker was not created once and kept",
                patched("J14", Some(1), "/facts/marker/created", json!(false)),
            ),
            (
                "the sandbox vanishes after a refused purge",
                "vanished after a refused purge",
                Box::new(|s| s.knobs().purge_vanishes = true),
            ),
            (
                "the written file is missing",
                "missing after the start",
                Box::new(|s| s.knobs().file_lost = true),
            ),
            (
                "purge without force exits 0",
                "purge without --force ended with exit 0",
                Box::new(|s| s.knobs().purge_unguarded = true),
            ),
            (
                "the sandbox survives purge --force",
                "still listed after purge --force",
                Box::new(|s| s.knobs().force_leaves_sandbox = true),
            ),
            (
                "the first sandbox phase changed",
                "the first sandbox changed",
                Box::new(|s| s.knobs().stop_changes_s = true),
            ),
            (
                "the first sandbox Codex pid changed",
                "the first sandbox changed",
                Box::new(|s| s.knobs().start_moves_s_codex = true),
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn j15_docker_capability_faults() {
    table(
        "J15",
        vec![
            (
                "engine id equals the host engine",
                "equals the host default engine id",
                Box::new(|s| s.knobs().same_engine_id = true),
            ),
            (
                "the container is in the default engine",
                "exists in the host default engine",
                Box::new(|s| s.knobs().container_in_default_engine = true),
            ),
            (
                "the host engine cannot list its containers",
                "cannot list the containers of the host engine",
                Box::new(|s| s.knobs().host_engine_unlistable = true),
            ),
            (
                "the docker label is not the engine label",
                "expected engine-2376-<16 hex digits of the engine CA>",
                Box::new(|s| s.knobs().docker_label_off = true),
            ),
            (
                "no warning text",
                "lacks the Docker capability warning",
                Box::new(|s| s.knobs().warning_missing = true),
            ),
            (
                "a DOCKER variable is absent",
                "DOCKER_TLS_VERIFY is not set",
                patched("J15", None, "/facts/environment/tls_verify", Value::Null),
            ),
            (
                "key mode 0644",
                "key.pem has mode 644",
                patched("J15", None, "/facts/certificates/2/mode", json!("644")),
            ),
            (
                "docker build fails",
                "docker build failed with exit 1",
                Box::new(|s| s.knobs().shell_exit.push(("docker build".to_owned(), 1))),
            ),
            (
                "docker run -i fails",
                "docker run -i failed with exit 1",
                Box::new(|s| s.knobs().shell_exit.push(("docker run -i".to_owned(), 1))),
            ),
            (
                "docker exec fails",
                "docker exec failed with exit 1",
                Box::new(|s| s.knobs().shell_exit.push(("docker exec".to_owned(), 1))),
            ),
            (
                "docker compose fails",
                "docker compose failed with exit 1",
                Box::new(|s| {
                    s.knobs()
                        .shell_exit
                        .push(("docker compose version".to_owned(), 1));
                }),
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn j16_engine_name_refused_while_the_resolver_works_passes() {
    let scenario = Scenario::new().await;
    scenario.patch("J16", None, "/facts/connection/curl_exit", json!(6));
    let ran = scenario.run().await;
    ran.expect("engine name refused, control refused", &[]);
}

#[tokio::test]
async fn j16_docker_capability_off_faults() {
    table(
        "J16",
        vec![
            (
                "the engine gives curl exit 0 and no HTTP code",
                "gave curl exit 0",
                patched("J16", None, "/facts/connection/curl_exit", json!(0)),
            ),
            (
                "the engine answers HTTP 200",
                "answered HTTP 200",
                Box::new(|s| {
                    s.patch("J16", None, "/facts/connection/curl_exit", json!(0));
                    s.patch("J16", None, "/facts/connection/http_code", json!(200));
                }),
            ),
            (
                "the engine connection times out",
                "gave curl exit 28",
                patched("J16", None, "/facts/connection/curl_exit", json!(28)),
            ),
            (
                "the engine name fails to resolve and the control fails too",
                "resolver fault",
                Box::new(|s| {
                    s.patch("J16", None, "/facts/connection/curl_exit", json!(6));
                    s.patch("J16", None, "/facts/resolver_control/curl_exit", json!(6));
                }),
            ),
            (
                "the engine name fails to resolve and no control ran",
                "control name example.com was not refused",
                Box::new(|s| {
                    s.patch("J16", None, "/facts/connection/curl_exit", json!(6));
                    s.patch("J16", None, "/facts/resolver_control", Value::Null);
                }),
            ),
            (
                "curl is missing for the engine",
                "engine endpoint was not tested",
                Box::new(|s| {
                    s.patch(
                        "J16",
                        None,
                        "/facts/connection",
                        json!({"attempted": true, "curl_found": false, "curl_exit": null, "http_code": 0}),
                    );
                    s.patch("J16", None, "/facts/resolver_control", curl_missing());
                }),
            ),
            (
                "a docker binary exists",
                "a docker binary exists",
                patched("J16", None, "/facts/docker_binary", json!(true)),
            ),
            (
                "a DOCKER variable is set",
                "DOCKER_ variables are set",
                patched(
                    "J16",
                    None,
                    "/facts/docker_variables",
                    json!(["DOCKER_HOST"]),
                ),
            ),
        ],
    )
    .await;
    let _ = Duration::ZERO;
}

#[tokio::test]
async fn j08_reflection_survives_an_enter_that_a_busy_codex_swallows() {
    let scenario = Scenario::new().await;
    scenario.knobs().reflect_swallow_first_enter = true;
    let ran = scenario.run().await;
    ran.expect(
        "the first Enter is lost, the second one runs the command",
        &[],
    );
}

#[tokio::test]
async fn the_shared_probe_waits_until_codex_has_settled() {
    let scenario = Scenario::new().await;
    scenario.knobs().codex_young_polls = 3;
    let ran = scenario.run().await;
    ran.expect(
        "a probe that started a second Codex too early would crash the first",
        &[],
    );
}
