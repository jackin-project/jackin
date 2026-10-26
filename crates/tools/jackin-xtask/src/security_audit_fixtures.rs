// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result, bail, ensure};
use jackin_process::{ExecRequest, ExecResult, RetryPolicy, StdioMode};
use serde_json::Value;

const SYNTHETIC_ID: &str = "m-0123456789abcdef";
const SYNTHETIC_PEM: &str = concat!(
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
);

pub(crate) fn run(root: &Path) -> Result<()> {
    let root = fs::canonicalize(root).context("resolving scanner fixture source root")?;
    verify_scanner_version(&root, "gitleaks", "8.30.1")?;
    verify_scanner_version(&root, "betterleaks", "1.9.0")?;
    let fixture_parent = tempfile::Builder::new()
        .prefix("jackin-security-audit-")
        .tempdir_in(&root)
        .context("creating private scanner fixture parent")?;
    let fixture_path = fixture_parent.path().to_path_buf();
    let body: Result<()> = (|| {
        restrict_fixture_parent(&fixture_path)
            .context("restricting scanner fixture parent permissions")?;
        test_gitleaks_allowlist(&root, &fixture_path)?;
        test_betterleaks_allowlist(&root, &fixture_path)?;
        test_audit_tool_provenance(&root, &fixture_path)?;
        Ok(())
    })();
    let cleanup = fixture_parent.close();
    finish_with_cleanup(body, cleanup, &fixture_path)?;
    write_success()
}

fn test_gitleaks_allowlist(root: &Path, parent: &Path) -> Result<()> {
    let config = fs::read_to_string(root.join(".gitleaks.toml"))
        .context("reading staged Gitleaks config")?;
    let target = ".github/velnor/task-execution-manifest-v1.json";
    let raw = make_gitleaks_repo(
        parent,
        "gitleaks-unallowlisted",
        target,
        &manifest("matrix_key", SYNTHETIC_ID)?,
        &config,
        true,
    )?;
    let raw_hits = generic_findings(&write_report(
        &raw,
        "gitleaks",
        &["git", ".", "--no-banner"],
    )?);
    ensure!(
        raw_hits.len() == 1,
        "pinned Gitleaks default rule did not detect synthetic matrix key"
    );
    ensure!(
        raw_hits[0]["Match"] == format!("matrix_key\":\"{SYNTHETIC_ID}\""),
        "pinned Gitleaks matrix match spelling changed: {:?}",
        raw_hits[0]["Match"]
    );

    let allowed = make_gitleaks_repo(
        parent,
        "gitleaks-matrix-allowed",
        target,
        &manifest("matrix_key", SYNTHETIC_ID)?,
        &config,
        false,
    )?;
    ensure!(
        generic_findings(&write_report(
            &allowed,
            "gitleaks",
            &["git", ".", "--no-banner"]
        )?)
        .is_empty(),
        "Gitleaks matrix allowlist stopped applying"
    );

    let api = make_gitleaks_repo(
        parent,
        "gitleaks-api-key-not-allowed",
        target,
        &manifest("api_key", SYNTHETIC_ID)?,
        &config,
        false,
    )?;
    let api_hits = generic_findings(&write_report(
        &api,
        "gitleaks",
        &["git", ".", "--no-banner"],
    )?);
    ensure!(
        api_hits.len() == 1,
        "Gitleaks allowlist suppressed same-file api_key synthetic finding"
    );
    ensure!(
        api_hits[0]["Match"] == format!("api_key\":\"{SYNTHETIC_ID}\""),
        "pinned Gitleaks api_key match spelling changed"
    );

    let other = make_gitleaks_repo(
        parent,
        "gitleaks-other-path",
        ".github/velnor/other.json",
        &manifest("matrix_key", SYNTHETIC_ID)?,
        &config,
        false,
    )?;
    ensure!(
        generic_findings(&write_report(
            &other,
            "gitleaks",
            &["git", ".", "--no-banner"]
        )?)
        .len()
            == 1,
        "Gitleaks allowlist escaped its intended path"
    );

    let changed = make_gitleaks_repo(
        parent,
        "gitleaks-changed-shape",
        target,
        &manifest("matrix_key", &format!("{SYNTHETIC_ID}0"))?,
        &config,
        false,
    )?;
    ensure!(
        generic_findings(&write_report(
            &changed,
            "gitleaks",
            &["git", ".", "--no-banner"]
        )?)
        .len()
            == 1,
        "Gitleaks allowlist accepted a value outside its exact shape"
    );

    let private_value = format!(
        "{{\"matrix_key\":\"{SYNTHETIC_ID}\",\
         \"private_key\":\"-----BEGIN RSA PRIVATE KEY-----\n\
         {SYNTHETIC_PEM}\n\
         -----END RSA PRIVATE KEY-----\"}}\n"
    );
    let private_key = make_gitleaks_repo(
        parent,
        "gitleaks-other-rule",
        target,
        &private_value,
        &config,
        false,
    )?;
    let findings = write_report(&private_key, "gitleaks", &["git", ".", "--no-banner"])?;
    ensure!(
        generic_findings(&findings).is_empty(),
        "Gitleaks matrix allowlist stopped suppressing its intended rule"
    );
    ensure!(
        findings
            .iter()
            .any(|finding| finding["RuleID"] == "private-key"),
        "Gitleaks matrix allowlist suppressed a different rule"
    );
    Ok(())
}

fn test_betterleaks_allowlist(root: &Path, parent: &Path) -> Result<()> {
    let production = fs::read_to_string(root.join(".gitleaks.toml"))
        .context("reading staged scanner allowlist")?;
    let target = ".github/velnor/task-execution-manifest-v1.json";
    let raw = make_betterleaks_repo(
        parent,
        "betterleaks-unallowlisted",
        target,
        &manifest("matrix_key", SYNTHETIC_ID)?,
        false,
        &production,
    )?;
    let hits = generic_findings(&write_report(
        &raw,
        "betterleaks",
        &["git", ".", "--confidence", "low"],
    )?);
    ensure!(
        hits.len() == 1,
        "test-only Betterleaks rule did not emit the matrix match"
    );
    ensure!(
        hits[0]["Match"] == format!("key\":\"{SYNTHETIC_ID}\""),
        "pinned Betterleaks match spelling changed"
    );

    let allowed = make_betterleaks_repo(
        parent,
        "betterleaks-matrix-allowed",
        target,
        &manifest("matrix_key", SYNTHETIC_ID)?,
        true,
        &production,
    )?;
    ensure!(
        generic_findings(&write_report(
            &allowed,
            "betterleaks",
            &["git", ".", "--confidence", "low"]
        )?)
        .is_empty(),
        "Betterleaks matrix allowlist stopped applying"
    );

    let api = make_betterleaks_repo(
        parent,
        "betterleaks-api-key-not-allowed",
        target,
        &manifest("api_key", SYNTHETIC_ID)?,
        true,
        &production,
    )?;
    let api_hits = generic_findings(&write_report(
        &api,
        "betterleaks",
        &["git", ".", "--confidence", "low"],
    )?);
    ensure!(
        api_hits.len() == 1,
        "Betterleaks allowlist suppressed same-file api_key synthetic finding"
    );
    ensure!(
        api_hits[0]["Match"] == format!("api_key\":\"{SYNTHETIC_ID}\""),
        "pinned Betterleaks api_key match spelling changed"
    );

    let other = make_betterleaks_repo(
        parent,
        "betterleaks-other-path",
        ".github/velnor/other.json",
        &manifest("matrix_key", SYNTHETIC_ID)?,
        true,
        &production,
    )?;
    let other_hits = generic_findings(&write_report(
        &other,
        "betterleaks",
        &["git", ".", "--confidence", "low"],
    )?);
    ensure!(
        other_hits.len() == 1,
        "Betterleaks allowlist escaped its generated-manifest path"
    );
    ensure!(
        other_hits[0]["Match"] == format!("key\":\"{SYNTHETIC_ID}\""),
        "pinned Betterleaks other-path match spelling changed"
    );

    let changed = make_betterleaks_repo(
        parent,
        "betterleaks-changed-shape",
        target,
        &manifest("matrix_key", &format!("{SYNTHETIC_ID}0"))?,
        true,
        &production,
    )?;
    ensure!(
        generic_findings(&write_report(
            &changed,
            "betterleaks",
            &["git", ".", "--confidence", "low"]
        )?)
        .len()
            == 1,
        "Betterleaks allowlist accepted a value outside its exact shape"
    );
    Ok(())
}

fn test_audit_tool_provenance(root: &Path, parent: &Path) -> Result<()> {
    let config = fs::read_to_string(root.join("hk.pkl")).context("reading staged hk config")?;
    let required = [
        concat!(
            "mise exec --locked -- kingfisher scan . --git-history full --confidence low ",
            "--no-validate --redact --no-update-check --baseline-file ",
            ".kingfisher-baseline.yml"
        ),
        "mise exec --locked -- betterleaks git . --redact",
        "mise exec --locked -- gitleaks git . --redact --no-banner",
    ];
    for command in required {
        ensure!(
            config.contains(command),
            "security-audit no longer selects its locked scanner command"
        );
    }

    let fixture = parent.join("audit-provenance");
    fs::create_dir(&fixture)?;
    git(&fixture, &["init", "-q"])?;
    git(&fixture, &["config", "user.name", "Jackin audit fixture"])?;
    git(
        &fixture,
        &[
            "config",
            "user.email",
            "jackin-audit-fixture@example.invalid",
        ],
    )?;
    for name in ["hk.pkl", ".gitleaks.toml", ".kingfisher-baseline.yml"] {
        fs::copy(root.join(name), fixture.join(name))
            .with_context(|| format!("copying staged {name}"))?;
    }
    for name in [".gitleaksignore", ".betterleaksignore"] {
        let source = root.join(name);
        if source.exists() {
            fs::copy(source, fixture.join(name))
                .with_context(|| format!("copying staged {name}"))?;
        }
    }
    git(&fixture, &["add", "--", "."])?;
    git(&fixture, &["commit", "-q", "-m", "locked audit fixture"])?;

    let fake_bin = parent.join("ambient-bin");
    fs::create_dir(&fake_bin)?;
    let marker = parent.join("ambient-scanners-ran");
    for scanner in ["kingfisher", "betterleaks", "gitleaks"] {
        let executable = fake_bin.join(scanner);
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' {scanner} >> \"$AMBIENT_MARKER\"\nexit 97\n"
            ),
        )?;
        fs::set_permissions(executable, fs::Permissions::from_mode(0o755))?;
    }
    let hk = mise_which(root, "hk")?;
    let mut path = fake_bin.into_os_string();
    path.push(":");
    path.push(env::var_os("PATH").context("reading inherited PATH for provenance fixture")?);
    let mut environment = BTreeMap::new();
    environment.insert("PATH".into(), path);
    environment.insert("AMBIENT_MARKER".into(), marker.as_os_str().to_owned());
    environment.insert("GIT_CONFIG_GLOBAL".into(), OsString::from("/dev/null"));
    environment.insert("GIT_CONFIG_SYSTEM".into(), OsString::from("/dev/null"));
    environment.insert("GIT_CONFIG_NOSYSTEM".into(), OsString::from("1"));
    let output = run_command(
        hk.as_os_str(),
        &["run", "security-audit"],
        &fixture,
        &environment,
    )?;
    ensure!(
        output.success,
        "locked security audit failed in clean scanner fixture ({:?})",
        output.code
    );
    ensure!(
        !marker.exists(),
        "ambient scanner shim bypassed `mise exec --locked` provenance"
    );
    Ok(())
}

fn make_gitleaks_repo(
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

fn make_betterleaks_repo(
    parent: &Path,
    name: &str,
    path: &str,
    contents: &str,
    allowlist: bool,
    production: &str,
) -> Result<PathBuf> {
    let mut config = String::from(concat!(
        "[extend]\nuseDefault = false\n\n",
        "[[rules]]\nid = \"generic-api-key\"\n",
        "description = \"synthetic match-shape rule; not a production detection rule\"\n",
        "regex = '''(?:api_key|key)\":\"(?P<secret>m-[0-9a-f]+)\"'''\n",
        "secretGroup = 1\nentropy = 0\n"
    ));
    if allowlist {
        let index = production
            .find("[[allowlists]]")
            .context("staged scanner config lacks its production allowlist")?;
        config.push_str(&production[index..]);
    }
    let repo = parent.join(name);
    make_repo(&repo, &[(".gitleaks.toml", &config), (path, contents)])?;
    Ok(repo)
}

fn make_repo(path: &Path, files: &[(&str, &str)]) -> Result<()> {
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
        if let Some(parent) = file
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(file, contents.as_bytes())
            .with_context(|| format!("writing scanner fixture {relative}"))?;
    }
    git(path, &["add", "--", "."])?;
    git(path, &["commit", "-q", "-m", "synthetic scanner fixture"])?;
    Ok(())
}

fn write_report(fixture: &Path, scanner: &str, arguments: &[&str]) -> Result<Vec<Value>> {
    let report = fixture.join(format!("{scanner}-report.json"));
    let report_arg = report.to_string_lossy().into_owned();
    let mut args = vec!["exec", "--locked", "--", scanner];
    args.extend_from_slice(arguments);
    args.extend(["--report-format", "json", "--report-path", &report_arg]);
    let output = run_command(OsStr::new("mise"), &args, fixture, &BTreeMap::new())?;
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

fn generic_findings(report: &[Value]) -> Vec<Value> {
    report
        .iter()
        .filter(|finding| finding.get("RuleID").and_then(Value::as_str) == Some("generic-api-key"))
        .cloned()
        .collect()
}

fn manifest(field: &str, value: &str) -> Result<String> {
    let mut manifest = serde_json::Map::new();
    manifest.insert(field.to_owned(), Value::String(value.to_owned()));
    Ok(format!(
        "{}\n",
        serde_json::to_string(&Value::Object(manifest))?
    ))
}

fn verify_scanner_version(root: &Path, scanner: &str, expected: &str) -> Result<()> {
    let output = run_command(
        OsStr::new("mise"),
        &["exec", "--locked", "--", scanner, "version"],
        root,
        &BTreeMap::new(),
    )?;
    ensure!(
        output.success,
        "pinned {scanner} version probe failed ({:?})",
        output.code
    );
    let stdout = String::from_utf8(output.stdout).context("decoding scanner version")?;
    ensure!(
        stdout.contains(expected),
        "unexpected pinned {scanner} version"
    );
    Ok(())
}

fn mise_which(root: &Path, name: &str) -> Result<PathBuf> {
    let output = run_command(OsStr::new("mise"), &["which", name], root, &BTreeMap::new())?;
    ensure!(
        output.success,
        "could not resolve pinned {name} executable ({:?})",
        output.code
    );
    let path = String::from_utf8(output.stdout).context("decoding Mise tool path")?;
    Ok(PathBuf::from(path.trim()))
}

fn git(root: &Path, arguments: &[&str]) -> Result<()> {
    let mut environment = BTreeMap::new();
    environment.insert("GIT_CONFIG_GLOBAL".into(), OsString::from("/dev/null"));
    environment.insert("GIT_CONFIG_SYSTEM".into(), OsString::from("/dev/null"));
    environment.insert("GIT_CONFIG_NOSYSTEM".into(), OsString::from("1"));
    let output = run_command(OsStr::new("git"), arguments, root, &environment)?;
    ensure!(
        output.success,
        "scanner fixture Git setup failed with {:?}",
        output.code
    );
    Ok(())
}

fn run_command(
    program: &OsStr,
    arguments: &[&str],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<ExecResult> {
    let executable = resolve_executable(program, cwd, environment)?;
    let request = build_exec_request(&executable, arguments, cwd, environment);
    let output = jackin_process::exec_sync(&request)
        .with_context(|| format!("starting scanner fixture command {}", executable.display()))?;
    std::str::from_utf8(&output.stdout).context("decoding scanner fixture stdout as UTF-8")?;
    std::str::from_utf8(&output.stderr).context("decoding scanner fixture stderr as UTF-8")?;
    Ok(output)
}

fn build_exec_request(
    executable: &Path,
    arguments: &[&str],
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> ExecRequest {
    ExecRequest::new(executable.to_path_buf(), arguments.iter().copied())
        .cwd(cwd)
        .envs(
            environment
                .iter()
                .map(|(key, value)| (key.as_os_str(), value.as_os_str())),
        )
        .retry(RetryPolicy::none())
        .stdin_mode(StdioMode::Inherit)
        .stdout_mode(StdioMode::Capture)
        .stderr_mode(StdioMode::Capture)
}

fn resolve_executable(
    program: &OsStr,
    cwd: &Path,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<PathBuf> {
    let program_path = Path::new(program);
    if program_path.is_absolute() {
        return Ok(program_path.to_path_buf());
    }
    if program_path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) || program_path
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        return Ok(cwd.join(program_path));
    }
    let path = environment
        .get(OsStr::new("PATH"))
        .cloned()
        .or_else(|| env::var_os("PATH"));
    find_executable(program, path.as_deref(), cwd)
}

fn search_directories(path: Option<&OsStr>, cwd: &Path) -> Vec<PathBuf> {
    let path = path.unwrap_or_else(|| OsStr::new("/bin:/usr/bin"));
    env::split_paths(path)
        .map(|directory| {
            if directory.as_os_str().is_empty() {
                cwd.to_path_buf()
            } else if directory.is_absolute() {
                directory
            } else {
                cwd.join(directory)
            }
        })
        .collect()
}

fn find_executable(name: &OsStr, path: Option<&OsStr>, cwd: &Path) -> Result<PathBuf> {
    for directory in search_directories(path, cwd) {
        let candidate = directory.join(name);
        if is_executable(&candidate) {
            return fs::canonicalize(&candidate)
                .with_context(|| format!("resolving executable {}", candidate.display()));
        }
    }
    bail!(
        "executable {} is not present on PATH",
        name.to_string_lossy()
    )
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use nix::unistd::{AccessFlags, access};

        access(path, AccessFlags::F_OK | AccessFlags::X_OK).is_ok()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn finish_with_cleanup<T>(
    body: Result<T>,
    cleanup: io::Result<()>,
    fixture_path: &Path,
) -> Result<T> {
    match (body, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(body_error), Ok(())) => Err(body_error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error)
            .with_context(|| format!("removing scanner fixture parent {}", fixture_path.display())),
        (Err(body_error), Err(cleanup_error)) => Err(body_error).context(format!(
            "also failed to remove scanner fixture parent {}: {cleanup_error}",
            fixture_path.display()
        )),
    }
}

fn write_success() -> Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "pinned security-audit synthetic fixtures passed")
        .context("writing security-audit fixture result")?;
    stdout
        .flush()
        .context("flushing security-audit fixture result")
}

#[cfg(unix)]
fn restrict_fixture_parent(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_fixture_parent(_: &Path) -> io::Result<()> {
    Ok(())
}
