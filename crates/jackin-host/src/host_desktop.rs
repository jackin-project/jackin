use std::path::Path;
use std::process::{Command as StdCommand, Stdio};

use anyhow::{Context, Result};

pub fn open_host_url(url: &str) -> Result<()> {
    let (program, args) =
        host_open_command(url).ok_or_else(|| anyhow::anyhow!("unsupported URL or host OS"))?;
    let redacted = jackin_tui::url_text::redact_url_for_log(url);
    run_host_desktop_command(program, args, "host URL opener")
        .with_context(|| format!("starting host URL opener for {redacted:?}"))
}

pub fn reveal_host_file(path: &Path) -> Result<()> {
    let (program, args) =
        host_reveal_command(path).ok_or_else(|| anyhow::anyhow!("unsupported host OS"))?;
    run_host_desktop_command(program, args, "host file reveal").context("starting host file reveal")
}

pub fn open_host_file(path: &Path) -> Result<()> {
    let (program, args) =
        host_file_open_command(path).ok_or_else(|| anyhow::anyhow!("unsupported host OS"))?;
    run_host_desktop_command(program, args, "host file opener").context("starting host file opener")
}

fn run_host_desktop_command(program: &str, args: Vec<String>, label: &str) -> Result<()> {
    let status = StdCommand::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("running {label} command {program:?}"))?;
    if !status.success() {
        anyhow::bail!("{label} command {program:?} exited with {status}");
    }
    Ok(())
}

pub fn host_reveal_command(path: &Path) -> Option<(&'static str, Vec<String>)> {
    if cfg!(target_os = "macos") {
        Some(("open", vec!["-R".to_owned(), path.display().to_string()]))
    } else if cfg!(target_os = "linux") {
        Some((
            "xdg-open",
            vec![path.parent().unwrap_or(path).display().to_string()],
        ))
    } else if cfg!(target_os = "windows") {
        Some(("explorer.exe", vec![format!("/select,{}", path.display())]))
    } else {
        None
    }
}

pub fn host_file_open_command(path: &Path) -> Option<(&'static str, Vec<String>)> {
    if cfg!(target_os = "macos") {
        Some(("open", vec![path.display().to_string()]))
    } else if cfg!(target_os = "linux") {
        Some(("xdg-open", vec![path.display().to_string()]))
    } else if cfg!(target_os = "windows") {
        Some(("explorer.exe", vec![path.display().to_string()]))
    } else {
        None
    }
}

pub fn host_open_command(url: &str) -> Option<(&'static str, Vec<String>)> {
    let open_links = std::env::var(jackin_core::env_model::JACKIN_OPEN_LINKS_ENV_NAME).ok();
    host_open_command_with_policy(url, open_links.as_deref())
}

pub fn host_open_command_with_policy(
    url: &str,
    open_links: Option<&str>,
) -> Option<(&'static str, Vec<String>)> {
    if !jackin_core::env_model::open_links_allowed(open_links) {
        return None;
    }
    if !jackin_tui::url_text::is_host_open_url(url) {
        return None;
    }
    if cfg!(target_os = "macos") {
        Some(("open", vec![url.to_owned()]))
    } else if cfg!(target_os = "linux") {
        Some(("xdg-open", vec![url.to_owned()]))
    } else if cfg!(target_os = "windows") {
        Some((
            "rundll32",
            vec!["url.dll,FileProtocolHandler".to_owned(), url.to_owned()],
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
