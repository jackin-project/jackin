// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Scanner fixture construction and isolated command execution.

use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

pub(super) fn make_gitleaks_repo(
    parent: &Path,
    name: &str,
    path: &str,
    contents: &str,
    config: &str,
    remove_allowlist: bool,
) -> Result<PathBuf> {
    let repo = parent.join(name);
    let scanner_config = if remove_allowlist {
        config
            .find("[[allowlists]]")
            .map_or(config, |index| &config[..index])
    } else {
        config
    };
    make_repo(
        &repo,
        &[(".gitleaks.toml", scanner_config), (path, contents)],
    )?;
    Ok(repo)
}

pub(super) fn make_betterleaks_repo(
    parent: &Path,
    name: &str,
    path: &str,
    contents: &str,
    allowlist: bool,
    production: &str,
) -> Result<PathBuf> {
    let mut config = String::from(
        "[extend]\nuseDefault = false\n\n[[rules]]\nid = \"generic-api-key\"\ndescription = \"synthetic match-shape rule; not a production detection rule\"\nregex = '''(?:api_key|key)\":\"(?P<secret>m-[0-9a-f]+)\"'''\nsecretGroup = 1\nentropy = 0\n",
    );
    if allowlist && let Some(index) = production.find("[[allowlists]]") {
        config.push_str(&production[index..]);
    }
    let repo = parent.join(name);
    make_repo(&repo, &[(".gitleaks.toml", &config), (path, contents)])?;
    Ok(repo)
}

pub(super) fn make_repo(path: &Path, files: &[(&str, &str)]) -> Result<()> {
    fs::create_dir(path).with_context(|| format!("creating scanner fixture {}", path.display()))?;
    git(path, &["init", "-q"])?;
    git(path, &["config", "user.name", "Jackin scanner fixture"])?;
    git(
        path,
        &[
            "config",
            "user.email",
            "jackin-scanner-fixture@example.invalid",
        ],
    )?;
    for (relative, contents) in files {
        let file = path.join(relative);
        fs::create_dir_all(file.parent().context("scanner fixture path lacks parent")?)?;
        fs::write(file, contents).with_context(|| format!("writing scanner fixture {relative}"))?;
    }
    git(path, &["add", "--", "."])?;
    git(path, &["commit", "-q", "-m", "synthetic scanner fixture"])?;
    Ok(())
}

pub(super) fn write_report(
    mise: &Path,
    fixture: &Path,
    scanner: &str,
    arguments: &[&str],
) -> Result<Vec<Value>> {
    let report = fixture.join(format!("{scanner}-report.json"));
    let report_arg = report.to_string_lossy().into_owned();
    let mut args = vec!["exec", "--locked", "--", scanner];
    args.extend_from_slice(arguments);
    args.extend(["--report-format", "json", "--report-path", &report_arg]);
    let output = run_command(mise, &args, fixture, &runtime_environment())?;
    ensure!(
        matches!(output.code, Some(0 | 1)),
        "{scanner} synthetic scan returned an unexpected status"
    );
    if !report.exists() {
        ensure!(
            output.code == Some(0),
            "{scanner} did not write a finding report"
        );
        return Ok(Vec::new());
    }
    let value: Value =
        serde_json::from_slice(&fs::read(report).context("reading scanner finding report")?)
            .context("parsing scanner finding report")?;
    let findings = value.as_array().context("scanner report format changed")?;
    ensure!(
        findings.is_empty() != (output.code == Some(1)),
        "{scanner} status disagrees with its JSON findings"
    );
    Ok(findings.clone())
}

pub(super) fn generic_findings(report: &[Value]) -> Vec<Value> {
    report
        .iter()
        .filter(|finding| finding.get("RuleID").and_then(Value::as_str) == Some("generic-api-key"))
        .cloned()
        .collect()
}

pub(super) fn manifest(field: &str, value: &str) -> Result<String> {
    let mut manifest = serde_json::Map::new();
    manifest.insert(field.to_owned(), Value::String(value.to_owned()));
    Ok(format!(
        "{}\n",
        serde_json::to_string(&Value::Object(manifest))?
    ))
}

pub(super) fn verify_scanner_version(
    mise: &Path,
    root: &Path,
    scanner: &str,
    expected: &str,
) -> Result<()> {
    let output = run_command(
        mise,
        &["exec", "--locked", "--", scanner, "version"],
        root,
        &runtime_environment(),
    )?;
    ensure!(output.success, "pinned {scanner} version probe failed");
    let stdout = String::from_utf8(output.stdout).context("decoding scanner version")?;
    ensure!(
        stdout.contains(expected),
        "unexpected pinned {scanner} version"
    );
    Ok(())
}

pub(super) fn mise_which(mise: &Path, cwd: &Path, name: &str) -> Result<PathBuf> {
    let output = run_command(mise, &["which", name], cwd, &runtime_environment())?;
    ensure!(output.success, "could not resolve pinned {name} executable");
    let path = String::from_utf8(output.stdout).context("decoding Mise tool path")?;
    fs::canonicalize(path.trim()).with_context(|| format!("resolving Mise tool {name}"))
}

pub(super) fn git(root: &Path, arguments: &[&str]) -> Result<()> {
    let mut environment = runtime_environment();
    environment.insert("GIT_CONFIG_GLOBAL".into(), OsString::from("/dev/null"));
    environment.insert("GIT_CONFIG_SYSTEM".into(), OsString::from("/dev/null"));
    environment.insert("GIT_CONFIG_NOSYSTEM".into(), OsString::from("1"));
    let output = run_command(Path::new("git"), arguments, root, &environment)?;
    ensure!(
        output.success,
        "scanner fixture Git setup failed with {}",
        output
            .code
            .map_or_else(|| "signal".to_owned(), |code| code.to_string())
    );
    Ok(())
}

pub(super) fn run_command(
    program: &Path,
    arguments: &[&str],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<jackin_process::ExecResult> {
    let request = jackin_process::ExecRequest::new(program.as_os_str(), arguments)
        .cwd(cwd)
        .env_clear()
        .envs(environment.iter());
    jackin_process::exec_sync(&request)
        .with_context(|| format!("starting scanner fixture command {}", program.display()))
}

pub(super) fn runtime_environment() -> BTreeMap<OsString, OsString> {
    let mut result = BTreeMap::new();
    result.insert(
        OsString::from("MISE_EXEC_AUTO_INSTALL"),
        OsString::from("false"),
    );
    for key in [
        "HOME", "USER", "LOGNAME", "TMPDIR", "LANG", "LC_ALL", "TERM", "PATH",
    ] {
        if let Some(value) = env::var_os(key) {
            result.insert(OsString::from(key), value);
        }
    }
    if let Ok(root) = env::current_dir() {
        result.insert(
            OsString::from("MISE_TRUSTED_CONFIG_PATHS"),
            root.into_os_string(),
        );
    }
    result
}

pub(super) fn find_executable(name: &str, path: &std::ffi::OsStr) -> Result<PathBuf> {
    for directory in env::split_paths(path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return fs::canonicalize(candidate)
                .with_context(|| format!("resolving {name} executable"));
        }
    }
    bail!("{name} is missing from PATH")
}
