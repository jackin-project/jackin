// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{CODEX_PLACEHOLDER_ID_TOKEN, EnvName, FileMode, SandboxPath, SecretBytes};
use serde_json::Value;

use crate::e2e::adapter::{AuthFieldKind, ConfigFileBehavior, CredentialBehavior, EnvironmentKind};
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

fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn reflect_script(url: &str, placeholder: &str) -> String {
    let url = quoted(url);
    let placeholder = quoted(placeholder);
    format!(
        "#!/bin/sh\n\
         P={placeholder}\n\
         chain=''\n\
         pid=$$\n\
         while [ \"$pid\" -gt 1 ]; do\n\
         chain=\"$chain $(cat /proc/$pid/comm)\"\n\
         pid=$(sed 's/.*) //' /proc/$pid/stat | cut -d' ' -f2)\n\
         done\n\
         echo \"$chain\" > {REFLECT_CHAIN}\n\
         curl -sS -i -m 25 {url} -H \"X-E2E-Reflect: $P\" -H \"Authorization: Bearer $P\" -o {REFLECT_OUT} 2>/dev/null\n\
         echo \"{REFLECT_DONE}$?\"\n"
    )
}

fn credential_name(name: &str) -> bool {
    name.starts_with("CODEX_AUTH_")
        || ["TOKEN", "KEY", "SECRET", "PASSWORD"]
            .iter()
            .any(|word| name.contains(word))
}

fn brokered_identity(rest: &str, name: &str) -> bool {
    if rest == name {
        return true;
    }
    let Some(head) = rest
        .strip_suffix(name)
        .and_then(|head| head.strip_suffix('_'))
    else {
        return false;
    };
    if let Some(digits) = head.strip_prefix('v') {
        return !digits.is_empty()
            && digits.as_bytes()[0] != b'0'
            && digits.bytes().all(|byte| byte.is_ascii_digit());
    }
    if let Some(hex) = head.strip_prefix('s') {
        return hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    }
    false
}

fn brokered_value(value: &str, name: &str) -> bool {
    if value.is_empty()
        || value
            .chars()
            .any(|char| char.is_whitespace() || char.is_control())
    {
        return false;
    }
    if let Some(rest) = value.strip_prefix("openshell:resolve:env:") {
        return brokered_identity(rest, name);
    }
    const MARKER: &str = "OPENSHELL-RESOLVE-ENV-";
    if let Some(index) = value.find(MARKER) {
        let prefix = &value[..index];
        let valid_prefix = !prefix.is_empty()
            && prefix.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'~')
            });
        if valid_prefix {
            return brokered_identity(&value[index + MARKER.len()..], name);
        }
    }
    false
}

fn value_kind(value: &str) -> &str {
    if value.is_empty() {
        "empty"
    } else if value.starts_with(PLACEHOLDER) {
        "placeholder"
    } else {
        "other"
    }
}

pub(crate) fn judge_create_env(
    environment: &BTreeMap<EnvName, String>,
    launch: &str,
    behavior: &CredentialBehavior,
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
    let expected: BTreeMap<&str, &EnvironmentKind> = behavior
        .environment
        .iter()
        .map(|variable| (variable.name.as_str(), &variable.kind))
        .collect();
    for (name, value) in environment {
        let name = name.as_str();
        if behavior
            .absent_environment
            .iter()
            .any(|found| found == name)
        {
            problems.push(format!("create-request variable {name} must be absent"));
            continue;
        }
        if !credential_name(name) {
            continue;
        }
        let kind = value_kind(value);
        let accepted = match expected.get(name) {
            Some(EnvironmentKind::Placeholder) => kind == "placeholder",
            Some(EnvironmentKind::Empty) => kind == "empty",
            Some(EnvironmentKind::Brokered) => brokered_value(value, name),
            None => kind == "empty" || kind == "placeholder",
        };
        if !accepted {
            problems.push(format!(
                "create-request variable {name} holds a real value (kind {kind})"
            ));
        }
    }
    problems
}

pub(crate) fn judge_kinds(
    facts: &FactsJ08,
    behavior: &CredentialBehavior,
) -> (Vec<String>, Option<String>) {
    let mut problems = Vec::new();
    let mut skip = None;
    for var in &facts.environment {
        if var.kind == "other" {
            problems.push(format!(
                "credential variable {} holds a real value (kind other)",
                var.name
            ));
        }
        if behavior
            .absent_environment
            .iter()
            .any(|name| name == &var.name)
        {
            problems.push(format!("credential variable {} must be absent", var.name));
        }
    }
    for expected in &behavior.environment {
        if !facts
            .environment
            .iter()
            .any(|found| found.name == expected.name && found.kind == expected.kind.as_str())
        {
            problems.push(format!(
                "credential variable {} lacks {} delivery",
                expected.name,
                expected.kind.as_str()
            ));
        }
    }
    for var in &facts.environment {
        if var.kind == "brokered"
            && !behavior.environment.iter().any(|expected| {
                expected.name == var.name && expected.kind == EnvironmentKind::Brokered
            })
        {
            problems.push(format!(
                "credential variable {} has undeclared brokered delivery",
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
    let auth = &facts.auth_file;
    let wanted = &behavior.auth_file;
    if auth.state != wanted.required_state {
        problems.push(format!(
            "auth.json state is {}, expected {}",
            auth.state, wanted.required_state
        ));
    }
    if auth.path != wanted.path {
        problems.push(format!(
            "auth.json path is {}, expected {}",
            auth.path, wanted.path
        ));
    }
    if auth.mode != wanted.mode {
        problems.push(format!(
            "auth.json mode is {:?}, expected {}",
            auth.mode,
            wanted.mode.as_deref().unwrap_or("null")
        ));
    }
    let found: BTreeMap<&str, &str> = auth
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field.kind.as_str()))
        .collect();
    let expected: BTreeMap<String, String> = wanted
        .fields
        .iter()
        .map(|field| {
            let kind = match &field.kind {
                AuthFieldKind::Placeholder => "placeholder".to_owned(),
                AuthFieldKind::Empty => "empty".to_owned(),
                AuthFieldKind::Absent => "absent".to_owned(),
                AuthFieldKind::Literal { value } => format!("literal:{value}"),
            };
            (field.name.clone(), kind)
        })
        .collect();
    let found_owned: BTreeMap<String, String> = found
        .into_iter()
        .map(|(name, kind)| (name.to_owned(), kind.to_owned()))
        .collect();
    if found_owned != expected {
        problems.push("auth fields differ from the adapter declaration".to_owned());
    }
    if !facts.leaks.is_empty() {
        problems.push(format!(
            "the probe found token-shaped strings in {} files",
            facts.leaks.len()
        ));
    }
    (problems, skip)
}

pub(crate) fn judge_auth_text(bytes: &[u8], behavior: &CredentialBehavior) -> Vec<String> {
    let wanted = &behavior.auth_file;
    if wanted.required_state == "absent" {
        return vec!["the selected target requires an absent auth file".to_owned()];
    }
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return vec!["auth file in the sandbox is not JSON".to_owned()];
    };
    let mut problems = Vec::new();
    for field in &wanted.fields {
        let found = value.pointer(&field.name);
        let valid = match (&field.kind, found) {
            (AuthFieldKind::Absent, None | Some(Value::Null)) => true,
            (AuthFieldKind::Empty, Some(Value::String(text))) => text.is_empty(),
            (AuthFieldKind::Literal { value: expected }, Some(Value::String(text))) => {
                text == expected
            }
            (AuthFieldKind::Placeholder, Some(Value::String(text))) => {
                text.starts_with(PLACEHOLDER)
                    || text == "placeholder"
                    || text == CODEX_PLACEHOLDER_ID_TOKEN
            }
            _ => false,
        };
        if !valid {
            let name = field.name.rsplit('/').next().unwrap_or(&field.name);
            problems.push(format!("auth.json {name} is not a placeholder"));
        }
    }
    problems
}

pub(crate) fn judge_env_text(text: &str, behavior: &CredentialBehavior) -> Vec<String> {
    let values: BTreeMap<&str, &str> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    let mut problems = Vec::new();
    for name in &behavior.absent_environment {
        if values.contains_key(name.as_str()) {
            problems.push(format!("environment variable {name} must be absent"));
        }
    }
    for expected in &behavior.environment {
        let Some(value) = values.get(expected.name.as_str()) else {
            problems.push(format!("environment variable {} is missing", expected.name));
            continue;
        };
        let kind = value_kind(value);
        let valid = match expected.kind {
            EnvironmentKind::Placeholder => kind == "placeholder",
            EnvironmentKind::Empty => kind == "empty",
            EnvironmentKind::Brokered => brokered_value(value, expected.name.as_str()),
        };
        if !valid {
            problems.push(format!(
                "environment variable {} has kind {kind}",
                expected.name
            ));
        }
    }
    for (name, value) in values {
        if credential_name(name)
            && !behavior
                .environment
                .iter()
                .any(|expected| expected.name == name)
            && !behavior
                .absent_environment
                .iter()
                .any(|found| found == name)
            && !value.is_empty()
            && !value.starts_with(PLACEHOLDER)
        {
            problems.push(format!("environment variable {name} is not a placeholder"));
        }
    }
    problems
}

fn judge_config_text(bytes: &[u8], config: &ConfigFileBehavior) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut problems = Vec::new();
    for required in &config.required_text {
        if !text.contains(required) {
            problems.push(format!("{} lacks required configuration", config.path));
        }
    }
    for forbidden in &config.forbidden_text {
        if text.contains(forbidden) {
            problems.push(format!("{} contains forbidden configuration", config.path));
        }
    }
    problems
}

fn judge_missing_file(path: &str, behavior: &CredentialBehavior) -> Vec<String> {
    let mut problems = Vec::new();
    if behavior.auth_file.required_state == "present" && path == behavior.auth_file.path {
        problems.push("required auth file is missing".to_owned());
    }
    for config in &behavior.config_files {
        if config.required_state == "present" && path == config.path {
            problems.push(format!("required configuration {} is missing", config.path));
        }
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j08(&mut self) -> Verdict {
        self.j08_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn read_sandbox_files(&mut self) -> Res<Vec<String>> {
        let sandbox = self.sandbox_s()?;
        let launch = self.w.l1.clone().ok_or_else(|| fail("no launch id"))?;
        let behavior = self.credentials.target.behavior().credential.clone();
        let mut paths = vec![format!("/sandbox/.jackin/env-{launch}")];
        if behavior.auth_file.required_state != "absent" {
            paths.push(behavior.auth_file.path.clone());
        }
        paths.extend(
            behavior
                .config_files
                .iter()
                .map(|config| config.path.clone()),
        );
        paths.push("/sandbox/.jackin/run/env-kinds".to_owned());
        let parsed: Vec<SandboxPath> = paths
            .iter()
            .map(|path| SandboxPath::parse(path))
            .collect::<Result<_, _>>()?;
        let contents = self.b.gateway.read_files(&sandbox, &parsed).await?;
        let mut problems = Vec::new();
        for (path, content) in paths.iter().zip(contents) {
            let Some(bytes) = content else {
                problems.extend(judge_missing_file(path, &behavior));
                continue;
            };
            let label = if path.starts_with("/sandbox/.jackin/env-") {
                "env file"
            } else if *path == behavior.auth_file.path {
                "auth.json"
            } else {
                path.as_str()
            };
            self.capture(&format!("J08 {label}"), &bytes);
            if *path == behavior.auth_file.path {
                problems.extend(judge_auth_text(&bytes, &behavior));
            }
            if let Some(config) = behavior
                .config_files
                .iter()
                .find(|config| *path == config.path)
            {
                problems.extend(judge_config_text(&bytes, config));
            }
        }
        Ok(problems)
    }

    async fn j08_inner(&mut self) -> Res<Verdict> {
        let sandbox = self.sandbox_s()?;
        let report = self.shared("J08")?.clone();
        let facts: FactsJ08 = report.facts("J08")?;
        let mut problems: Vec<String> = report.gate("J08").into_iter().collect();
        let behavior = self.credentials.target.behavior().credential.clone();
        let (kind_problems, skip) = judge_kinds(&facts, &behavior);
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
        problems.extend(judge_env_text(&text_of(&env.stdout), &behavior));
        problems.extend(self.reflection(&sandbox).await);
        if !problems.is_empty() {
            Ok(Verdict::fail(problems.join("; ")))
        } else if let Some(reason) = skip {
            Ok(Verdict::skip(reason))
        } else {
            Ok(Verdict::pass(
                "declared credential delivery and configuration, no raw key, no reflection credential",
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
                judge_create_env(
                    &environment,
                    launch.as_str(),
                    &self.credentials.target.behavior().credential,
                )
            }
            Err(error) => vec![format!(
                "the create-request environment is unreadable: {error}"
            )],
        }
    }

    async fn reflection(&mut self, sandbox: &jackin_core::SandboxName) -> Vec<String> {
        let placeholder = format!("e2e-reflect-nonsecret-{}", self.b.tag);
        match self.reflect_through_agent(sandbox, &placeholder).await {
            Ok(problems) => problems,
            Err(failure) => vec![format!("the reflection check cannot run: {}", failure.0)],
        }
    }

    async fn reflect_through_agent(
        &mut self,
        sandbox: &jackin_core::SandboxName,
        placeholder: &str,
    ) -> Res<Vec<String>> {
        let behavior = self.credentials.target.behavior().reflection.clone();
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
                    content: SecretBytes::new(
                        reflect_script(&behavior.url, placeholder).into_bytes(),
                    ),
                },
            )
            .await?;
        let main = self.projects.main.clone();
        let mut session = self.open_pty(&["attach", sandbox.as_str()], &main)?;
        let typed = self.type_into_agent(&mut session).await;
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
        if !chain
            .split_whitespace()
            .any(|name| name == behavior.descendant_comm)
        {
            problems.push(format!(
                "the reflection command is not a descendant of {} (ancestors:{chain})",
                behavior.descendant_comm
            ));
        }
        if code != 0 || !body.starts_with(b"HTTP/") {
            problems.push(format!(
                "the reflection request got no HTTP answer (curl exit {code})"
            ));
        }
        Ok(problems)
    }

    async fn type_into_agent(&self, session: &mut crate::e2e::pty::PtySession) -> Res<i32> {
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
