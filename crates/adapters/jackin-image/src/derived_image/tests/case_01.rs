// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn renders_derived_dockerfile_with_workspace_and_entrypoint() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        &BTreeMap::new(),
        None,
    );

    // No agent_installs → overlay carries no agent COPY/install blocks.
    assert!(!dockerfile.contains("agent-binaries"));
    assert!(!dockerfile.contains("claude plugin"));
    assert!(!dockerfile.contains("WORKDIR"));
    assert!(dockerfile.contains(
        "COPY --link --chmod=0755 .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh"
    ));
    // A fixed PATH covers every agent's bin dir so the mounted binaries resolve.
    assert!(dockerfile.contains(
        "ENV PATH=\"/jackin/runtime:/home/agent/.local/bin:/home/agent/.amp/bin:/home/agent/.kimi-code/bin:/home/agent/.opencode/bin:/home/agent/.grok/bin:/home/agent/.antigravity/bin:/home/agent/.gemini-cli/bin:/home/agent/.cursor-agent/bin:/home/agent/.muse/bin:/home/agent/.omp/bin:/home/agent/.hermes/bin:${PATH}\""
    ));
    assert!(dockerfile.contains("ENTRYPOINT [\"/jackin/runtime/jackin-capsule\"]"));
}

#[test]
fn renders_runtime_finalization_in_one_layer() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        Some(".jackin-runtime/jackin-capsule"),
        &BTreeMap::new(),
        None,
    );

    assert!(dockerfile.contains(
        "COPY --link --chmod=0755 .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh"
    ));
    assert!(dockerfile.contains(
        "COPY --link --chmod=0755 .jackin-runtime/jackin-capsule /jackin/runtime/jackin-capsule"
    ));
    assert!(!dockerfile.contains("RUN chmod +x /jackin/runtime/"));
    // The title shim + runtime-dir creation share one finalization RUN (separate
    // from the now-standalone default-home snapshot for readability).
    assert_eq!(
        dockerfile
            .matches("( grep -q '__JACKIN_AUTO_TITLE_LOADED'")
            .count(),
        1,
        "title shim should be in exactly one finalization layer: {dockerfile}"
    );
    assert!(dockerfile.contains(
        "COPY --link --chown=agent:0 --chmod=0644 .jackin-runtime/zsh-title-shim /jackin/runtime/zsh-title-shim"
    ));
    assert!(
        dockerfile
            .contains("cat /jackin/runtime/zsh-title-shim >> /home/agent/.zshrc ) \\\n    && install -d -o agent -g 0 /jackin/run /jackin/state /jackin/account-credentials"),
        "runtime dir setup should share finalization and assign ownership at mkdir time: {dockerfile}"
    );
    assert!(
        dockerfile.contains(
            "RUN install -d -o agent -g 0 /jackin/default-home \\\n    && rm -rf '/home/agent/.claude/backups' \\\n    && for dir in '.claude'; do"
        ),
        "default-home snapshot creates only the root, never the per-agent targets (else mv nests): {dockerfile}"
    );
    // The mv target parent is made at mv time so `.claude` renames onto
    // /jackin/default-home/.claude rather than moving into a pre-created dir.
    assert!(
        dockerfile.contains("mkdir -p \"$(dirname \"/jackin/default-home/$dir\")\""),
        "mv must create the target parent, not pre-create the target: {dockerfile}"
    );
    assert!(
        !dockerfile.contains("/jackin/default-home/.claude /jackin/default-home/.codex"),
        "per-agent target dirs must not be pre-created in install -d: {dockerfile}"
    );
    assert_eq!(
        dockerfile.matches("mv \"/home/agent/$dir\"").count(),
        1,
        "default-home snapshot should use one mv loop, not one copy command per agent: {dockerfile}"
    );
    assert!(
        dockerfile.contains("install -d -o agent -g 0 -m 0775 \"/home/agent/$dir\""),
        "live-home placeholders must be writable by runtime supplementary group 0: {dockerfile}"
    );
    assert!(
        dockerfile.contains("find /jackin/default-home -type d -exec chmod g+rx {} +"),
        "default-home snapshot should normalize directory group readability before the guard: {dockerfile}"
    );
    assert!(
        dockerfile.contains("find /jackin/default-home -type f -exec chmod g+r {} +"),
        "default-home snapshot should normalize file group readability before the guard: {dockerfile}"
    );
    assert!(!dockerfile.contains("chown -R agent:agent /jackin/default-home"));
    assert!(!dockerfile.contains("chown agent:agent /jackin/run /jackin/state"));
    // Finalization is its own RUN now (default-home snapshot was pulled out).
    assert!(dockerfile.contains("\nRUN ( grep -q '__JACKIN_AUTO_TITLE_LOADED'"));
    assert!(!dockerfile.contains(
        "\nRUN install -d -o agent -g 0 /jackin/run /jackin/state /jackin/account-credentials"
    ));
    assert_eq!(
        dockerfile
            .matches("\nRUN install -d -o agent -g 0 /jackin/default-home")
            .count(),
        1
    );
    assert!(
        dockerfile.contains(
            "RUN bad=\"$(find /jackin/default-home \\( -type d ! -perm -0050 -o -type f ! -perm -0040 \\) -print -quit)\""
        ),
        "default-home snapshot should fail fast when a build-time installer bakes unreadable seed files: {dockerfile}"
    );
    assert!(
        dockerfile.contains("jackin default-home contains a non-group-readable path: $bad"),
        "default-home guard should explain the unreadable seed path: {dockerfile}"
    );
    assert!(
        dockerfile.contains(
            "RUN install -d -o agent -g 0 -m 0775 /home/agent /home/agent/.cache /home/agent/.cache/mise /home/agent/.config /home/agent/.config/git /home/agent/.config/fish /home/agent/.config/mise /home/agent/.local /home/agent/.local/bin /home/agent/.local/share /home/agent/.local/share/mise /home/agent/.local/share/mise/installs /home/agent/.local/share/mise/plugins /home/agent/.local/share/mise/shims /home/agent/.local/state /home/agent/.local/state/mise /home/agent/.local/state/mise/tracked-configs"
        ),
        "runtime home mutable roots must be writable by supplementary group 0: {dockerfile}"
    );
    assert!(
        dockerfile.contains("ARG JACKIN_RUN_UID=1000"),
        "derived image must accept the runtime host UID as a build arg: {dockerfile}"
    );
    assert!(
        dockerfile.contains("chown -R ${JACKIN_RUN_UID}:0 /home/agent"),
        "the whole runtime home must be owned by the runtime UID: {dockerfile}"
    );
    assert!(
        dockerfile.contains("chmod -R g+rwX /home/agent"),
        "the whole runtime home must be recursively group-writable: {dockerfile}"
    );
    assert!(
        dockerfile.contains("touch /home/agent/.gitconfig /home/agent/.config/git/config"),
        "runtime Git config files must exist before permission repair: {dockerfile}"
    );
    assert!(
        dockerfile.contains("chmod 0664 /home/agent/.gitconfig /home/agent/.config/git/config"),
        "runtime Git config files must be writable by supplementary group 0: {dockerfile}"
    );
    assert!(
        dockerfile.contains("! -uid ${JACKIN_RUN_UID}"),
        "runtime home guard must verify runtime UID ownership: {dockerfile}"
    );
    assert!(
        dockerfile.contains("for path in /home/agent/.zshrc /home/agent/.config/fish/config.fish"),
        "runtime shell config files must be repaired after finalization: {dockerfile}"
    );
    assert!(
        dockerfile.contains(
            "jackin runtime home contains a non-runtime-UID, non-group-0, or non-group-writable mutable path: $bad"
        ),
        "runtime mutable home dirs should be guarded after permission repair: {dockerfile}"
    );
    let finalization_pos = dockerfile
        .find("cat /jackin/runtime/zsh-title-shim >> /home/agent/.zshrc")
        .expect("runtime finalization");
    let writable_pos = dockerfile
        .find("Runtime home mutability")
        .expect("runtime home mutability");
    assert!(
        finalization_pos < writable_pos,
        "runtime home mutability repair must run after finalization creates/appends shell files: {dockerfile}"
    );
}

#[test]
fn runtime_payload_layers_follow_heavy_agent_and_default_home_layers() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        Some(".jackin-runtime/jackin-capsule"),
        &BTreeMap::new(),
        None,
    );

    let agent_section = dockerfile
        .find("# ── Agent CLIs")
        .expect("agent install section");
    let default_home_guard = dockerfile
        .find("jackin default-home contains a non-group-readable path")
        .expect("default-home guard");
    let entrypoint_copy = dockerfile
        .find(
            "COPY --link --chmod=0755 .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh",
        )
        .expect("entrypoint copy");
    let status_copy = dockerfile
        .find("COPY --link --chmod=0755 .jackin-runtime/agent-status /jackin/runtime/agent-status")
        .expect("agent-status copy");
    let title_shim_copy = dockerfile
        .find("COPY --link --chown=agent:0 --chmod=0644 .jackin-runtime/zsh-title-shim /jackin/runtime/zsh-title-shim")
        .expect("title-shim copy");
    let capsule_copy = dockerfile
        .find("COPY --link --chmod=0755 .jackin-runtime/jackin-capsule /jackin/runtime/jackin-capsule")
        .expect("capsule copy");
    let finalization = dockerfile
        .find("RUN ( grep -q '__JACKIN_AUTO_TITLE_LOADED'")
        .expect("runtime finalization");

    assert!(
        agent_section < default_home_guard,
        "agent/default-home ordering changed unexpectedly: {dockerfile}"
    );
    for (name, index) in [
        ("entrypoint", entrypoint_copy),
        ("agent-status", status_copy),
        ("title shim", title_shim_copy),
        ("capsule", capsule_copy),
    ] {
        assert!(
            default_home_guard < index && index < finalization,
            "{name} runtime payload copy should be after default-home guard and before finalization: {dockerfile}"
        );
    }
    assert!(
        title_shim_copy < finalization,
        "finalization must run after the zsh title shim exists: {dockerfile}"
    );
}

#[test]
fn renders_derived_dockerfile_keeps_construct_agent_identity() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        &BTreeMap::new(),
        None,
    );

    assert!(!dockerfile.contains("ARG JACKIN_HOST_UID"));
    assert!(!dockerfile.contains("ARG JACKIN_HOST_GID"));
    assert!(!dockerfile.contains("groupmod "));
    assert!(!dockerfile.contains("usermod "));
    assert!(!dockerfile.contains("chown -R agent:agent /home/agent"));
    assert!(!dockerfile.contains("chgrp -R 0 /home/agent /jackin/default-home"));
    assert!(!dockerfile.contains("chmod -R g=u /home/agent"));
    assert!(dockerfile.contains("COPY --link --chown=agent:0"));
    assert!(dockerfile.contains("install -d -o agent -g 0 /jackin/default-home"));
    assert!(dockerfile.contains("-type f ! -perm -0040"));
    assert!(dockerfile.contains("USER agent"));
}

#[test]
fn renders_derived_dockerfile_with_runtime_hooks() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        Some(&HooksConfig {
            setup_once: Some("hooks/setup-once.sh".to_owned()),
            source: Some("hooks/source.sh".to_owned()),
            preflight: Some("hooks/preflight.sh".to_owned()),
        }),
        &[Agent::Claude],
        None,
        &BTreeMap::new(),
        None,
    );

    assert!(dockerfile.contains(
        "COPY --link --chown=agent:0 --chmod=0755 hooks/setup-once.sh /jackin/runtime/hooks/setup-once.sh"
    ));
    assert!(dockerfile.contains(
        "RUN install -d /jackin/runtime/hooks \\\n    && install -d -o agent -g 0 /jackin/state /jackin/state/hooks"
    ));
    assert_eq!(
        dockerfile
            .matches("\nRUN install -d /jackin/runtime/hooks")
            .count(),
        1
    );
    assert!(!dockerfile.contains("chown -R agent:agent /jackin/state"));
    assert!(dockerfile.contains(
        "COPY --link --chown=agent:0 --chmod=0755 hooks/source.sh /jackin/runtime/hooks/source.sh"
    ));
    assert!(dockerfile.contains(
        "COPY --link --chown=agent:0 --chmod=0755 hooks/preflight.sh /jackin/runtime/hooks/preflight.sh"
    ));
    assert!(!dockerfile.contains("chmod +x /jackin/runtime/hooks/"));
    assert!(!dockerfile.contains("\nRUN chmod +x /jackin/runtime/hooks/"));
    assert!(!dockerfile.contains("\nRUN grep -q '__JACKIN_ZSHENV_SOURCE_LOADED'"));
    assert!(dockerfile.contains(
        "COPY --link --chown=agent:0 --chmod=0644 .jackin-runtime/zshenv-source-shim /jackin/runtime/zshenv-source-shim"
    ));
    assert!(dockerfile.contains("cat /jackin/runtime/zshenv-source-shim >> /home/agent/.zshenv"));
    assert!(dockerfile.contains(
        "for path in /home/agent/.zshrc /home/agent/.zshenv /home/agent/.config/fish/config.fish"
    ));
    // Structural shape: the four load-bearing fragments must appear
    // in order — guard test, rc capture, source call, success-only
    // export, file append. A regression that drops the guard, the rc
    // check, or the `fi` terminator breaks this ordering.
    let guard_pos = ZSHENV_SOURCE_SHIM
        .find("if [ -z \"${__JACKIN_ZSHENV_SOURCE_LOADED:-}\"")
        .unwrap();
    let source_pos = ZSHENV_SOURCE_SHIM
        .find("source /jackin/runtime/hooks/source.sh")
        .unwrap();
    let close_fn_pos = ZSHENV_SOURCE_SHIM.find("} || __jackin_rc=$?").unwrap();
    let export_pos = ZSHENV_SOURCE_SHIM
        .find("export __JACKIN_ZSHENV_SOURCE_LOADED=1")
        .unwrap();
    let close_pos = ZSHENV_SOURCE_SHIM.rfind("fi").unwrap();
    assert!(guard_pos < source_pos);
    assert!(source_pos < close_fn_pos);
    assert!(close_fn_pos < export_pos);
    assert!(export_pos < close_pos);
    assert!(ZSHENV_SOURCE_SHIM.contains("trap - ERR"));
    // Role hooks that `set -euo pipefail` must not leak nounset /
    // errexit / pipefail into the zsh that loads `.zshrc` next —
    // the source call runs in an anonymous fn with localized
    // options + traps.
    assert!(ZSHENV_SOURCE_SHIM.contains("setopt local_options local_traps"));
    // Single emission — derived-from-derived rebuilds must not stack
    // duplicate shim blocks in /home/agent/.zshenv.
    assert_eq!(dockerfile.matches(">> /home/agent/.zshenv").count(), 1);
}

#[test]
fn renders_derived_dockerfile_without_runtime_hooks() {
    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:0.1-trixie\n",
        None,
        &[Agent::Claude],
        None,
        &BTreeMap::new(),
        None,
    );

    assert!(!dockerfile.contains("setup-once.sh"));
    assert!(!dockerfile.contains("source.sh"));
    assert!(!dockerfile.contains("preflight.sh"));
    assert!(!dockerfile.contains("/jackin/runtime/hooks"));
    assert!(!dockerfile.contains("/jackin/state/hooks"));
    assert!(!dockerfile.contains("/home/agent/.zshenv"));
}

#[test]
fn fallback_only_context_does_not_create_agent_binary_dir() {
    let repo = tempdir().unwrap();
    std::fs::write(
        repo.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.path().join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["kimi"]

[kimi]
"#,
    )
    .unwrap();

    let validated = jackin_manifest::validate_role_repo(repo.path()).unwrap();
    let build = create_derived_build_context_for_agents(
        repo.path(),
        &validated,
        None,
        None,
        &[Agent::Kimi],
        &BTreeMap::new(),
    )
    .unwrap();
    let dockerignore = std::fs::read_to_string(build.context_dir.join(".dockerignore")).unwrap();

    assert!(
        !build
            .context_dir
            .join(".jackin-runtime/agent-binaries")
            .exists(),
        "fallback-only context should not create empty agent-binaries dir"
    );
    assert!(
        !dockerignore.contains("!.jackin-runtime/agent-binaries/"),
        "fallback-only context should not reopen agent-binaries in .dockerignore: {dockerignore}"
    );
}
