//! Container context: identity metadata (container name, instance ID,
//! diagnostics) about the running container, available to the daemon.
//!
//! Not responsible for: role/workspace config (see `config`) or attach-session
//! state (see `attach_context`).

use jackin_core::constants::instance_id_from_container_base as instance_id_from_container_name;

pub const JACKIN_CONTAINER_NAME_ENV: &str = "JACKIN_CONTAINER_NAME";
pub const JACKIN_INSTANCE_ID_ENV: &str = "JACKIN_INSTANCE_ID";

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
    let (run_log_display, run_log_href) = if run_id.is_empty() {
        ("(not set)".to_owned(), None)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_owned());
        let full_path = format!("{home}/.jackin/data/diagnostics/runs/{run_id}.jsonl");
        (
            format!("~/.jackin/data/diagnostics/runs/{run_id}.jsonl"),
            Some(format!("file://{full_path}")),
        )
    };
    ContainerDiagnostics {
        host_version,
        run_id,
        run_log_display,
        run_log_href,
    }
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
