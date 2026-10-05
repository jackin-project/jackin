// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Read-only evidence from the production Capsule launched by the e2e suite.

use std::path::Path;
use std::process::Command;

use jackin_core::container_paths::{CAPSULE_BIN, USAGE_SOCK, path_is_ancestor_or_equal};

/// Prove the running supervisor is the requested binary, the scoped relay is
/// live, and no host-wide usage authority was mounted into the role.
pub(super) fn assert_runtime(
    container: &str,
    home: &Path,
    canaries: &[&str],
) -> Result<(), String> {
    let expected_path = std::env::var_os("JACKIN_CAPSULE_BIN")
        .ok_or_else(|| "JACKIN_CAPSULE_BIN is required for production runtime proof".to_owned())?;
    let expected = std::fs::read(&expected_path)
        .map_err(|error| format!("reading expected Capsule binary: {error}"))?;
    let executable = docker_text(
        &["exec", "--user", "0", container, "readlink", "/proc/1/exe"],
        canaries,
    )?;
    if executable.trim() != CAPSULE_BIN {
        return Err(format!(
            "PID 1 executable is {executable:?}, expected {CAPSULE_BIN}"
        ));
    }
    let actual = docker_bytes(
        &["exec", "--user", "0", container, "cat", "/proc/1/exe"],
        canaries,
    )?;
    if actual != expected {
        return Err("PID 1 executable bytes differ from JACKIN_CAPSULE_BIN".to_owned());
    }

    let inspection = docker_text(&["inspect", container], canaries)?;
    assert_no_canaries(&inspection, "container inspection metadata", canaries)?;
    let inspected: serde_json::Value = serde_json::from_str(&inspection)
        .map_err(|error| format!("decoding container inspection: {error}"))?;
    let instance = inspected
        .as_array()
        .and_then(|entries| entries.first())
        .ok_or_else(|| "container inspection returned no instance".to_owned())?;
    let image = instance["Image"]
        .as_str()
        .ok_or_else(|| "container inspection has no image ID".to_owned())?;
    let digest = image.strip_prefix("sha256:").unwrap_or_default();
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("container image is not identified by a sha256 digest".to_owned());
    }
    let mounts = instance["Mounts"]
        .as_array()
        .ok_or_else(|| "container inspection has no mount inventory".to_owned())?;
    let canonical_home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let home_roots = [home, canonical_home.as_path()];
    let protected: Vec<_> = home_roots
        .iter()
        .flat_map(|root| {
            [
                root.to_path_buf(),
                root.join(".jackin"),
                root.join(".config/jackin"),
                root.join(".jackin/usage-broker"),
            ]
        })
        .collect();
    for mount in mounts {
        let source = mount["Source"]
            .as_str()
            .ok_or_else(|| "mount inventory has no source".to_owned())?;
        let destination = mount["Destination"]
            .as_str()
            .ok_or_else(|| "mount inventory has no destination".to_owned())?;
        let canonical_source = Path::new(source)
            .canonicalize()
            .unwrap_or_else(|_| Path::new(source).to_path_buf());
        let source_path = canonical_source.as_path();
        let destination_path = Path::new(destination);
        let authority_exposed = protected
            .iter()
            .any(|path| path_is_ancestor_or_equal(source_path, path))
            || home_roots
                .iter()
                .any(|root| path_is_ancestor_or_equal(&root.join(".config/jackin"), source_path))
            || source_path.components().any(|component| {
                matches!(
                    component.as_os_str().to_str(),
                    Some("usage-broker" | "usage-broker.sock")
                )
            });
        let docker_exposed = [source_path, destination_path].iter().any(|path| {
            path.file_name().is_some_and(|name| name == "docker.sock")
                || path_is_ancestor_or_equal(path, Path::new("/var/run/docker.sock"))
                || path_is_ancestor_or_equal(path, Path::new("/run/docker.sock"))
        });
        if authority_exposed || docker_exposed {
            return Err(redact(
                &format!("forbidden host authority mount: {source} -> {destination}"),
                canaries,
            ));
        }
    }

    let argv = serde_json::to_string(&(
        &instance["Config"]["Entrypoint"],
        &instance["Config"]["Cmd"],
    ))
    .map_err(|error| format!("encoding container command: {error}"))?;
    assert_no_canaries(&argv, "container command arguments", canaries)?;
    let pid1_argv = docker_text(
        &["exec", "--user", "0", container, "cat", "/proc/1/cmdline"],
        canaries,
    )?;
    assert_no_canaries(&pid1_argv, "PID 1 command arguments", canaries)?;
    let logs = docker_bytes(&["logs", container], canaries)?;
    assert_no_canaries(&String::from_utf8_lossy(&logs), "container logs", canaries)?;

    docker_bytes(
        &["exec", "--user", "0", container, "test", "-S", USAGE_SOCK],
        canaries,
    )?;
    let command = format!(
        "for p in /proc/[0-9]*/cmdline; do \
         [ -r \"$p\" ] || continue; \
         tr '\\000' '\\n' < \"$p\" 2>/dev/null | \
         awk 'NR == 1 {{ executable = $0 }} \
         NR == 2 && executable == \"{CAPSULE_BIN}\" && $0 == \"usage-relay-proxy\" {{ print \"live-relay\" }}'; \
         done"
    );
    let relay = docker_text(
        &["exec", "--user", "0", container, "sh", "-c", &command],
        canaries,
    )?;
    if !relay.lines().any(|line| line == "live-relay") {
        return Err("no production Capsule usage-relay-proxy process is live".to_owned());
    }
    Ok(())
}

fn docker_text(args: &[&str], canaries: &[&str]) -> Result<String, String> {
    String::from_utf8(docker_bytes(args, canaries)?)
        .map_err(|error| format!("Docker evidence is not UTF-8: {error}"))
}

fn docker_bytes(args: &[&str], canaries: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("docker")
        .args(args)
        .output()
        .map_err(|error| format!("running Docker evidence probe: {error}"))?;
    if !output.status.success() {
        return Err(redact(
            &format!(
                "Docker evidence probe failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
            canaries,
        ));
    }
    let mut bytes = output.stdout;
    // Docker emits container stderr logs on its own stderr stream.
    if args.first() == Some(&"logs") {
        bytes.extend_from_slice(&output.stderr);
    }
    Ok(bytes)
}

fn assert_no_canaries(text: &str, location: &str, canaries: &[&str]) -> Result<(), String> {
    if canaries
        .iter()
        .any(|canary| !canary.is_empty() && text.contains(canary))
    {
        return Err(format!("credential canary exposed in {location}"));
    }
    Ok(())
}

fn redact(text: &str, canaries: &[&str]) -> String {
    let mut text = text.to_owned();
    for canary in canaries.iter().filter(|canary| !canary.is_empty()) {
        text = text.replace(canary, "<redacted>");
    }
    text
}
