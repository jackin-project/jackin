//! Container context: identity metadata (container name, instance ID,
//! diagnostics) about the running container, available to the daemon.
//!
//! Not responsible for: role/workspace config (see `config`) or attach-session
//! state (see `attach_context`).

use jackin_core::constants::instance_id_from_container_base as instance_id_from_container_name;

pub const JACKIN_CONTAINER_NAME_ENV: &str = "JACKIN_CONTAINER_NAME";
pub const JACKIN_INSTANCE_ID_ENV: &str = "JACKIN_INSTANCE_ID";
pub const JACKIN_RUN_DIAGNOSTICS_PATH_ENV: &str = "JACKIN_RUN_DIAGNOSTICS_PATH";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusIdentity {
    pub container_name: String,
    pub instance_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDiagnostics {
    pub host_version: String,
    pub run_id: String,
    pub run_log_display: String,
    pub run_log_href: Option<String>,
}

impl Default for ContainerDiagnostics {
    fn default() -> Self {
        Self {
            host_version: "unknown".to_owned(),
            run_id: String::new(),
            run_log_display: "(not set)".to_owned(),
            run_log_href: None,
        }
    }
}

pub fn resolve_status_identity() -> StatusIdentity {
    let container_name = resolve_container_name();
    let instance_id = resolve_instance_id(&container_name);
    StatusIdentity {
        container_name,
        instance_id,
    }
}

pub fn resolve_container_diagnostics() -> ContainerDiagnostics {
    let host_version =
        std::env::var("JACKIN_HOST_VERSION").unwrap_or_else(|_| "unknown".to_owned());
    let run_id = std::env::var("JACKIN_RUN_ID").unwrap_or_default();
    let diagnostics_path = std::env::var(JACKIN_RUN_DIAGNOSTICS_PATH_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_owned());
    let (run_log_display, run_log_href) =
        resolve_run_log_location(&run_id, diagnostics_path.as_deref(), &home);
    ContainerDiagnostics {
        host_version,
        run_id,
        run_log_display,
        run_log_href,
    }
}

fn resolve_run_log_location(
    run_id: &str,
    diagnostics_path: Option<&str>,
    home: &str,
) -> (String, Option<String>) {
    if run_id.is_empty() {
        return ("(not set)".to_owned(), None);
    }
    if let Some(path) = diagnostics_path {
        return (path.to_owned(), file_href_for_path(path));
    }
    let full_path = format!("{home}/.jackin/data/diagnostics/runs/{run_id}.jsonl");
    (
        format!("~/.jackin/data/diagnostics/runs/{run_id}.jsonl"),
        file_href_for_path(&full_path),
    )
}

fn file_href_for_path(path: &str) -> Option<String> {
    url::Url::from_file_path(path)
        .ok()
        .map(|url| url.to_string())
}

fn resolve_container_name() -> String {
    if let Some(value) = std::env::var(JACKIN_CONTAINER_NAME_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        return value;
    }
    if let Some(value) = std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        crate::clog!("statusbar: container name resolved from HOSTNAME");
        return value;
    }
    const ETC_HOSTNAME_MAX_BYTES: u64 = 256;
    if let Some(value) = crate::util::read_text_bounded(
        "/etc/hostname",
        std::path::Path::new("/etc/hostname"),
        ETC_HOSTNAME_MAX_BYTES,
    )
    .map(|value| value.trim().to_owned())
    .filter(|value| !value.is_empty())
    {
        crate::clog!("statusbar: container name resolved from /etc/hostname");
        return value;
    }
    crate::clog!(
        "statusbar: container name unresolved - {JACKIN_CONTAINER_NAME_ENV}, HOSTNAME, and /etc/hostname all empty or unreadable; chrome chip will be blank"
    );
    String::new()
}

fn resolve_instance_id(container_name: &str) -> String {
    std::env::var(JACKIN_INSTANCE_ID_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            instance_id_from_container_name(container_name)
                .map_or_else(|| container_name.to_owned(), str::to_owned)
        })
}

#[cfg(test)]
mod tests {
    use super::resolve_run_log_location;

    #[test]
    fn diagnostics_path_prefers_host_supplied_path() {
        let (display, href) = resolve_run_log_location(
            "jk-run-abc123",
            Some("/Users/operator/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl"),
            "/home/agent",
        );

        assert_eq!(
            display,
            "/Users/operator/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl"
        );
        assert_eq!(
            href.as_deref(),
            Some("file:///Users/operator/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl")
        );
    }

    #[test]
    fn diagnostics_path_falls_back_to_container_home_for_older_launches() {
        let (display, href) = resolve_run_log_location("jk-run-abc123", None, "/home/agent");

        assert_eq!(
            display,
            "~/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl"
        );
        assert_eq!(
            href.as_deref(),
            Some("file:///home/agent/.jackin/data/diagnostics/runs/jk-run-abc123.jsonl")
        );
    }
}
