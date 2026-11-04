// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{CODEX_PLACEHOLDER_ID_TOKEN, EnvName, FileMode, SandboxPath, SecretBytes};
use serde_json::Value;

use crate::e2e::probe::FactsJ08;
use crate::e2e::run::{Fail, Res, Run, fail, tail_of, text_of};
use crate::e2e::verdict::Verdict;

pub(crate) const PLACEHOLDER: &str = "openshell:resolve:";
const LEAK_LISTED: usize = 5;
const REFLECT_SCRIPT: &str = "/sandbox/.e2e-reflect.sh";
const REFLECT_OUT: &str = "/sandbox/.e2e-reflect.out";
const REFLECT_CHAIN: &str = "/sandbox/.e2e-reflect.chain";
const REFLECT_DONE: &str = "E2E-REFLECT-DONE ";
const ENTER_ATTEMPTS: u32 = 3;

fn reflect_script(placeholder: &str) -> String {
    format!(
        "#!/bin/sh\n\
         P='{placeholder}'\n\
         chain=''\n\
         pid=$$\n\
         while [ \"$pid\" -gt 1 ]; do\n\
         chain=\"$chain $(cat /proc/$pid/comm)\"\n\
         pid=$(sed 's/.*) //' /proc/$pid/stat | cut -d' ' -f2)\n\
         done\n\
         echo \"$chain\" > {REFLECT_CHAIN}\n\
         curl -sS -i -m 25 \"https://chatgpt.com/backend-api/e2e-reflect/$P?token=$P\" \
         -H \"X-E2E-Reflect: $P\" -H \"Authorization: Bearer $P\" -o {REFLECT_OUT} 2>/dev/null\n\
         echo \"{REFLECT_DONE}$?\"\n"
    )
}

fn credential_name(name: &str) -> bool {
    name.starts_with("CODEX_AUTH_")
        || ["TOKEN", "KEY", "SECRET", "PASSWORD"]
            .iter()
            .any(|word| name.contains(word))
}

pub(crate) fn judge_create_env(
    environment: &BTreeMap<EnvName, String>,
    launch: &str,
) -> Vec<String> {
    let mut problems = Vec::new();
    match environment
        .iter()
        .find(|(name, _)| name.as_str() == "JACKIN_LAUNCH_ID")
    {
        Some((_, value)) if value == launch => {}
        Some(_) => problems
            .push("JACKIN_LAUNCH_ID of the create request differs from the launch".to_owned()),
        None => problems.push("the create request carries no JACKIN_LAUNCH_ID".to_owned()),
    }
    for (name, value) in environment {
        if credential_name(name.as_str()) && !value.is_empty() && !value.starts_with(PLACEHOLDER) {
            problems.push(format!(
                "create-request variable {name} holds a real value (kind other)"
            ));
        }
    }
    problems
}

pub(crate) fn judge_kinds(facts: &FactsJ08) -> (Vec<String>, Option<String>) {
    let mut problems = Vec::new();
    let mut skip = None;
    for var in &facts.environment {
        if var.kind == "other" {
            problems.push(format!(
                "credential variable {} holds a real value (kind other)",
                var.name
            ));
        }
    }
    match facts.start_record.state.as_str() {
        "read" => {}
        state @ ("absent" | "unreadable" | "stale") => {
            skip = Some(format!(
                "the start record is {state}, so the environments are unknown"
            ));
        }
        state => problems.push(format!("the start record is {state}")),
    }
    if let Some(auth) = &facts.auth_file {
        for (name, kind) in [
            ("access_token", &auth.access_token),
            ("refresh_token", &auth.refresh_token),
            ("id_token", &auth.id_token),
        ] {
            if kind == "other" {
                problems.push(format!("auth.json {name} is a real token"));
            }
        }
    }
    if !facts.leaks.is_empty() {
        problems.push(format!(
            "the probe found token-shaped strings in {} files",
            facts.leaks.len()
        ));
    }
    (problems, skip)
}

pub(crate) fn judge_auth_text(bytes: &[u8]) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return vec!["auth.json in the sandbox is not JSON".to_owned()];
    };
    let mut problems = Vec::new();
    for name in ["access_token", "refresh_token", "id_token"] {
        let token = value
            .pointer(&format!("/tokens/{name}"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let placeholder = token.starts_with(PLACEHOLDER)
            || (name == "id_token" && token == CODEX_PLACEHOLDER_ID_TOKEN);
        if !token.is_empty() && !placeholder {
            problems.push(format!("auth.json {name} is not a placeholder"));
        }
    }
    problems
}

pub(crate) fn judge_env_text(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, value)| {
            name.starts_with("CODEX_AUTH_") && !value.is_empty() && !value.starts_with(PLACEHOLDER)
        })
        .map(|(name, _)| format!("environment variable {name} is not a placeholder"))
        .collect()
}

impl Run<'_> {
    pub(crate) async fn j08(&mut self) -> Verdict {
        self.j08_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn read_sandbox_files(&mut self) -> Res<Vec<String>> {
        let sandbox = self.sandbox_s()?;
        let launch = self.w.l1.clone().ok_or_else(|| fail("no launch id"))?;
        let paths = [
            format!("/sandbox/.jackin/env-{launch}"),
            "/sandbox/.codex/auth.json".to_owned(),
            "/sandbox/.jackin/run/env-kinds".to_owned(),
        ];
        let parsed: Vec<SandboxPath> = paths
            .iter()
            .map(|path| SandboxPath::parse(path))
            .collect::<Result<_, _>>()?;
        let contents = self.b.gateway.read_files(&sandbox, &parsed).await?;
        let mut problems = Vec::new();
        for (label, content) in ["env file", "auth.json", "env-kinds"].iter().zip(contents) {
            let Some(bytes) = content else { continue };
            self.capture(&format!("J08 {label}"), &bytes);
            if *label == "auth.json" {
                problems.extend(judge_auth_text(&bytes));
            }
        }
        Ok(problems)
    }

    async fn j08_inner(&mut self) -> Res<Verdict> {
        let sandbox = self.sandbox_s()?;
        let report = self.shared("J08")?.clone();
        let facts: FactsJ08 = report.facts("J08")?;
        let mut problems: Vec<String> = report.gate("J08").into_iter().collect();
        let (kind_problems, skip) = judge_kinds(&facts);
        problems.extend(kind_problems);
        problems.extend(self.read_sandbox_files().await?);
        problems.extend(self.create_environment(&sandbox).await);
        drop(self.inspect(&sandbox, "J08 inspect").await?);
        let record = self
            .openshell(
                "J08 sandbox get",
                &["sandbox", "get", sandbox.as_str(), "-o", "json"],
            )
            .await;
        match record {
            Ok(output)
                if output.ok() && serde_json::from_slice::<Value>(&output.stdout).is_ok() => {}
            Ok(output) => problems.push(format!(
                "openshell sandbox get -o json is unreadable: {} {}",
                output.exit.describe(),
                tail_of(&self.redactor, &output.stderr)
            )),
            Err(failure) => problems.push(format!(
                "openshell sandbox get -o json is unreadable: {}",
                failure.0
            )),
        }
        let env = self.exec(&sandbox, "J08 env", "/usr/bin/env").await?;
        problems.extend(judge_env_text(&text_of(&env.stdout)));
        let placeholder = text_of(&env.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("CODEX_AUTH_ACCESS_TOKEN="))
            .filter(|value| value.starts_with(PLACEHOLDER))
            .map(str::to_owned);
        problems.extend(self.reflection(&sandbox, placeholder).await);
        if !problems.is_empty() {
            Ok(Verdict::fail(problems.join("; ")))
        } else if let Some(reason) = skip {
            Ok(Verdict::skip(reason))
        } else {
            Ok(Verdict::pass(
                "placeholders only, no key in files or env, no reflection",
            ))
        }
    }

    async fn create_environment(&mut self, sandbox: &jackin_core::SandboxName) -> Vec<String> {
        let Some(launch) = self.w.l1.clone() else {
            return vec!["no launch id".to_owned()];
        };
        match self.b.gateway.sandbox_environment(sandbox).await {
            Ok(environment) => {
                let joined: Vec<u8> = environment
                    .values()
                    .flat_map(|value| value.bytes().chain(std::iter::once(b'\n')))
                    .collect();
                self.scan("J08 create environment", &joined);
                judge_create_env(&environment, launch.as_str())
            }
            Err(error) => vec![format!(
                "the create-request environment is unreadable: {error}"
            )],
        }
    }

    async fn reflection(
        &mut self,
        sandbox: &jackin_core::SandboxName,
        placeholder: Option<String>,
    ) -> Vec<String> {
        let cannot = |why: String| vec![format!("the reflection check cannot run: {why}")];
        let Some(placeholder) = placeholder else {
            return cannot("the sandbox environment holds no placeholder to send".to_owned());
        };
        match self.reflect_through_codex(sandbox, &placeholder).await {
            Ok(problems) => problems,
            Err(failure) => cannot(failure.0),
        }
    }

    async fn reflect_through_codex(
        &mut self,
        sandbox: &jackin_core::SandboxName,
        placeholder: &str,
    ) -> Res<Vec<String>> {
        let script = SandboxPath::parse(REFLECT_SCRIPT)?;
        self.exec(
            sandbox,
            "J08 reflect clean",
            "rm -f /sandbox/.e2e-reflect.*",
        )
        .await?;
        self.b
            .gateway
            .write_file(
                sandbox,
                &jackin_openshell::SandboxFile {
                    path: script,
                    mode: FileMode(0o755),
                    content: SecretBytes::new(reflect_script(placeholder).into_bytes()),
                },
            )
            .await?;
        let main = self.projects.main.clone();
        let mut session = self.open_pty(&["attach", sandbox.as_str()], &main)?;
        let typed = self.type_into_codex(&mut session).await;
        let detached = self.detach(&mut session).await;
        self.finish_pty("J08 reflection", &session);
        let code = typed?;
        detached?;
        let paths = [REFLECT_CHAIN, REFLECT_OUT]
            .map(SandboxPath::parse)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let files = self.b.gateway.read_files(sandbox, &paths).await?;
        let chain = files[0]
            .clone()
            .map(|bytes| text_of(&bytes))
            .unwrap_or_default();
        let body = files[1].clone().unwrap_or_default();
        self.capture("J08 reflection response", &body);
        let mut problems = Vec::new();
        if !chain.split_whitespace().any(|name| name == "codex") {
            problems.push(format!(
                "the reflection command is not a descendant of Codex (ancestors:{chain})"
            ));
        }
        if code != 0 || !body.starts_with(b"HTTP/") {
            problems.push(format!(
                "the reflection request got no HTTP answer (curl exit {code})"
            ));
        }
        Ok(problems)
    }

    async fn type_into_codex(&self, session: &mut crate::e2e::pty::PtySession) -> Res<i32> {
        self.expect_codex(session).await?;
        let gap = self.b.budgets.key_gap;
        tokio::time::sleep(gap).await;
        session.send(b"!")?;
        tokio::time::sleep(gap).await;
        session.send(format!("sh {REFLECT_SCRIPT}").as_bytes())?;
        tokio::time::sleep(gap).await;
        let wait = self.b.budgets.expect / ENTER_ATTEMPTS;
        let mut last = fail("the command was never submitted");
        for _ in 0..ENTER_ATTEMPTS {
            session.send(b"\r")?;
            let found = session
                .expect_with("the reflection result", wait, |text| {
                    let start = text.find(REFLECT_DONE)?;
                    let rest = &text[start + REFLECT_DONE.len()..];
                    let digits = rest.chars().take_while(char::is_ascii_digit).count();
                    (digits > 0).then_some((start, start + REFLECT_DONE.len() + digits))
                })
                .await;
            match found {
                Ok((_, matched)) => {
                    return matched
                        .trim_start_matches(REFLECT_DONE)
                        .parse()
                        .map_err(|_| fail("unreadable reflection result"));
                }
                Err(error) => last = error.into(),
            }
        }
        Err(last)
    }

    pub(crate) fn j08_late(&self, held: Verdict) -> Verdict {
        if self.leaks.is_empty() {
            return held;
        }
        let shown: Vec<&str> = self
            .leaks
            .iter()
            .take(LEAK_LISTED)
            .map(String::as_str)
            .collect();
        let mut reason = shown.join("; ");
        if held.status() == crate::e2e::verdict::Status::Fail {
            reason = format!("{}; {reason}", held.reason());
        }
        Verdict::fail(reason)
    }
}
