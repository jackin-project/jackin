// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::LaunchId;
use serde_json::Value;

use super::j12::json_of;
use crate::e2e::probe::{FactsJ04, FactsJ15, ProbeMode, ProbeReport, ProbeRequest};
use crate::e2e::run::{ENGINE_HOST, Fail, PROBE, Res, Run, fail};
use crate::e2e::verdict::Verdict;

pub(crate) const WARNING: &str = "warning: the Docker capability is enabled for this sandbox";
const RISKS: [&str; 4] = [
    "can read the engine client key",
    "bypass the sandbox network policy",
    "bind mount names a path on the engine host",
    "engine outlives the sandbox",
];
const CERTS: [&str; 3] = ["ca.pem", "cert.pem", "key.pem"];

#[derive(Debug)]
pub(crate) struct Shell {
    pub label: &'static str,
    pub code: i32,
    pub body: String,
}

pub(crate) struct DockerSeen<'a> {
    pub load_text: &'a str,
    pub shell: &'a [Shell],
    pub host_id: Option<&'a str>,
    pub host_names: &'a [String],
    pub container: &'a str,
}

pub(crate) fn blocker_present(repo_root: &std::path::Path) -> bool {
    repo_root.join("rewrite/blockers/P0.5-docker.md").is_file()
}

fn step<'a>(shell: &'a [Shell], label: &str) -> Option<&'a Shell> {
    shell.iter().find(|entry| entry.label == label)
}

pub(crate) fn judge_docker(
    identity: &FactsJ04,
    facts: &FactsJ15,
    seen: &DockerSeen<'_>,
) -> Vec<String> {
    let mut problems = Vec::new();
    if !identity.docker_binary {
        problems.push("the J04 probe with the capability on found no docker binary".to_owned());
    }
    if !facts.docker_binary.found {
        problems.push("no docker binary in the sandbox".to_owned());
    }
    let env = &facts.environment;
    for (name, value) in [
        ("DOCKER_HOST", &env.docker_host),
        ("DOCKER_TLS_VERIFY", &env.tls_verify),
        ("DOCKER_CERT_PATH", &env.cert_path),
    ] {
        if value.as_deref().is_none_or(str::is_empty) {
            problems.push(format!("{name} is not set in the shell"));
        }
    }
    for cert in CERTS {
        match facts.certificates.iter().find(|found| found.name == cert) {
            None => problems.push(format!("certificate {cert} is missing")),
            Some(found) if cert == "key.pem" && found.mode.trim_start_matches('0') != "600" => {
                problems.push(format!("key.pem has mode {}, not 0600", found.mode));
            }
            Some(_) => {}
        }
    }
    if facts.run.exit_code != Some(0) || !facts.run.hello {
        problems.push("docker run hello-world did not print its greeting".to_owned());
    }
    if !seen.load_text.contains(WARNING) {
        problems.push("the load output lacks the Docker capability warning".to_owned());
    }
    for risk in RISKS {
        if !seen.load_text.contains(risk) {
            problems.push(format!("the load output lacks the risk line `{risk}`"));
        }
    }
    for (label, needle) in [
        ("run", "Hello from Docker"),
        ("build", ""),
        ("run -i", "e2e-ping"),
        ("exec", "e2e-exec"),
        ("compose", ""),
    ] {
        match step(seen.shell, label) {
            Some(entry) if entry.code == 0 && entry.body.contains(needle) => {}
            Some(entry) => problems.push(format!("docker {label} failed with exit {}", entry.code)),
            None => problems.push(format!("docker {label} did not run")),
        }
    }
    let listed_inside = step(seen.shell, "ps")
        .is_some_and(|entry| entry.body.lines().any(|line| line.trim() == seen.container));
    if !listed_inside {
        problems.push(format!(
            "container {} is not in the chosen engine",
            seen.container
        ));
    }
    if seen.host_names.iter().any(|name| name == seen.container) {
        problems.push(format!(
            "container {} exists in the host default engine",
            seen.container
        ));
    }
    match (facts.engine.id.as_deref(), seen.host_id) {
        (Some(chosen), Some(host)) if chosen == host => {
            problems.push("the sandbox engine id equals the host default engine id".to_owned());
        }
        (None, _) => problems.push("the probe reported no engine id".to_owned()),
        (_, None) => problems.push("the host default engine id is unknown".to_owned()),
        _ => {}
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j15(&mut self) -> Verdict {
        if blocker_present(&self.b.repo_root) {
            return Verdict::skip("P0.5 BLOCKED: rewrite/blockers/P0.5-docker.md exists");
        }
        self.j15_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn shell_step(
        &mut self,
        session: &mut crate::e2e::pty::PtySession,
        label: &'static str,
        command: &str,
    ) -> Res<Shell> {
        let timeout = self.b.budgets.exec;
        let result = session.run_shell(command, timeout).await;
        self.finish_pty("J15 shell", session);
        let (body, code) = result?;
        Ok(Shell { label, code, body })
    }

    async fn j15_inner(&mut self) -> Res<Verdict> {
        let port = self.engine_port()?;
        let dock = self.projects.dock.clone();
        let launch = LaunchId::generate().map_err(|error| fail(error.to_string()))?;
        let project = dock.display().to_string();
        let loaded = self
            .jackin(
                "J15 load",
                &["load", "sentinel", &project, "--launch-id", launch.as_str()],
                &dock,
            )
            .await?;
        let load_text = format!("{}{}", loaded.stdout_text(), loaded.stderr_text());
        if !loaded.ok() {
            return Err(fail(format!(
                "loading the Docker sandbox exited {}: {}",
                loaded.exit.describe(),
                crate::e2e::run::tail_of(&self.redactor, &loaded.stderr)
            )));
        }
        let found = self.find_launch(&launch).await?;
        let d = found
            .first()
            .map(|view| view.name.clone())
            .ok_or_else(|| fail("the Docker load created no sandbox"))?;
        self.w.d = Some(d.clone());
        let container = self.names.container();
        self.w.container_created = true;
        let mut session = self.open_pty(&["shell", d.as_str()], &dock)?;
        let timeout = self.b.budgets.exec;
        let probe_line =
            format!("{PROBE} --docker on --engine {ENGINE_HOST}:{port} --only J04,J15");
        let probed = session.run_shell(&probe_line, timeout).await;
        self.finish_pty("J15 shell", &session);
        let (body, code) = probed?;
        if code != 0 {
            return Err(fail(format!("the probe in the shell exited {code}")));
        }
        let json = json_of(&body).ok_or_else(|| fail("the probe in the shell printed no JSON"))?;
        let value: Value = serde_json::from_str(json)?;
        let request = ProbeRequest {
            only: vec!["J04".to_owned(), "J15".to_owned()],
            docker: true,
            mode: ProbeMode::Sandbox,
        };
        let report = ProbeReport::parse(value.to_string().as_bytes(), &request, &self.redactor)?;
        let facts: FactsJ15 = report.facts("J15")?;
        let identity: FactsJ04 = report.facts("J04")?;
        if let (Some(host), Some(path)) =
            (&facts.environment.docker_host, &facts.environment.cert_path)
        {
            self.w.docker_env = Some((host.clone(), path.clone()));
        }
        let mut shell = Vec::new();
        let steps: [(&'static str, String); 6] = [
            ("run", format!("docker run --name {container} hello-world")),
            (
                "build",
                format!(
                    "rm -rf /tmp/e2e-ctx && mkdir -p /tmp/e2e-ctx && printf 'FROM hello-world\\nLABEL e2e=1\\n' > /tmp/e2e-ctx/Dockerfile && docker build -q -t {container}-img /tmp/e2e-ctx"
                ),
            ),
            (
                "run -i",
                "echo e2e-ping | docker run -i --rm busybox cat".to_owned(),
            ),
            (
                "exec",
                format!(
                    "docker run -d --name {container}-exec busybox sleep 120 && docker exec {container}-exec echo e2e-exec; code=$?; docker rm -f {container}-exec >/dev/null; exit $code"
                ),
            ),
            ("compose", "docker compose version".to_owned()),
            ("ps", "docker ps -a --format '{{.Names}}'".to_owned()),
        ];
        for (label, command) in steps {
            shell.push(self.shell_step(&mut session, label, &command).await?);
        }
        let host_id = self.b.engine.engine_id().await.ok();
        let host_names = self.b.engine.container_names().await.unwrap_or_default();
        let mut problems: Vec<String> = report
            .gate("J15")
            .into_iter()
            .chain(report.gate("J04"))
            .collect();
        problems.extend(judge_docker(
            &identity,
            &facts,
            &DockerSeen {
                load_text: &load_text,
                shell: &shell,
                host_id: host_id.as_deref(),
                host_names: &host_names,
                container: &container,
            },
        ));
        session.send(b"exit\n")?;
        drop(session.wait_exit(timeout).await);
        if problems.is_empty() {
            Ok(Verdict::pass(
                "docker run, build, run -i, exec and compose work; the container lives only in the chosen engine",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
