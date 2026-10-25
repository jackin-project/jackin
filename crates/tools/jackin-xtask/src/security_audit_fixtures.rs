// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Native qualification fixtures for the pinned security scanners.

use std::{env, ffi::OsString, fs, io::Write, os::unix::fs::PermissionsExt, path::Path};

use anyhow::{Context, Result, ensure};

mod fixtures;
use fixtures::{
    find_executable, generic_findings, git, make_betterleaks_repo, make_gitleaks_repo, manifest,
    mise_which, run_command, runtime_environment, verify_scanner_version, write_report,
};

const SYNTHETIC_ID: &str = "m-0123456789abcdef";
const SYNTHETIC_PEM: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

pub(crate) fn run(root: &Path) -> Result<()> {
    let root = fs::canonicalize(root).context("resolving scanner fixture source root")?;
    let mise = find_executable("mise", &env::var_os("PATH").unwrap_or_default())?;
    verify_scanner_version(&mise, &root, "gitleaks", "8.30.1")?;
    verify_scanner_version(&mise, &root, "betterleaks", "1.9.0")?;
    let fixture_parent = tempfile::Builder::new()
        .prefix("jackin-security-audit-")
        .tempdir_in(&root)
        .context("creating private scanner fixture parent")?;
    fs::set_permissions(fixture_parent.path(), fs::Permissions::from_mode(0o700))?;
    test_gitleaks_allowlist(&root, fixture_parent.path(), &mise)?;
    test_betterleaks_allowlist(&root, fixture_parent.path(), &mise)?;
    test_audit_tool_provenance(&root, fixture_parent.path(), &mise)?;
    writeln!(
        std::io::stdout().lock(),
        "pinned security-audit native fixtures passed"
    )
    .context("writing scanner fixture result")?;
    Ok(())
}

fn test_gitleaks_allowlist(root: &Path, parent: &Path, mise: &Path) -> Result<()> {
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
        mise,
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
            mise,
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
        mise,
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
            mise,
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
            mise,
            &changed,
            "gitleaks",
            &["git", ".", "--no-banner"]
        )?)
        .len()
            == 1,
        "Gitleaks allowlist accepted a value outside its exact shape"
    );

    let private_value = format!(
        "{{\"matrix_key\":\"{SYNTHETIC_ID}\",\"private_key\":\"-----BEGIN RSA PRIVATE KEY-----\n{SYNTHETIC_PEM}\n-----END RSA PRIVATE KEY-----\"}}\n"
    );
    let private_key = make_gitleaks_repo(
        parent,
        "gitleaks-other-rule",
        target,
        &private_value,
        &config,
        false,
    )?;
    let findings = write_report(mise, &private_key, "gitleaks", &["git", ".", "--no-banner"])?;
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

fn test_betterleaks_allowlist(root: &Path, parent: &Path, mise: &Path) -> Result<()> {
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
        mise,
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
            mise,
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
        mise,
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
        mise,
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
            mise,
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

fn test_audit_tool_provenance(root: &Path, parent: &Path, mise: &Path) -> Result<()> {
    let config = fs::read_to_string(root.join("hk.pkl")).context("reading staged hk config")?;
    let required = [
        "mise exec --locked -- kingfisher scan . --git-history full --confidence low --no-validate --redact --no-update-check --baseline-file .kingfisher-baseline.yml",
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
    let hk = mise_which(mise, &fixture, "hk")?;
    let mut path = fake_bin.into_os_string();
    path.push(":");
    path.push(env::var_os("PATH").unwrap_or_default());
    let mut environment = runtime_environment();
    environment.insert("PATH".into(), path);
    environment.insert("AMBIENT_MARKER".into(), marker.as_os_str().to_owned());
    environment.insert("GIT_CONFIG_GLOBAL".into(), OsString::from("/dev/null"));
    environment.insert("GIT_CONFIG_NOSYSTEM".into(), OsString::from("1"));
    let output = run_command(&hk, &["run", "security-audit"], &fixture, &environment)?;
    ensure!(
        output.success,
        "locked security audit failed in clean scanner fixture"
    );
    ensure!(
        !marker.exists(),
        "ambient scanner shim bypassed `mise exec --locked` provenance"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
