use std::path::Path;
use std::process::{Command as StdCommand, Stdio};

use anyhow::{Context, Result};

pub(super) fn open_host_url(url: &str) -> Result<()> {
    let (program, args) =
        host_open_command(url).ok_or_else(|| anyhow::anyhow!("unsupported URL or host OS"))?;
    let redacted = jackin_core::url_text::redact_url_for_log(url);
    run_host_desktop_command(program, args, "host URL opener")
        .with_context(|| format!("starting host URL opener for {redacted:?}"))
}

pub(super) fn reveal_host_file(path: &Path) -> Result<()> {
    let (program, args) =
        host_reveal_command(path).ok_or_else(|| anyhow::anyhow!("unsupported host OS"))?;
    run_host_desktop_command(program, args, "host file reveal").context("starting host file reveal")
}

pub(super) fn open_host_file(path: &Path) -> Result<()> {
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

pub(super) fn host_reveal_command(path: &Path) -> Option<(&'static str, Vec<String>)> {
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

pub(super) fn host_file_open_command(path: &Path) -> Option<(&'static str, Vec<String>)> {
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

pub(super) fn host_open_command(url: &str) -> Option<(&'static str, Vec<String>)> {
    let open_links = std::env::var(jackin_core::env_model::JACKIN_OPEN_LINKS_ENV_NAME).ok();
    host_open_command_with_policy(url, open_links.as_deref())
}

pub(super) fn host_open_command_with_policy(
    url: &str,
    open_links: Option<&str>,
) -> Option<(&'static str, Vec<String>)> {
    if !jackin_core::env_model::open_links_allowed(open_links) {
        return None;
    }
    if !jackin_core::url_text::is_host_open_url(url) {
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
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn host_open_command_rejects_non_http_urls() {
        assert!(host_open_command("file:///tmp/report.html").is_none());
        assert!(host_open_command("javascript:alert(1)").is_none());
    }

    #[test]
    fn host_open_command_accepts_http_urls() {
        let Some((_program, args)) = host_open_command_with_policy(
            "https://github.com/jackin-project/jackin/actions/runs/1",
            None,
        ) else {
            panic!("http(s) URL should produce a host opener command on supported test platforms");
        };
        assert!(args.iter().any(|arg| arg.contains("github.com")));
    }

    #[test]
    fn host_open_command_accepts_mailto_urls() {
        let Some((_program, args)) =
            host_open_command_with_policy("mailto:operator@example.com", None)
        else {
            panic!("mailto URL should produce a host opener command on supported test platforms");
        };
        assert!(args.iter().any(|arg| arg == "mailto:operator@example.com"));
    }

    #[test]
    fn host_open_command_honors_open_links_opt_out() {
        assert!(
            host_open_command_with_policy(
                "https://github.com/jackin-project/jackin/actions/runs/1",
                Some("deny"),
            )
            .is_none()
        );
    }

    #[test]
    fn host_reveal_command_matches_current_platform() {
        let path = Path::new("/tmp/jackin/report.txt");
        let command = host_reveal_command(path).expect("current platform should support reveal");

        if cfg!(target_os = "macos") {
            assert_eq!(command.0, "open");
            assert_eq!(command.1, vec!["-R", "/tmp/jackin/report.txt"]);
        } else if cfg!(target_os = "linux") {
            assert_eq!(command.0, "xdg-open");
            assert_eq!(command.1, vec!["/tmp/jackin"]);
        } else if cfg!(target_os = "windows") {
            assert_eq!(command.0, "explorer.exe");
            assert_eq!(command.1, vec!["/select,/tmp/jackin/report.txt"]);
        }
    }

    #[test]
    fn host_file_open_command_matches_current_platform() {
        let path = Path::new("/tmp/jackin/report.txt");
        let command = host_file_open_command(path).expect("current platform should support open");

        if cfg!(target_os = "macos") {
            assert_eq!(command.0, "open");
            assert_eq!(command.1, vec!["/tmp/jackin/report.txt"]);
        } else if cfg!(target_os = "linux") {
            assert_eq!(command.0, "xdg-open");
            assert_eq!(command.1, vec!["/tmp/jackin/report.txt"]);
        } else if cfg!(target_os = "windows") {
            assert_eq!(command.0, "explorer.exe");
            assert_eq!(command.1, vec!["/tmp/jackin/report.txt"]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn host_desktop_command_reports_nonzero_exit() {
        run_host_desktop_command("/usr/bin/env", vec!["true".to_owned()], "test opener")
            .expect("successful command should pass");

        let err = run_host_desktop_command("/usr/bin/env", vec!["false".to_owned()], "test opener")
            .expect_err("nonzero command should fail");

        assert!(err.to_string().contains("test opener command"));
    }
}
