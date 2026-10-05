// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};

// Independent runtime adapters are the identity/install oracle: the dispatch
// must execute a binary actually provisioned for that same agent.
fn execute_entrypoint(script: &str, slug: &str) -> Output {
    let fixture = tempfile::tempdir().expect("entrypoint fixture");
    let mut command = Command::new("bash");
    for agent in Agent::ALL {
        for binary in agent.runtime().container_binary_paths() {
            let name = Path::new(binary).file_name().expect("native executable");
            let stub = fixture.path().join(name);
            std::fs::write(
                &stub,
                format!(
                    "#!/bin/bash\nprintf '%s\\n' '{}'\nprintf '<%s>\\n' \"$@\"\n",
                    agent.slug()
                ),
            )
            .expect("write native CLI stub");
            std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
                .expect("executable CLI stub");
        }
    }
    // Execute the actual complete rendered entrypoint. Only absolute runtime
    // integration locations are relocated to inert fixture paths.
    let script = script
        .replace("/jackin/runtime/jackin-capsule runtime-setup", ":")
        .replace(
            "/jackin/runtime/hooks/",
            &format!("{}/hooks/", fixture.path().display()),
        );
    #[expect(
        clippy::disallowed_methods,
        reason = "process oracle executes the rendered production entrypoint"
    )]
    command
        .args([
            "-c",
            &script,
            "entrypoint",
            "two words",
            "$(must-stay-literal)",
        ])
        .env(
            "PATH",
            format!(
                "{}:{}",
                fixture.path().display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        )
        .env("JACKIN_AGENT", slug)
        .env("JACKIN_SESSION_STATE_DIR", fixture.path())
        .env_remove("JACKIN_EXEC_BINDINGS")
        .output()
        .expect("run entrypoint process")
}

#[test]
fn registry_launches_every_installed_native_cli() {
    let registry = jackin_core::agent_runtime_registry();
    assert_eq!(registry.len(), Agent::ALL.len());
    let admitted: std::collections::BTreeSet<_> =
        Agent::ALL.iter().map(|agent| agent.slug()).collect();
    let installed: std::collections::BTreeSet<_> =
        registry.iter().map(|runtime| runtime.slug()).collect();
    assert_eq!(
        admitted, installed,
        "registry identity coverage must be exact"
    );
    for runtime in registry {
        let agent = Agent::from_slug(runtime.slug()).expect("registry identity is admitted");
        assert!(Agent::ALL.contains(&agent));
        let output = execute_entrypoint(&ENTRYPOINT_SH, runtime.slug());
        assert!(
            output.status.success(),
            "{} failed: {output:?}",
            runtime.slug()
        );
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
        assert_eq!(
            stdout.lines().next(),
            Some(runtime.slug()),
            "wrong executable for {agent}"
        );
        if agent != Agent::Amp {
            assert!(
                stdout.contains("<two words>\n<$(must-stay-literal)>"),
                "argv splitting/evaluation for {agent}: {stdout}"
            );
        }
    }
    assert_eq!(
        execute_entrypoint(&ENTRYPOINT_SH, "unregistered")
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn missing_dispatch_case_fails_at_runtime() {
    for agent in Agent::ALL {
        let start = ENTRYPOINT_SH
            .find(&format!("\n  {})", agent.slug()))
            .expect("generated case");
        let end = start
            + ENTRYPOINT_SH[start..]
                .find("    ;;\n")
                .expect("case terminator")
            + "    ;;\n".len();
        let mutant = format!("{}{}", &ENTRYPOINT_SH[..start], &ENTRYPOINT_SH[end..]);
        let output = execute_entrypoint(&mutant, agent.slug());
        assert_eq!(
            output.status.code(),
            Some(2),
            "missing {} case must fail behaviorally: {output:?}",
            agent.slug()
        );
    }
}
