// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Fixed-allowlist OpenTelemetry Resource construction.

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;

use super::ServiceIdentity;

static PROCESS_INSTANCE_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub(super) fn build_resource_for(identity: ServiceIdentity) -> Resource {
    build_resource_for_sources(identity, &|| {
        std::fs::read_to_string("/proc/self/cgroup").ok()
    })
}

pub(super) fn build_resource_for_sources(
    identity: ServiceIdentity,
    cgroup: &impl Fn() -> Option<String>,
) -> Resource {
    use jackin_telemetry::schema::attrs::{self, std_attrs};

    let executable_name = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| identity.service_name.to_owned());
    let mut attributes = vec![
        KeyValue::new(std_attrs::SERVICE_NAMESPACE, "jackin"),
        KeyValue::new(std_attrs::SERVICE_NAME, identity.service_name),
        KeyValue::new(std_attrs::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
        KeyValue::new(
            std_attrs::SERVICE_INSTANCE_ID,
            PROCESS_INSTANCE_ID
                .get_or_init(|| uuid::Uuid::new_v4().to_string())
                .clone(),
        ),
        KeyValue::new(std_attrs::PROCESS_PID, i64::from(std::process::id())),
        KeyValue::new(std_attrs::PROCESS_EXECUTABLE_NAME, executable_name),
        KeyValue::new(attrs::APP_MODE, identity.app_mode.as_str()),
        KeyValue::new(std_attrs::PROCESS_RUNTIME_NAME, "rust"),
    ];
    if let Some(os_type) = semantic_os_type(std::env::consts::OS) {
        attributes.push(KeyValue::new(std_attrs::OS_TYPE, os_type));
    }
    if let Some(version) = sysinfo::System::long_os_version() {
        attributes.push(KeyValue::new(std_attrs::OS_VERSION, version));
    }
    attributes.push(KeyValue::new(
        std_attrs::PROCESS_RUNTIME_VERSION,
        env!("RUSTC_VERSION"),
    ));
    // `/etc/hostname` is intentionally not consulted: project container paths
    // exclude `/etc`, and the environment is operator-controlled. The kernel
    // cgroup record is the sole trusted best-effort provenance for this field.
    if identity == ServiceIdentity::CAPSULE
        && let Some(cgroup) = cgroup()
        && let Some(container_id) = container_id_from_cgroup(&cgroup)
    {
        attributes.push(KeyValue::new(std_attrs::CONTAINER_ID, container_id));
    }
    Resource::builder_empty()
        .with_attributes(attributes)
        .build()
}

pub(super) fn semantic_os_type(target_os: &'static str) -> Option<&'static str> {
    match target_os {
        "aix" => Some("aix"),
        "macos" | "ios" | "tvos" | "watchos" | "visionos" => Some("darwin"),
        "dragonfly" => Some("dragonflybsd"),
        "freebsd" => Some("freebsd"),
        "hpux" => Some("hpux"),
        "android" | "linux" => Some("linux"),
        "netbsd" => Some("netbsd"),
        "openbsd" => Some("openbsd"),
        "illumos" | "solaris" => Some("solaris"),
        "windows" => Some("windows"),
        "zos" => Some("z_os"),
        _ => None,
    }
}

pub(super) fn container_id_from_cgroup(value: &str) -> Option<String> {
    value
        .split(|character: char| !character.is_ascii_hexdigit())
        .find_map(verified_container_id)
}

pub(super) fn verified_container_id(value: &str) -> Option<String> {
    let value = value.trim();
    ((12..=64).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}
