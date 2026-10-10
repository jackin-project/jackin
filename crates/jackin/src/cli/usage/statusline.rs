// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Compose Claude Code's existing statusline command with bounded local usage
//! observation. This module only proposes settings JSON; it never writes the
//! settings file or starts Claude Code.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

pub(super) const MAX_INPUT_BYTES: usize = 16 * 1024;
pub(super) const MAX_SETTINGS_BYTES: usize = 1024 * 1024;
pub(super) const MAX_LEGACY_COMMAND_BYTES: usize = 16 * 1024;
const MAX_COMPOSED_COMMAND_BYTES: usize = 64 * 1024;

/// Add bounded usage ingestion after a Claude Code statusline command.
///
/// The returned JSON merge patch contains only the composed `statusLine`,
/// preserving its existing properties and command semantics. Operators must
/// merge this property into the existing settings rather than replace the file
/// with the patch. If settings have no `statusLine`, the patch adds a command
/// that consumes input without rendering output before ingesting. This function
/// fails closed when it cannot preserve an existing command.
#[expect(
    clippy::disallowed_methods,
    reason = "bounded synchronous settings input is read only by this operator CLI, outside render/runtime threads"
)]
pub(super) fn compose(
    settings: &Path,
    binary: &Path,
    scope: &super::UsageStatuslineScopeArgs,
    data_dir: &Path,
) -> Result<Value> {
    let settings_bytes = match std::fs::File::open(settings) {
        Ok(settings_file) => {
            let mut settings_bytes = Vec::with_capacity(MAX_SETTINGS_BYTES + 1);
            settings_file
                .take((MAX_SETTINGS_BYTES + 1) as u64)
                .read_to_end(&mut settings_bytes)
                .with_context(|| format!("read Claude Code settings at {}", settings.display()))?;
            if settings_bytes.len() > MAX_SETTINGS_BYTES {
                bail!("Claude Code settings exceed the 1 MiB composition limit");
            }
            settings_bytes
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => b"{}".to_vec(),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("open Claude Code settings at {}", settings.display()));
        }
    };
    let mut value: Value = serde_json::from_slice(&settings_bytes)
        .with_context(|| format!("parse Claude Code settings at {}", settings.display()))?;
    let root = value
        .as_object_mut()
        .context("Claude Code settings must be a JSON object")?;
    let legacy_command = match root.get("statusLine") {
        None => "cat >/dev/null".to_owned(),
        Some(status_line) => {
            let status_line = status_line
                .as_object()
                .context("Claude Code settings statusLine must be a command object")?;
            if status_line.get("type").and_then(Value::as_str) != Some("command") {
                bail!("Claude Code statusLine.type must be `command`");
            }
            status_line
                .get("command")
                .and_then(Value::as_str)
                .context("Claude Code statusLine.command must be a string")?
                .to_owned()
        }
    };
    if legacy_command.contains('\0') {
        bail!("Claude Code statusLine.command cannot contain a NUL byte");
    }
    if legacy_command.len() > MAX_LEGACY_COMMAND_BYTES {
        bail!("Claude Code statusLine.command exceeds the 16 KiB composition limit");
    }

    let binary = binary
        .to_str()
        .context("jackin binary path must be valid UTF-8")?;
    let data_dir = data_dir
        .to_str()
        .context("jackin data directory path must be valid UTF-8")?;
    let (scope_mode, binding_id, binding_revision) = if scope.session_only {
        if scope.binding.is_some() || scope.binding_revision.is_some() {
            bail!("choose exactly `--session-only` or `--binding` with `--binding-revision`");
        }
        ("session-only", "", String::new())
    } else {
        let binding_id = scope
            .binding
            .as_deref()
            .context("choose `--session-only` or `--binding`")?;
        let binding_revision = scope
            .binding_revision
            .context("`--binding` requires `--binding-revision`")?;
        if binding_id.trim().is_empty() || binding_id.contains('\0') {
            bail!("statusline binding id must be nonempty and contain no NUL byte");
        }
        ("binding", binding_id, binding_revision.to_string())
    };

    let composed_command = wrapper_command(
        &legacy_command,
        binary,
        scope_mode,
        binding_id,
        &binding_revision,
        data_dir,
    );
    if composed_command.len() > MAX_COMPOSED_COMMAND_BYTES {
        bail!("composed Claude Code statusLine.command exceeds the 64 KiB limit");
    }
    if let Some(status_line) = root.get_mut("statusLine") {
        status_line
            .as_object_mut()
            .context("Claude Code settings statusLine must be a command object")?
            .insert("command".to_owned(), Value::String(composed_command));
    } else {
        root.insert(
            "statusLine".to_owned(),
            serde_json::json!({
                "type": "command",
                "command": composed_command,
            }),
        );
    }
    let status_line = root
        .get("statusLine")
        .cloned()
        .context("composed Claude Code statusLine is missing")?;
    Ok(serde_json::json!({ "statusLine": status_line }))
}

fn wrapper_command(
    command: &str,
    binary: &str,
    scope_mode: &str,
    binding_id: &str,
    binding_revision: &str,
    data_dir: &str,
) -> String {
    let wrapper = PYTHON_WRAPPER.replace("__MAX_INPUT_BYTES__", &MAX_INPUT_BYTES.to_string());

    format!(
        "sh -c {} jackin-statusline {} {} {} {} {} {} {}",
        shell_quote(
            "if command -v python3 >/dev/null 2>&1; then python3 -I -S -c \"$1\" \"$2\" \"$3\" \"$4\" \"$5\" \"$6\" \"$7\"; else \"${SHELL:-/bin/sh}\" -c \"$2\"; fi"
        ),
        shell_quote(&wrapper),
        shell_quote(command),
        shell_quote(binary),
        shell_quote(scope_mode),
        shell_quote(binding_id),
        shell_quote(binding_revision),
        shell_quote(data_dir),
    )
}

const PYTHON_WRAPPER: &str = r#"
import os
import subprocess
import sys
import threading

MAX_INPUT_BYTES = __MAX_INPUT_BYTES__
CAPTURE_LIMIT = MAX_INPUT_BYTES + 1
legacy_command, jackin_binary, scope_mode, binding_id, binding_revision, data_dir = sys.argv[1:7]
shell_path = os.environ.get("SHELL") or "/bin/sh"

try:
    legacy = subprocess.Popen(
        [shell_path, "-c", legacy_command],
        stdin=subprocess.PIPE,
        bufsize=0,
    )
except OSError:
    sys.exit(127)

captured = bytearray()
forward_to_legacy = True

def relay_input():
    global forward_to_legacy
    while True:
        chunk = sys.stdin.buffer.read(8192)
        if not chunk:
            break
        remaining = CAPTURE_LIMIT - len(captured)
        if remaining > 0:
            captured.extend(chunk[:remaining])
        if forward_to_legacy:
            view = memoryview(chunk)
            while view:
                try:
                    written = legacy.stdin.write(view)
                    if written is None or written <= 0:
                        raise BrokenPipeError()
                    view = view[written:]
                except (BrokenPipeError, OSError):
                    forward_to_legacy = False
                    break
    if forward_to_legacy:
        try:
            legacy.stdin.close()
        except (BrokenPipeError, OSError):
            pass

relay = threading.Thread(target=relay_input)
relay.start()
legacy_status = legacy.wait()
relay.join()
if legacy_status != 0:
    sys.exit(legacy_status if legacy_status > 0 else 128 - legacy_status)

if len(captured) <= MAX_INPUT_BYTES:
    if scope_mode == "session-only":
        scope_args = ["--session-only"]
    else:
        scope_args = ["--binding", binding_id, "--binding-revision", binding_revision]
    try:
        subprocess.run(
            [
                jackin_binary,
                "usage",
                "statusline",
                "ingest",
                *scope_args,
                "--format",
                "json",
                "--data-dir",
                data_dir,
            ],
            input=bytes(captured),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=2,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        pass
sys.exit(0)
"#;

/// Quote one argument for the POSIX-compatible shell command Claude Code runs.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
