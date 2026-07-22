//! Developer ID sign + notarize + staple for a built `JackinDesktop.app`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde_json::Value;

use crate::cmd;
use super::{
    assert_no_embedded_libs, progress, require_macos, resolve_version_build, tempfile_dir,
    verify_app, which,
};

#[derive(Args)]
pub(crate) struct SignNotarizeArgs {
    /// Path to `JackinDesktop.app` (default `native/dist/JackinDesktop.app`).
    app: Option<PathBuf>,
    /// Final post-staple ZIP path (default under `native/dist/`).
    out_zip: Option<PathBuf>,
    /// Short version (or env `JACKIN_APP_VERSION`).
    #[arg(long)]
    version: Option<String>,
    /// Build number (or env `JACKIN_APP_BUILD`).
    #[arg(long)]
    build: Option<String>,
}

pub(crate) fn run(args: SignNotarizeArgs) -> Result<()> {
    require_macos("desktop sign-notarize")?;
    let root = crate::docs::repo_root()?;
    let app = args
        .app
        .unwrap_or_else(|| root.join("native/dist/JackinDesktop.app"));
    let (version, build) = resolve_version_build(args.version, args.build)?;
    validate_stable_version(&version)?;

    let identity = env::var("DEVELOPER_ID_APPLICATION").context(
        "set DEVELOPER_ID_APPLICATION to the Developer ID Application identity",
    )?;
    if !app.is_dir() {
        bail!(
            "app not found at {} — run `cargo xtask desktop build` first",
            app.display()
        );
    }

    assert_no_embedded_libs(&app)?;

    progress("==> codesign (hardened runtime, secure timestamp, no --deep)");
    let mut codesign = cmd::command("codesign");
    codesign.args([
        "--force",
        "--options",
        "runtime",
        "--timestamp",
        "--sign",
        &identity,
        app.to_str().context("app utf-8")?,
    ]);
    cmd::run_streaming(&mut codesign)?;

    let mut verify = cmd::command("codesign");
    verify.args([
        "--verify",
        "--deep",
        "--strict",
        "--verbose=2",
        app.to_str().context("app utf-8")?,
    ]);
    cmd::run_streaming(&mut verify)?;

    check_expected_cert(&app)?;
    check_expected_team(&app)?;
    reject_get_task_allow(&app)?;

    let notary_log_dir = match env::var("NOTARY_LOG_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => match env::var("RUNNER_TEMP") {
            Ok(tmp) => PathBuf::from(tmp).join("jackin-notary"),
            Err(_) => PathBuf::from("/tmp/jackin-notary"),
        },
    };
    fs::create_dir_all(&notary_log_dir)?;
    let submit_zip = notary_log_dir.join("submit-JackinDesktop.zip");
    if submit_zip.exists() {
        fs::remove_file(&submit_zip)?;
    }
    progress("==> submission zip (disposable)");
    ditto_zip(&app, &submit_zip)?;

    progress("==> notarytool submit");
    let notary_json = notary_log_dir.join("notary-submit.json");
    run_notarytool(&submit_zip, &notary_json)?;
    let status = parse_notary_status(&notary_json)?;
    if status != "Accepted" {
        bail!(
            "notarytool status was '{status}', required Accepted (log: {})",
            notary_json.display()
        );
    }

    progress("==> staple + validate");
    let mut staple = cmd::command("xcrun");
    staple.args(["stapler", "staple", app.to_str().context("app utf-8")?]);
    cmd::run_streaming(&mut staple)?;
    let mut staple_v = cmd::command("xcrun");
    staple_v.args(["stapler", "validate", app.to_str().context("app utf-8")?]);
    cmd::run_streaming(&mut staple_v)?;
    let mut verify2 = cmd::command("codesign");
    verify2.args([
        "--verify",
        "--deep",
        "--strict",
        "--verbose=2",
        app.to_str().context("app utf-8")?,
    ]);
    cmd::run_streaming(&mut verify2)?;
    let mut spctl = cmd::command("spctl");
    spctl.args([
        "--assess",
        "--type",
        "execute",
        "--verbose=4",
        app.to_str().context("app utf-8")?,
    ]);
    cmd::run_streaming(&mut spctl).context("Gatekeeper assessment failed")?;

    progress("==> release-mode verifier");
    verify_app(&app, None, &version, &build, true)?;

    let out_zip = args.out_zip.unwrap_or_else(|| {
        root.join(format!(
            "native/dist/jackin-desktop-{version}-aarch64-apple-darwin.zip"
        ))
    });
    if out_zip.exists() {
        fs::remove_file(&out_zip)?;
    }
    if let Some(parent) = out_zip.parent() {
        fs::create_dir_all(parent)?;
    }
    progress(format!("==> final post-staple ZIP: {}", out_zip.display()));
    ditto_zip(&app, &out_zip)?;
    verify_app(&app, Some(&out_zip), &version, &build, true)?;

    drop(fs::remove_file(&submit_zip));
    progress(format!("==> signed, notarized, stapled: {}", app.display()));
    progress(format!("==> release zip: {}", out_zip.display()));
    Ok(())
}

fn validate_stable_version(version: &str) -> Result<()> {
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
        bail!("JACKIN_APP_VERSION must be stable X.Y.Z (got {version})");
    }
    Ok(())
}

fn ditto_zip(app: &Path, zip: &Path) -> Result<()> {
    which("ditto").context("ditto required for app ZIP")?;
    let parent = app.parent().context("app parent")?;
    let name = app.file_name().context("app name")?;
    let mut ditto = cmd::command("ditto");
    ditto
        .current_dir(parent)
        .args([
            "-c",
            "-k",
            "--keepParent",
            name.to_str().context("name utf-8")?,
            zip.to_str().context("zip utf-8")?,
        ]);
    cmd::run(&mut ditto)
}

fn run_notarytool(submit_zip: &Path, notary_json: &Path) -> Result<()> {
    let mut base = cmd::command("xcrun");
    base.arg("notarytool").arg("submit").arg(
        submit_zip
            .to_str()
            .context("submit zip utf-8")?,
    );

    if let Ok(key_path) = env::var("APP_STORE_CONNECT_API_KEY_PATH") {
        if !Path::new(&key_path).is_file() {
            bail!("APP_STORE_CONNECT_API_KEY_PATH not a file");
        }
        let key_id = env::var("APP_STORE_CONNECT_KEY_ID")
            .context("APP_STORE_CONNECT_KEY_ID required with API key path")?;
        let issuer = env::var("APP_STORE_CONNECT_ISSUER_ID")
            .context("APP_STORE_CONNECT_ISSUER_ID required for team API keys")?;
        base.args([
            "--key",
            &key_path,
            "--key-id",
            &key_id,
            "--issuer",
            &issuer,
            "--wait",
            "--output-format",
            "json",
        ]);
    } else if let Ok(profile) = env::var("NOTARY_PROFILE") {
        base.args([
            "--keychain-profile",
            &profile,
            "--wait",
            "--output-format",
            "json",
        ]);
    } else {
        bail!("set NOTARY_PROFILE or APP_STORE_CONNECT_API_KEY_PATH + KEY_ID + ISSUER_ID");
    }

    let out = cmd::output_string(&mut base).context("notarytool submit")?;
    fs::write(notary_json, &out)?;
    // Also stream for CI logs.
    progress(out.trim());
    Ok(())
}

fn parse_notary_status(path: &Path) -> Result<String> {
    let raw = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if let Ok(v) = serde_json::from_str::<Value>(&raw)
        && let Some(status) = v.get("status").and_then(Value::as_str)
    {
        return Ok(status.to_owned());
    }
    // Fallback: last line JSON blob
    if let Some(last) = raw.lines().last()
        && let Ok(v) = serde_json::from_str::<Value>(last)
        && let Some(status) = v.get("status").and_then(Value::as_str)
    {
        return Ok(status.to_owned());
    }
    Ok(String::new())
}

fn check_expected_cert(app: &Path) -> Result<()> {
    let Ok(expected_raw) = env::var("EXPECTED_CERT_SHA256") else {
        return Ok(());
    };
    if expected_raw.is_empty() {
        return Ok(());
    }
    let tmp = tempfile_dir("jackin-codesign-cert")?;
    let prefix = tmp.join("codesign-cert");
    let mut extract = cmd::command("codesign");
    extract.args([
        "-d",
        &format!("--extract-certificates={}", prefix.display()),
        app.to_str().context("app utf-8")?,
    ]);
    cmd::run(&mut extract).context("could not extract signing certificate")?;
    let cert_file = PathBuf::from(format!("{}0", prefix.display()));
    if !cert_file.is_file() {
        bail!("missing extracted leaf certificate");
    }
    let mut shasum = cmd::command("shasum");
    shasum.args(["-a", "256", cert_file.to_str().context("cert utf-8")?]);
    let hash_line = cmd::output_string(&mut shasum)?;
    let cert_hash = hash_line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let expected = expected_raw
        .to_ascii_lowercase()
        .chars()
        .filter(|c| *c != ':')
        .collect::<String>();
    if cert_hash != expected {
        bail!("certificate SHA-256 mismatch (expected configured fingerprint)");
    }
    drop(fs::remove_dir_all(&tmp));
    Ok(())
}

fn check_expected_team(app: &Path) -> Result<()> {
    let Ok(expected) = env::var("EXPECTED_TEAM_ID") else {
        return Ok(());
    };
    if expected.is_empty() {
        return Ok(());
    }
    // codesign -dv writes identity details to stderr; capture both streams.
    #[expect(
        clippy::disallowed_methods,
        reason = "codesign -dv emits identity on stderr; cmd helpers only surface stdout on success"
    )]
    let output = std::process::Command::new("codesign")
        .args([
            "-dv",
            "--verbose=4",
            app.to_str().context("app utf-8")?,
        ])
        .output()
        .context("codesign -dv")?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    match extract_team_identifier(&combined) {
        Some(team) if team == expected => Ok(()),
        Some(team) => bail!("TeamIdentifier mismatch (got {team})"),
        None => bail!("TeamIdentifier mismatch (got empty)"),
    }
}

fn extract_team_identifier(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("TeamIdentifier=") {
            return Some(rest.trim().to_owned());
        }
    }
    None
}

fn reject_get_task_allow(app: &Path) -> Result<()> {
    let mut codesign = cmd::command("codesign");
    codesign.args([
        "-d",
        "--entitlements",
        ":-",
        app.to_str().context("app utf-8")?,
    ]);
    let ents = cmd::output_string(&mut codesign).unwrap_or_default();
    if ents.contains("get-task-allow") {
        bail!("forbidden entitlement get-task-allow present");
    }
    Ok(())
}
