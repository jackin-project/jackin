// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::super::UsageStatuslineScopeArgs;
use super::*;
use serde_json::json;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn fixture(
    temp: &TempDir,
    settings: Value,
    binary_body: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let settings_path = temp.path().join("settings.json");
    std::fs::write(&settings_path, serde_json::to_vec(&settings).unwrap()).unwrap();

    let binary_path = temp.path().join("fake jackin");
    std::fs::write(&binary_path, binary_body).unwrap();
    let mut permissions = std::fs::metadata(&binary_path).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&binary_path, permissions).unwrap();
    (settings_path, binary_path)
}

fn session_scope() -> UsageStatuslineScopeArgs {
    UsageStatuslineScopeArgs {
        session_only: true,
        binding: None,
        binding_revision: None,
    }
}

fn binding_scope(binding: &str) -> UsageStatuslineScopeArgs {
    UsageStatuslineScopeArgs {
        session_only: false,
        binding: Some(binding.to_owned()),
        binding_revision: Some(7),
    }
}

fn run_composed(shell_command: &str, input: &[u8], env: &[(&str, &Path)]) -> std::process::Output {
    let mut launcher = Command::new("/bin/sh");
    launcher
        .arg("-c")
        .arg(shell_command)
        .env("SHELL", "/bin/sh")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());
    for (key, path) in env {
        launcher.env(key, path);
    }
    let mut child = launcher.spawn().unwrap();
    child.stdin.as_mut().unwrap().write_all(input).unwrap();
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

#[test]
fn preserves_settings_and_statusline_options() {
    let temp = TempDir::new().unwrap();
    let original = json!({
        "theme": "dark",
        "env": {"API_SECRET": "statusline-secret-sentinel"},
        "statusLine": {
            "type": "command",
            "command": "printf 'old output'",
            "padding": 3,
            "refreshInterval": 10,
            "futureOption": {"kept": true}
        }
    });
    let (settings, binary) = fixture(&temp, original.clone(), "#!/bin/sh\nexit 0\n");
    let result = compose(
        &settings,
        &binary,
        &session_scope(),
        &temp.path().join("data"),
    )
    .unwrap();

    let result_object = result.as_object().expect("merge patch should be an object");
    assert_eq!(result_object.len(), 1);
    assert!(result_object.contains_key("statusLine"));
    let result_text = serde_json::to_string(&result).unwrap();
    assert!(!result_text.contains("statusline-secret-sentinel"));
    assert!(!result_text.contains("theme"));
    assert_eq!(result["statusLine"]["type"], original["statusLine"]["type"]);
    assert_eq!(
        result["statusLine"]["padding"],
        original["statusLine"]["padding"]
    );
    assert_eq!(
        result["statusLine"]["refreshInterval"],
        original["statusLine"]["refreshInterval"]
    );
    assert_eq!(
        result["statusLine"]["futureOption"],
        original["statusLine"]["futureOption"]
    );
    assert_ne!(
        result["statusLine"]["command"],
        original["statusLine"]["command"]
    );
    assert_eq!(
        std::fs::read(&settings).unwrap(),
        serde_json::to_vec(&original).unwrap(),
        "compose must not mutate the settings file"
    );
}

#[test]
fn adds_a_statusline_for_initial_setup_without_rendering_output() {
    let temp = TempDir::new().unwrap();
    let original = json!({"theme": "dark"});
    let args_path = temp.path().join("ingress.args");
    let (settings, binary) = fixture(
        &temp,
        original.clone(),
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ARGS_CAPTURE\"\ncat > \"$INPUT_CAPTURE\"\n",
    );
    let data_dir = temp.path().join("data");
    let proposed = compose(&settings, &binary, &session_scope(), &data_dir).unwrap();
    let input = br#"{"session_id":"session-1"}"#;
    let input_path = temp.path().join("ingress.json");
    let output = run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        input,
        &[("INPUT_CAPTURE", &input_path), ("ARGS_CAPTURE", &args_path)],
    );

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(input_path).unwrap(), input);
    let args = std::fs::read_to_string(args_path).unwrap();
    assert!(args.contains("--session-only\n"));
    assert!(!args.contains("--binding\n"));
    assert!(!args.contains("--account\n"));
    assert_eq!(proposed.as_object().unwrap().len(), 1);
    assert!(proposed.get("theme").is_none());
    assert_eq!(proposed["statusLine"]["type"], "command");
    assert_eq!(
        std::fs::read(&settings).unwrap(),
        serde_json::to_vec(&original).unwrap(),
        "compose must not mutate the settings file"
    );
}

#[test]
fn proposes_initial_settings_when_settings_file_is_absent() {
    let temp = TempDir::new().unwrap();
    let settings = temp.path().join("claude/settings.json");
    let binary = temp.path().join("jackin");
    std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&binary, permissions).unwrap();

    let proposed = compose(
        &settings,
        &binary,
        &session_scope(),
        &temp.path().join("data"),
    )
    .unwrap();

    assert_eq!(proposed["statusLine"]["type"], "command");
    assert!(
        !settings.exists(),
        "compose must not create the settings file"
    );
    assert!(!settings.parent().unwrap().exists());
}

#[test]
fn forwards_original_payload_and_output_once_and_quotes_arguments() {
    let temp = TempDir::new().unwrap();
    let input_path = temp.path().join("ingress input.json");
    let args_path = temp.path().join("ingress args.txt");
    let legacy = "cat; printf '\\nlegacy-tail\\n'";
    let original = json!({"statusLine": {"type": "command", "command": legacy}});
    let (settings, binary) = fixture(
        &temp,
        original,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ARGS_CAPTURE\"\ncat > \"$INPUT_CAPTURE\"\nprintf 'ingress-output-must-not-leak\\n'\nexit 7\n",
    );
    let injection_path = temp.path().join("statusline-injection");
    let binding = format!("binding ' ; touch {}", injection_path.display());
    let data_dir = temp.path().join("data directory");
    let proposed = compose(&settings, &binary, &binding_scope(&binding), &data_dir).unwrap();
    let input = br#"{"session_id":"session-1","rate_limits":{"five_hour":{"used_percentage":18,"resets_at":2000000000}}}"#;
    let output = run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        input,
        &[("INPUT_CAPTURE", &input_path), ("ARGS_CAPTURE", &args_path)],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = input.to_vec();
    expected.extend_from_slice(b"\nlegacy-tail\n");
    assert_eq!(output.stdout, expected);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("ingress-output-must-not-leak"));
    assert_eq!(std::fs::read(&input_path).unwrap(), input);
    let args = std::fs::read_to_string(args_path).unwrap();
    assert!(args.contains("--binding\n"));
    assert!(args.contains(&binding));
    assert!(args.contains("--binding-revision\n7\n"));
    assert!(args.contains("--data-dir\n"));
    assert!(args.contains(data_dir.to_str().unwrap()));
    assert!(args.contains("--format\njson\n"));
    assert!(!injection_path.exists());
}

#[test]
fn oversized_payload_reaches_legacy_without_ingress() {
    let temp = TempDir::new().unwrap();
    let input_path = temp.path().join("ingress.json");
    let args_path = temp.path().join("ingress.args");
    let original = json!({"statusLine": {"type": "command", "command": "cat"}});
    let (settings, binary) = fixture(
        &temp,
        original,
        "#!/bin/sh\necho called > \"$ARGS_CAPTURE\"\ncat > \"$INPUT_CAPTURE\"\n",
    );
    let data_dir = temp.path().join("data");
    let proposed = compose(&settings, &binary, &session_scope(), &data_dir).unwrap();
    let input = vec![b'x'; MAX_INPUT_BYTES + 1];
    let output = run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        &input,
        &[("INPUT_CAPTURE", &input_path), ("ARGS_CAPTURE", &args_path)],
    );

    assert!(output.status.success());
    assert_eq!(output.stdout, input);
    assert!(!args_path.exists(), "oversized input must skip ingress");
    assert!(!input_path.exists());
}

#[test]
fn malformed_small_payload_does_not_replace_legacy_output() {
    let temp = TempDir::new().unwrap();
    let args_path = temp.path().join("ingress.args");
    let original = json!({"statusLine": {"type": "command", "command": "printf 'legacy\\n'"}});
    let (settings, binary) = fixture(
        &temp,
        original,
        "#!/bin/sh\necho called > \"$ARGS_CAPTURE\"\nprintf 'broken-json'\nexit 2\n",
    );
    let data_dir = temp.path().join("data");
    let proposed = compose(&settings, &binary, &session_scope(), &data_dir).unwrap();
    let output = run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        b"{broken",
        &[("ARGS_CAPTURE", &args_path)],
    );

    assert!(output.status.success());
    assert_eq!(output.stdout, b"legacy\n");
    assert!(
        args_path.exists(),
        "small malformed input reaches the parser"
    );
}

#[test]
fn rejects_settings_that_cannot_be_safely_composed() {
    let temp = TempDir::new().unwrap();
    for (settings_value, expected_error) in [
        (
            json!({"statusLine": {"type": "prompt"}}),
            "statusLine.type must be `command`",
        ),
        (
            json!({"statusLine": {"type": "command"}}),
            "statusLine.command must be a string",
        ),
    ] {
        let (settings, binary) = fixture(&temp, settings_value, "#!/bin/sh\nexit 0\n");
        let error = compose(
            &settings,
            &binary,
            &session_scope(),
            &temp.path().join("data"),
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected_error));
    }

    let nul_command = json!({"statusLine": {"type": "command", "command": "printf\0bad"}});
    let (settings, binary) = fixture(&temp, nul_command, "#!/bin/sh\nexit 0\n");
    let error = compose(
        &settings,
        &binary,
        &session_scope(),
        &temp.path().join("data"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot contain a NUL byte"));
}

#[test]
fn rejects_statusline_scope_without_exactly_one_selection() {
    let temp = TempDir::new().unwrap();
    for (scope, expected) in [
        (
            UsageStatuslineScopeArgs {
                session_only: false,
                binding: None,
                binding_revision: None,
            },
            "--session-only",
        ),
        (
            UsageStatuslineScopeArgs {
                session_only: false,
                binding: Some("binding".to_owned()),
                binding_revision: None,
            },
            "--binding-revision",
        ),
        (
            UsageStatuslineScopeArgs {
                session_only: true,
                binding: Some("binding".to_owned()),
                binding_revision: Some(1),
            },
            "--session-only",
        ),
    ] {
        let settings = temp.path().join("settings.json");
        std::fs::write(&settings, b"{}").unwrap();
        let binary = temp.path().join("jackin");
        std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
        let error = compose(&settings, &binary, &scope, &temp.path().join("data")).unwrap_err();
        assert!(error.to_string().contains(expected));
    }
}

#[test]
fn rejects_long_commands_and_quote_expansion_without_changing_settings() {
    let temp = TempDir::new().unwrap();
    let binary = temp.path().join("jackin");
    std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&binary, permissions).unwrap();

    for (command, expected_error) in [
        ("'".repeat(MAX_LEGACY_COMMAND_BYTES), "64 KiB"),
        ("x".repeat(MAX_LEGACY_COMMAND_BYTES + 1), "16 KiB"),
    ] {
        let original = json!({"statusLine": {"type": "command", "command": command}});
        let settings = temp.path().join("settings.json");
        let original_bytes = serde_json::to_vec(&original).unwrap();
        std::fs::write(&settings, &original_bytes).unwrap();

        let error = compose(
            &settings,
            &binary,
            &session_scope(),
            &temp.path().join("data"),
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected_error));
        assert_eq!(std::fs::read(&settings).unwrap(), original_bytes);
    }
}

#[test]
fn preserves_legacy_failure_status_and_skips_ingestion() {
    let temp = TempDir::new().unwrap();
    let args_path = temp.path().join("ingress.args");
    let (settings, binary) = fixture(
        &temp,
        json!({"statusLine": {"type": "command", "command": "printf 'legacy\\n'; exit 7"}}),
        "#!/bin/sh\necho called > \"$ARGS_CAPTURE\"\n",
    );
    let proposed = compose(
        &settings,
        &binary,
        &session_scope(),
        &temp.path().join("data"),
    )
    .unwrap();
    let output = run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        b"payload",
        &[("ARGS_CAPTURE", &args_path)],
    );

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"legacy\n");
    assert!(
        !args_path.exists(),
        "failed legacy status must skip ingestion"
    );
}

#[test]
fn runs_legacy_command_without_ingestion_when_python_is_unavailable() {
    let temp = TempDir::new().unwrap();
    let args_path = temp.path().join("ingress.args");
    let (settings, binary) = fixture(
        &temp,
        json!({"statusLine": {"type": "command", "command": "printf legacy"}}),
        "#!/bin/sh\necho called > \"$ARGS_CAPTURE\"\n",
    );
    let path = temp.path().join("path-with-shell-only");
    std::fs::create_dir(&path).unwrap();
    std::os::unix::fs::symlink("/bin/sh", path.join("sh")).unwrap();
    let proposed = compose(
        &settings,
        &binary,
        &session_scope(),
        &temp.path().join("data"),
    )
    .unwrap();
    let output = run_composed(
        proposed["statusLine"]["command"].as_str().unwrap(),
        b"payload",
        &[("PATH", &path), ("ARGS_CAPTURE", &args_path)],
    );

    assert!(output.status.success());
    assert_eq!(output.stdout, b"legacy");
    assert!(!args_path.exists(), "missing Python must skip ingestion");
}

#[test]
fn rejects_malformed_and_oversized_settings_without_unbounded_reads() {
    let temp = TempDir::new().unwrap();
    let settings = temp.path().join("settings.json");
    let binary = temp.path().join("jackin");
    std::fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&binary, permissions).unwrap();

    std::fs::write(&settings, b"{").unwrap();
    let error = compose(
        &settings,
        &binary,
        &session_scope(),
        &temp.path().join("data"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("parse Claude Code settings"));

    std::fs::write(&settings, vec![b' '; MAX_SETTINGS_BYTES + 1]).unwrap();
    let error = compose(
        &settings,
        &binary,
        &session_scope(),
        &temp.path().join("data"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("1 MiB composition limit"));
}
