//! Bootstrap GitHub environment `release-macos` Apple credentials.
//!
//! Never prints secret material. Uses `gh secret set` / `gh variable set`.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::Args;

use super::{progress, tempfile_dir, which};

const ENV_NAME: &str = "release-macos";
const DEFAULT_REPO: &str = "jackin-project/jackin";

#[derive(Args)]
pub(crate) struct BootstrapSecretsArgs {
    #[arg(long)]
    p12: Option<PathBuf>,
    #[arg(long)]
    p12_password: Option<String>,
    #[arg(long)]
    p12_password_env: Option<String>,
    #[arg(long)]
    p8: Option<PathBuf>,
    #[arg(long)]
    key_id: Option<String>,
    #[arg(long)]
    issuer: Option<String>,
    #[arg(long)]
    team_id: Option<String>,
    #[arg(long)]
    cert_sha256: Option<String>,
    #[arg(long)]
    op_p12: Option<String>,
    #[arg(long)]
    op_p12_password: Option<String>,
    #[arg(long)]
    op_p8: Option<String>,
    #[arg(long, default_value = DEFAULT_REPO)]
    repo: String,
    #[arg(long)]
    dry_run: bool,
}

pub(crate) fn run(args: BootstrapSecretsArgs) -> Result<()> {
    which("gh").context("gh CLI required")?;
    which("base64").context("base64 required")?;
    which("shasum").context("shasum required")?;

    let key_id = args
        .key_id
        .clone()
        .or_else(|| env::var("APP_STORE_CONNECT_KEY_ID").ok())
        .filter(|s| !s.is_empty())
        .context("App Store Connect key id required (--key-id)")?;
    let issuer = args
        .issuer
        .clone()
        .or_else(|| env::var("APP_STORE_CONNECT_ISSUER_ID").ok())
        .filter(|s| !s.is_empty())
        .context("App Store Connect issuer id required (--issuer) for team keys")?;
    let mut team_id = args
        .team_id
        .clone()
        .or_else(|| env::var("JACKIN_DEVELOPER_ID_TEAM_ID").ok())
        .filter(|s| !s.is_empty());
    let mut cert_sha256 = args
        .cert_sha256
        .clone()
        .or_else(|| env::var("JACKIN_DEVELOPER_ID_CERT_SHA256").ok())
        .filter(|s| !s.is_empty());

    progress("resolving PKCS#12 (not printing bytes)");
    let p12_b64 = resolve_p12_b64(&args)?;
    progress("resolving PKCS#12 password (not printing)");
    let p12_pass = resolve_p12_password(&args)?;
    progress("resolving App Store Connect .p8 (not printing)");
    let p8_body = resolve_p8(&args)?;

    if cert_sha256.is_none()
        && let Some(path) = args.p12.as_ref()
        && path.is_file()
    {
        progress("deriving certificate SHA-256 from p12");
        cert_sha256 = derive_cert_sha256_from_p12(path, &p12_pass).ok();
    }
    if cert_sha256.is_none() {
        progress(
            "warning: CERT_SHA256 empty — publish will skip fingerprint fail-closed check unless you set JACKIN_DEVELOPER_ID_CERT_SHA256",
        );
    }
    if team_id.is_none() {
        progress(
            "warning: TEAM_ID empty — set JACKIN_DEVELOPER_ID_TEAM_ID for fail-closed Team ID check",
        );
    }

    if args.dry_run {
        progress(format!(
            "dry-run: would set secrets/vars on {} env={ENV_NAME}",
            args.repo
        ));
        progress(
            "  secrets: DEVELOPER_ID_APPLICATION_P12_BASE64, DEVELOPER_ID_APPLICATION_P12_PASSWORD, APP_STORE_CONNECT_API_KEY_P8, APP_STORE_CONNECT_KEY_ID, APP_STORE_CONNECT_ISSUER_ID",
        );
        progress("  variables: JACKIN_DEVELOPER_ID_TEAM_ID, JACKIN_DEVELOPER_ID_CERT_SHA256");
        progress(format!(
            "  key-id length={} issuer length={} p12_b64 length={} p8 length={}",
            key_id.len(),
            issuer.len(),
            p12_b64.len(),
            p8_body.len()
        ));
        return Ok(());
    }

    progress(format!("ensuring environment {ENV_NAME} exists"));
    ensure_environment(&args.repo)?;

    progress(format!(
        "writing secrets to {} environment {ENV_NAME} (values not echoed)",
        args.repo
    ));
    gh_secret_set(&args.repo, "DEVELOPER_ID_APPLICATION_P12_BASE64", &p12_b64)?;
    gh_secret_set(
        &args.repo,
        "DEVELOPER_ID_APPLICATION_P12_PASSWORD",
        &p12_pass,
    )?;
    gh_secret_set(&args.repo, "APP_STORE_CONNECT_API_KEY_P8", &p8_body)?;
    gh_secret_set(&args.repo, "APP_STORE_CONNECT_KEY_ID", &key_id)?;
    gh_secret_set(&args.repo, "APP_STORE_CONNECT_ISSUER_ID", &issuer)?;

    if let Some(team) = team_id.take() {
        gh_variable_set(&args.repo, "JACKIN_DEVELOPER_ID_TEAM_ID", &team)?;
    }
    if let Some(sha) = cert_sha256.take() {
        gh_variable_set(&args.repo, "JACKIN_DEVELOPER_ID_CERT_SHA256", &sha)?;
    }

    progress("done. Verify names only:");
    let mut list = cmd_command("gh");
    list.args(["secret", "list", "--repo", &args.repo, "--env", ENV_NAME]);
    crate::cmd::run_streaming(&mut list)?;
    progress("Next: on main with non-dev version, run");
    progress("  gh workflow run release.yml --ref main -f mode=publish -f lanes=github");
    Ok(())
}

fn resolve_p12_b64(args: &BootstrapSecretsArgs) -> Result<String> {
    if let Ok(v) = env::var("DEVELOPER_ID_APPLICATION_P12_BASE64")
        && !v.is_empty()
    {
        return Ok(v);
    }
    let mut path = args.p12.clone();
    let mut tmp_holder: Option<PathBuf> = None;
    if path.is_none()
        && let Some(op_ref) = args.op_p12.as_ref()
    {
        let tmp = tempfile_dir("jackin-bootstrap-p12")?;
        let file = tmp.join("cert.p12");
        let body = op_read(op_ref)?;
        fs::write(&file, body)?;
        tmp_holder = Some(tmp);
        path = Some(file);
    }
    let path = path.context("provide --p12, --op-p12, or DEVELOPER_ID_APPLICATION_P12_BASE64")?;
    if !path.is_file() {
        bail!("p12 not found: {}", path.display());
    }
    let b64 = base64_file(&path)?;
    if let Some(tmp) = tmp_holder {
        drop(fs::remove_dir_all(tmp));
    }
    Ok(b64)
}

fn resolve_p12_password(args: &BootstrapSecretsArgs) -> Result<String> {
    if let Ok(v) = env::var("DEVELOPER_ID_APPLICATION_P12_PASSWORD")
        && !v.is_empty()
    {
        return Ok(v);
    }
    if let Some(p) = args.p12_password.as_ref() {
        return Ok(p.clone());
    }
    if let Some(name) = args.p12_password_env.as_ref() {
        return env::var(name).with_context(|| format!("env {name} empty for --p12-password-env"));
    }
    if let Some(op_ref) = args.op_p12_password.as_ref() {
        return op_read(op_ref);
    }
    bail!(
        "provide --p12-password, --p12-password-env, --op-p12-password, or DEVELOPER_ID_APPLICATION_P12_PASSWORD"
    )
}

fn resolve_p8(args: &BootstrapSecretsArgs) -> Result<String> {
    if let Ok(v) = env::var("APP_STORE_CONNECT_API_KEY_P8")
        && !v.is_empty()
    {
        return Ok(v);
    }
    if let Some(path) = args.p8.as_ref() {
        return fs::read_to_string(path).with_context(|| format!("reading p8 {}", path.display()));
    }
    if let Some(op_ref) = args.op_p8.as_ref() {
        return op_read(op_ref);
    }
    bail!("provide --p8, --op-p8, or APP_STORE_CONNECT_API_KEY_P8")
}

fn op_read(reference: &str) -> Result<String> {
    which("op").context("1Password CLI (op) required for op:// refs")?;
    let mut op = cmd_command("op");
    op.args(["read", reference]);
    crate::cmd::output_string(&mut op).with_context(|| format!("op read {reference}"))
}

fn base64_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    // Match shell `base64 | tr -d '\n'` — no line wraps.
    Ok(base64_nopad_std(&bytes))
}

fn base64_nopad_std(bytes: &[u8]) -> String {
    // Standard base64 without newlines (no external crate — simple encoder).
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(T[((n >> 18) & 0x3f) as usize] as char);
        out.push(T[((n >> 12) & 0x3f) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(T[((n >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(T[(n & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

fn derive_cert_sha256_from_p12(p12: &Path, pass: &str) -> Result<String> {
    which("openssl").context("openssl required to derive cert fingerprint")?;
    let tmp = tempfile_dir("jackin-p12-fp")?;
    let pem = tmp.join("cert.pem");
    let mut extract = Command::new("openssl");
    extract
        .args([
            "pkcs12",
            "-in",
            p12.to_str().context("p12 utf-8")?,
            "-clcerts",
            "-nokeys",
            "-passin",
            &format!("pass:{pass}"),
            "-out",
            pem.to_str().context("pem utf-8")?,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let status = extract.status().context("openssl pkcs12")?;
    if !status.success() {
        drop(fs::remove_dir_all(&tmp));
        bail!("openssl pkcs12 extract failed");
    }
    let mut x509 = cmd_command("openssl");
    x509.args([
        "x509",
        "-in",
        pem.to_str().context("pem utf-8")?,
        "-fingerprint",
        "-sha256",
        "-noout",
    ]);
    let line = crate::cmd::output_string(&mut x509)?;
    drop(fs::remove_dir_all(&tmp));
    let value = match line.split_once('=') {
        Some((_, v)) => v,
        None => line.trim(),
    };
    let hex = value
        .chars()
        .filter(|c| *c != ':')
        .collect::<String>()
        .to_ascii_lowercase();
    Ok(hex)
}

fn ensure_environment(repo: &str) -> Result<()> {
    let mut gh = Command::new("gh");
    gh.args([
        "api",
        "-X",
        "PUT",
        &format!("repos/{repo}/environments/{ENV_NAME}"),
        "--input",
        "-",
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::piped());
    let mut child = gh.spawn().context("spawning gh api")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(
            br#"{"wait_timer":0,"prevent_self_review":false,"reviewers":[],"deployment_branch_policy":null}"#,
        )?;
    }
    let status = child.wait().context("gh api environment")?;
    if !status.success() {
        bail!("failed to ensure environment {ENV_NAME}");
    }
    Ok(())
}

fn gh_secret_set(repo: &str, name: &str, value: &str) -> Result<()> {
    let mut gh = Command::new("gh");
    gh.args(["secret", "set", name, "--repo", repo, "--env", ENV_NAME])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = gh
        .spawn()
        .with_context(|| format!("gh secret set {name}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(value.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        bail!("gh secret set {name} failed");
    }
    Ok(())
}

fn gh_variable_set(repo: &str, name: &str, value: &str) -> Result<()> {
    let mut gh = Command::new("gh");
    gh.args(["variable", "set", name, "--repo", repo])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = gh
        .spawn()
        .with_context(|| format!("gh variable set {name}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(value.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        bail!("gh variable set {name} failed");
    }
    Ok(())
}

fn cmd_command(program: &str) -> Command {
    Command::new(program)
}

#[cfg(test)]
mod tests;
