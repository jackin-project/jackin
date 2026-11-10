// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::SandboxName;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::gateway::ScenarioGateway;
use super::{J05Fault, SandboxModel};
use crate::support::{capture, hex};

pub(crate) fn head_of(dir: &Path) -> String {
    let output = capture(
        jackin_testkit::git_command()
            .args(["rev-parse", "HEAD"])
            .current_dir(dir),
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn proc(pid: u32, ppid: u32, comm: &str, ticks: u64, tty: Option<&str>) -> Value {
    json!({"pid": pid, "ppid": ppid, "sid": pid, "comm": comm, "start_ticks": ticks, "tty": tty})
}

fn processes(model: &SandboxModel, own_tty: &str, descends: bool) -> Value {
    let pid = model.codex_pid.max(4242);
    json!({
        "boot_id": format!("boot-{}", model.boot),
        "run_state": "running",
        "main": proc(100, 1, "jackin-start", 1000, None),
        "codex": [proc(pid, 100, "codex", u64::from(pid) * 10, Some("pts/0"))],
        "self": proc(9000, 8000, "jackin-sentinel-probe", 90000, Some(own_tty)),
        "self_descends_from_main": descends,
    })
}

fn exec_ok() -> Value {
    json!({"attempted": true, "credential": "chatgpt_login", "exit_code": 0, "answered": true, "output_bytes": 5})
}

fn egress(code: u32, exit: u32) -> Value {
    json!({"url": "https://example.invalid/", "curl_found": true, "curl_exit": exit, "http_code": code})
}

fn project_files(dir: &Path) -> Vec<Value> {
    let listing = capture(
        jackin_testkit::git_command()
            .args(["ls-files", "-co", "--exclude-standard", "-z"])
            .current_dir(dir),
    );
    let mut files: Vec<Value> = listing
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
        .map(|raw| {
            let path = String::from_utf8_lossy(raw).into_owned();
            let bytes = std::fs::read(dir.join(&path)).expect("file");
            json!({"path": path, "sha256": hex(&Sha256::digest(bytes))})
        })
        .collect();
    files.sort_by_key(|value| value["path"].as_str().unwrap_or("").to_owned());
    files
}

fn facts(
    gateway: &ScenarioGateway,
    step: &str,
    model: &SandboxModel,
    call: u32,
    docker: bool,
    engine: Option<&str>,
) -> Value {
    let hooks = json!([
        {"name": "setup", "path": "/usr/local/share/jackin/hooks/setup", "uid": 0, "mode": "755", "writable": false},
        {"name": "preflight", "path": "/usr/local/share/jackin/hooks/preflight", "uid": 0, "mode": "755", "writable": false},
    ]);
    match step {
        "J00" => {
            json!({"codex": {"path": "/usr/local/bin/codex", "present": true, "executable": true, "version": "codex-cli 0.167.0"}})
        }
        "J04" => {
            json!({"user": "sandbox", "uid": 1000, "gid": 1000, "home": "/sandbox", "sudo": {"found": false, "works": false}, "docker_binary": docker})
        }
        "J05" => {
            let path = gateway
                .shared
                .state()
                .workspaces
                .get(&model.workspace)
                .map(|(path, _)| path.clone())
                .unwrap_or_default();
            let mut files = project_files(&path);
            let fault = gateway.shared.knobs().j05_fault;
            let zero = "0".repeat(64);
            match fault {
                Some(J05Fault::DropFile) => {
                    files.remove(0);
                }
                Some(J05Fault::ExtraFile) => {
                    files.push(json!({"path": "extra.txt", "sha256": zero}));
                }
                Some(J05Fault::BadDigest) => files[0]["sha256"] = json!("f".repeat(64)),
                Some(J05Fault::IncludeIgnored) => {
                    files.push(json!({"path": "ignored.log", "sha256": zero}));
                }
                Some(J05Fault::NewlineName) => {
                    files.push(json!({"path": "a\nb.txt", "sha256": zero}));
                }
                None => {}
            }
            json!({"project": "/sandbox/project", "git_work_tree": true, "files": files})
        }
        "J06" => json!({
            "state_dir": "/sandbox/.sentinel",
            "setup_runs": model.setup_runs,
            "preflight_runs": model.preflight_runs,
            "preflight_failures": model.preflight_failures,
            "hooks": hooks,
        }),
        "J07" => json!({"processes": processes(model, "pts/1", false), "codex_exec": exec_ok()}),
        "J08" => json!({
            "environment": [{"name": "ZAI_API_KEY", "kind": "placeholder", "source": "launch_env"}],
            "agent_environment": {"main": "record", "codex": "record"},
            "start_record": {"state": "read", "attempt": 1},
            "auth_file": null,
            "leaks": [],
        }),
        "J09" => json!({
            "outside_host": egress(0, 7),
            "metadata_host": egress(0, 7),
            "provider_host": egress(401, 0),
            "codex_exec": exec_ok(),
        }),
        "J10" => json!({"writes": [
            {"path": "/etc/e2e-probe", "expected": "denied", "dac_writable": true, "created": false},
            {"path": "/tmp/e2e-probe", "expected": "allowed", "dac_writable": true, "created": true},
        ]}),
        "J11" => json!({"processes": processes(model, "pts/1", false)}),
        "J12" => {
            json!({"processes": processes(model, if call == 2 { "pts/2" } else { "pts/1" }, false)})
        }
        "J13" => {
            let path = gateway
                .shared
                .state()
                .workspaces
                .get(&model.workspace)
                .map(|(path, _)| path.clone())
                .unwrap_or_default();
            let head = head_of(&path);
            json!({
                "project": "/sandbox/project", "work_tree": true, "branch": "main", "head": head,
                "branches": [{"name": "main", "tip": head}], "clean": true, "stash": 0,
            })
        }
        "J14" => json!({
            "boot_id": format!("boot-{}", model.boot),
            "marker": {"path": "/sandbox/.sentinel/marker", "id": format!("marker-{}", model.launch), "created": call == 1},
            "setup_runs": 1, "preflight_runs": 1,
        }),
        "J15" => json!({
            "docker_binary": {"found": true, "path": "/usr/local/bin/docker"},
            "environment": {"docker_host": "tcp://host.openshell.internal:2376", "tls_verify": "1", "cert_path": "/sandbox/.docker-engine"},
            "certificates": [
                {"name": "ca.pem", "mode": "644"}, {"name": "cert.pem", "mode": "644"}, {"name": "key.pem", "mode": "600"},
            ],
            "run": {"exit_code": 0, "hello": true},
            "engine": {"id": "ENGINE-D24", "server_version": "29.9.2"},
        }),
        "J16" => json!({
            "docker_binary": false, "docker_variables": [], "endpoint": engine,
            "connection": {"attempted": true, "curl_found": true, "curl_exit": 7, "http_code": 0},
            "resolver_control": egress(0, 7),
        }),
        other => panic!("no scenario facts for {other}"),
    }
}

fn step_json(result: &str, facts: Value) -> Value {
    let reason = if result == "PASS" {
        ""
    } else {
        "scenario reason"
    };
    json!({
        "result": result,
        "reason": reason,
        "checks": [{"name": "scenario_check", "result": result, "detail": ""}],
        "facts": facts,
    })
}

pub(crate) fn build_report(
    gateway: &ScenarioGateway,
    sandbox: &SandboxName,
    model: &SandboxModel,
    args: &[String],
) -> Value {
    let value_of = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|at| args.get(at + 1))
            .cloned()
    };
    let docker = value_of("--docker").as_deref() == Some("on");
    let engine = value_of("--engine");
    let only = value_of("--only").unwrap_or_default();
    let mut steps = serde_json::Map::new();
    for step in only.split(',').filter(|step| !step.is_empty()) {
        let call = {
            let mut state = gateway.shared.state();
            let model = state
                .sandboxes
                .entry(sandbox.as_str().to_owned())
                .or_default();
            let counter = model.probe_calls.entry(step.to_owned()).or_insert(0);
            *counter += 1;
            *counter
        };
        steps.insert(
            step.to_owned(),
            step_json(
                "PASS",
                facts(gateway, step, model, call, docker, engine.as_deref()),
            ),
        );
    }
    let mut report = json!({
        "schema": 2, "probe": "jackin-sentinel-probe", "mode": "sandbox",
        "docker": if docker { "on" } else { "off" }, "steps": Value::Object(steps),
    });
    let knobs = gateway.shared.knobs().clone();
    for patch in &knobs.probes {
        let call = gateway
            .shared
            .state()
            .sandboxes
            .get(sandbox.as_str())
            .and_then(|found| found.probe_calls.get(&patch.step).copied());
        if report["steps"].get(&patch.step).is_none() {
            continue;
        }
        if patch.call.is_some() && patch.call != call {
            continue;
        }
        let full = format!("/steps/{}{}", patch.step, patch.pointer);
        if let Some(slot) = report.pointer_mut(&full) {
            *slot = patch.value.clone();
        } else if let Some((parent, key)) = full.rsplit_once('/')
            && let Some(Value::Object(map)) = report.pointer_mut(parent)
        {
            map.insert(key.to_owned(), patch.value.clone());
        }
    }
    if let Some(steps) = report["steps"].as_object_mut() {
        for dropped in &knobs.drop_steps {
            steps.remove(dropped);
        }
    }
    report
}

pub(crate) fn probe_output(
    gateway: &ScenarioGateway,
    sandbox: &SandboxName,
    model: &SandboxModel,
    args: &[String],
) -> String {
    build_report(gateway, sandbox, model, args).to_string()
}
