// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn codex_launch_preflight_rejects_empty_host_auth_without_mounting_it() {
    use crate::instance::{AuthProvisionOutcome, PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let host_home = temp.path().join("host_home");
    std::fs::create_dir_all(host_home.join(".codex")).unwrap();
    std::fs::write(host_home.join(".codex/auth.json"), "\n \t").unwrap();

    let (state, outcome) = RoleState::prepare(
        &paths,
        "jk-agent-smith",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        &host_home,
        Agent::Codex,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::HostMissing);
    let mounts = agent_mounts(&state).unwrap();
    assert!(
        !mounts
            .iter()
            .any(|mount| mount.contains("/jackin/codex/auth.json")),
        "empty Codex credentials must fail closed before launch mount admission: {mounts:?}"
    );
    assert!(
        state.auth_mount_paths.is_empty(),
        "empty Codex credentials must not acquire an auth mount lease"
    );
}

#[tokio::test]
async fn agent_mounts_for_amp_synced_includes_secrets_json() {
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["amp"]

[amp]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let host_home = temp.path().join("host_home");
    std::fs::create_dir_all(host_home.join(".local/share/amp")).unwrap();
    std::fs::write(
        host_home.join(".local/share/amp/secrets.json"),
        "{\"apiKey@https://ampcode.com/\":\"sgamp_user_test\"}",
    )
    .unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-the-architect",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        &host_home,
        Agent::Amp,
    )
    .unwrap();

    let mounts = agent_mounts(&state).unwrap();
    assert!(
        mounts
            .iter()
            .any(|m| m.contains(":/home/agent/.local/share/amp")),
        "durable Amp data mount missing: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.contains("/jackin/amp/secrets.json") && m.ends_with(":ro")),
        "secrets.json handoff missing: {mounts:?}"
    );
}

#[tokio::test]
async fn agent_mounts_for_amp_ignore_mounts_state_but_no_auth_handoff() {
    use crate::instance::{PrepareResolvers, RoleState};
    use jackin_core::Agent;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["amp"]

[amp]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-the-architect",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        Agent::Amp,
    )
    .unwrap();

    let mounts = agent_mounts(&state).unwrap();
    assert!(
        mounts.iter().any(|m| m.contains(":/jackin/state")),
        "jackin state mount missing: {mounts:?}"
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.contains(":/home/agent/.local/share/amp")),
        "durable Amp data mount missing: {mounts:?}"
    );
    assert!(
        !mounts
            .iter()
            .any(|m| m.contains("/jackin/amp/secrets.json")),
        "ignore mode must not mount Amp auth handoff files: {mounts:?}"
    );
}

#[test]
fn exec_binding_names_joins_names_in_order() {
    let bindings = vec![
        jackin_protocol::ExecBinding {
            name: "A".to_owned(),
            kind: jackin_protocol::ExecKind::Op,
            source: "op://x".to_owned(),
        },
        jackin_protocol::ExecBinding {
            name: "B".to_owned(),
            kind: jackin_protocol::ExecKind::Literal,
            source: "v".to_owned(),
        },
    ];
    // This string is the contract the in-container picker reads; pin it so the
    // two launch paths can't drift on the format.
    assert_eq!(exec_binding_names(&bindings), "A,B");
    assert_eq!(exec_binding_names(&[]), "");
}

#[test]
fn capsule_config_redacts_literal_exec_binding_source_only() {
    let secret = "literal-secret-must-not-reach-agent-toml";
    let config = jackin_protocol::CapsuleConfig {
        workdir: "/workspace".to_owned(),
        exec_bindings: vec![
            jackin_protocol::ExecBinding {
                name: "LITERAL_TOKEN".to_owned(),
                kind: jackin_protocol::ExecKind::Literal,
                source: secret.to_owned(),
            },
            jackin_protocol::ExecBinding {
                name: "OP_TOKEN".to_owned(),
                kind: jackin_protocol::ExecKind::Op,
                source: "op://vault/item/field".to_owned(),
            },
            jackin_protocol::ExecBinding {
                name: "ENV_TOKEN".to_owned(),
                kind: jackin_protocol::ExecKind::Env,
                source: "$HOST_TOKEN".to_owned(),
            },
        ],
        ..Default::default()
    };

    let serialized = capsule_config_contents(&config).unwrap();
    assert!(!serialized.contains(secret));
    let projected: jackin_protocol::CapsuleConfig = toml::from_str(&serialized).unwrap();
    assert_eq!(projected.exec_bindings[0].source, "literal");
    assert_eq!(projected.exec_bindings[1].source, "op://vault/item/field");
    assert_eq!(projected.exec_bindings[2].source, "$HOST_TOKEN");
    assert_eq!(config.exec_bindings[0].source, secret);
}

#[test]
fn capsule_config_handoff_rejects_root_ancestors_and_private_mount_ancestors() {
    for workdir in ["/", "/home", "/jackin", "/workspace/../"] {
        let config = jackin_protocol::CapsuleConfig {
            workdir: workdir.to_owned(),
            ..Default::default()
        };
        let error = capsule_config_contents(&config)
            .expect_err("unsafe capsule workdir must not reach agent.toml");
        assert!(
            error.to_string().contains("protected"),
            "unexpected rejection for {workdir}: {error:#}"
        );
    }

    let config = jackin_protocol::CapsuleConfig {
        workdir: "/workspace".to_owned(),
        instance_mount_paths: std::collections::BTreeMap::from([(
            "canary".to_owned(),
            vec!["/workspace/private-slot".to_owned()],
        )]),
        ..Default::default()
    };
    let error = capsule_config_contents(&config)
        .expect_err("workspace ancestor of private mount must be rejected");
    assert!(error.to_string().contains("mount destination"));
}

#[test]
fn capsule_config_handoff_preserves_an_ordinary_workspace() {
    let config = jackin_protocol::CapsuleConfig {
        workdir: "/workspace/project".to_owned(),
        ..Default::default()
    };
    let serialized = capsule_config_contents(&config).expect("ordinary workdir is valid");
    assert!(serialized.contains("workdir = \"/workspace/project\""));
}

#[cfg(unix)]
#[test]
fn socket_dir_is_private_with_zero_exec_bindings() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = tempdir().unwrap();
    let socket_dir = temp.path().join("sockets").join("zero-bindings");
    prepare_socket_dir(&socket_dir, "role = 'fixture'\n").unwrap();

    let mode = std::fs::metadata(&socket_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
    assert!(
        socket_dir
            .join(jackin_protocol::CAPSULE_CONFIG_FILENAME)
            .is_file()
    );
}

#[test]
fn launch_args_keep_exact_safe_metadata_inline() {
    let token = "fake-github-token-for-argv-test";
    let token_entry = format!("GH_TOKEN={token}");
    let headers_entry = "OTEL_EXPORTER_OTLP_HEADERS=authorization=fake".to_owned();
    let secret = "fake-jackin-secret-for-argv-test";
    let secret_entry = format!("JACKIN_SECRET={secret}");
    let role_suffix_entry = "JACKIN_ROLE_METADATA=not-inline";
    let mut args = vec![
        "run",
        "-e",
        "JACKIN_ROLE=fixture",
        "-e",
        token_entry.as_str(),
        "-e",
        headers_entry.as_str(),
        "-e",
        secret_entry.as_str(),
        "-e",
        role_suffix_entry,
    ];

    let host_only = extract_host_env_entries(&mut args).unwrap();

    assert_eq!(args, ["run", "-e", "JACKIN_ROLE=fixture"]);
    assert!(!args.join(" ").contains(token));
    assert!(!args.join(" ").contains(secret));
    assert_eq!(
        host_only,
        [
            ("GH_TOKEN".to_owned(), token.to_owned()),
            (
                "OTEL_EXPORTER_OTLP_HEADERS".to_owned(),
                "authorization=fake".to_owned()
            ),
            ("JACKIN_SECRET".to_owned(), secret.to_owned()),
            ("JACKIN_ROLE_METADATA".to_owned(), "not-inline".to_owned()),
        ]
    );
}

#[cfg(unix)]
#[test]
fn env_file_is_private_host_only_and_removed_on_success_and_error_drop() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = tempdir().unwrap();
    let jackin_home = temp.path().join("jackin-home");
    let socket_dir = jackin_home.join("sockets").join("fixture");
    let file = create_host_env_file(
        &jackin_home,
        "fixture",
        &[("GH_TOKEN".to_owned(), "fake-secret".to_owned())],
    )
    .unwrap()
    .unwrap();
    let path = file.path().to_path_buf();

    assert!(path.is_file());
    assert!(!path.starts_with(socket_dir));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "GH_TOKEN=fake-secret\n"
    );
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    drop(file);
    assert!(!path.exists());

    let error_file = create_host_env_file(
        &jackin_home,
        "fixture",
        &[("GH_TOKEN".to_owned(), "other-fake-secret".to_owned())],
    )
    .unwrap()
    .unwrap();
    let error_path = error_file.path().to_path_buf();
    let result: std::io::Result<()> = {
        let _guard = error_file;
        Err(std::io::Error::other("simulated runtime failure"))
    };
    assert!(result.is_err());
    assert!(!error_path.exists());
}
