// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    CREDENTIAL_FILE_MAX_BYTES, FsError, HostPath, ProcessError, ProcessStatus, Program,
    StdFileSystem,
};
use jackin_openshell::{
    DiscoveryError, GatewayAuth, RegisteredGateway, registered_gateways, resolve_gateway,
};
use tokio_util::sync::CancellationToken;

use super::fakes::{RegistryFs, ScriptedRunner, exited};
use super::harness::{GATEWAY, host, name};

const ONE: &str = r#"[
  {
    "active": true,
    "auth": "mtls",
    "endpoint": "https://localhost:17670",
    "is_remote": false,
    "name": "jackin-local",
    "remote_host": null,
    "resolved_host": null,
    "source": "user",
    "type": "local"
  }
]"#;

const THREE: &str = r#"[
  {"name": "jackin-local", "endpoint": "https://localhost:17670", "auth": "mtls", "extra": {"nested": 1}},
  {"name": "cloud", "endpoint": "https://gw.example.com", "auth": "oidc", "future_field": true},
  {"name": "other", "endpoint": "https://other.example.com:8443", "auth": "mtls"}
]"#;

fn entry(name_text: &str, endpoint: &str, auth: GatewayAuth) -> RegisteredGateway {
    RegisteredGateway {
        name: name(name_text),
        endpoint: endpoint.to_owned(),
        auth,
    }
}

#[tokio::test]
async fn registered_gateways_parses_the_cli_json_without_connecting() {
    let cancel = CancellationToken::new();
    let runner = ScriptedRunner::always(exited(0, ONE.as_bytes(), b""));
    let gateways = registered_gateways(&runner, &cancel).await.unwrap();
    assert_eq!(
        gateways,
        vec![entry(
            "jackin-local",
            "https://localhost:17670",
            GatewayAuth::Mtls
        )]
    );
    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].program, Program::Openshell);
    assert_eq!(
        calls[0].args,
        ["gateway", "list", "-o", "json"]
            .map(str::to_owned)
            .to_vec()
    );
    assert!(calls[0].stdin_null);
    assert!(calls[0].timeout > Duration::ZERO);
    calls[0].assert_openshell_env_isolated();

    let runner = ScriptedRunner::always(exited(0, THREE.as_bytes(), b""));
    assert_eq!(
        registered_gateways(&runner, &cancel).await.unwrap(),
        vec![
            entry("jackin-local", "https://localhost:17670", GatewayAuth::Mtls),
            entry(
                "cloud",
                "https://gw.example.com",
                GatewayAuth::Other("oidc".to_owned())
            ),
            entry("other", "https://other.example.com:8443", GatewayAuth::Mtls),
        ]
    );
    let runner = ScriptedRunner::always(exited(0, b"[]", b""));
    assert!(
        registered_gateways(&runner, &cancel)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn registered_gateways_failure_modes() {
    let cancel = CancellationToken::new();
    let runner = ScriptedRunner::always(exited(1, b"", b"\x1b[31mno registry\x1b[0m"));
    match registered_gateways(&runner, &cancel).await.unwrap_err() {
        DiscoveryError::ListFailed { status, tail } => {
            assert_eq!(status, ProcessStatus::Exited(1));
            assert!(!tail.as_str().contains('\u{1b}') && tail.as_str().contains("no registry"));
        }
        other => panic!("{other:?}"),
    }
    for body in [&b"not json"[..], &b"{}"[..]] {
        let runner = ScriptedRunner::always(exited(0, body, b""));
        let error = registered_gateways(&runner, &cancel).await.unwrap_err();
        assert!(
            matches!(error, DiscoveryError::ListUnparsable { .. }),
            "{error:?}"
        );
    }
    for failure in [
        ProcessError::Spawn {
            program: Program::Openshell,
            message: "gone".to_owned(),
        },
        ProcessError::Timeout {
            program: Program::Openshell,
            after: Duration::from_secs(1),
            group: None,
        },
    ] {
        let failure = std::sync::Mutex::new(Some(failure));
        let runner = ScriptedRunner::new(move |_| Err(failure.lock().unwrap().take().unwrap()));
        let error = registered_gateways(&runner, &cancel).await.unwrap_err();
        assert!(matches!(error, DiscoveryError::Process(_)), "{error:?}");
    }
}

#[tokio::test]
async fn resolve_gateway_selects_by_name_reads_exactly_three_files_and_writes_nothing() {
    let cancel = CancellationToken::new();
    let home = host("/home/test/.config/openshell");
    let fs = RegistryFs::default();
    let mtls = |gateway: &str, file: &str| {
        host(&format!(
            "/home/test/.config/openshell/gateways/{gateway}/mtls/{file}"
        ))
    };
    for (gateway, tag) in [(GATEWAY, "A"), ("other", "B")] {
        for file in ["ca.crt", "tls.crt", "tls.key"] {
            fs.put(&mtls(gateway, file), format!("{tag}-{file}").as_bytes());
        }
    }
    let runner = ScriptedRunner::always(exited(0, THREE.as_bytes(), b""));
    let resolved = resolve_gateway(&runner, &fs, &home, &name(GATEWAY), &cancel)
        .await
        .unwrap();
    assert_eq!(resolved.name().as_str(), GATEWAY);
    assert_eq!(resolved.endpoint(), "https://localhost:17670");
    assert_eq!(
        fs.reads(),
        vec![
            mtls(GATEWAY, "ca.crt"),
            mtls(GATEWAY, "tls.crt"),
            mtls(GATEWAY, "tls.key")
        ]
    );
    assert!(fs.mutations().is_empty());
    let debug = format!("{resolved:?}");
    assert!(!debug.contains("A-tls.key") && !debug.contains("A-ca.crt"));

    let before = fs.reads().len();
    let error = resolve_gateway(&runner, &fs, &home, &name("absent"), &cancel)
        .await
        .unwrap_err();
    assert!(
        matches!(error, DiscoveryError::UnknownGateway(ref missing) if missing.as_str() == "absent")
    );
    assert_eq!(fs.reads().len(), before);

    for file in ["ca.crt", "tls.crt", "tls.key"] {
        let path = mtls(GATEWAY, file);
        let saved = format!("A-{file}");
        fs.remove(&path);
        let error = resolve_gateway(&runner, &fs, &home, &name(GATEWAY), &cancel)
            .await
            .unwrap_err();
        assert!(
            matches!(error, DiscoveryError::MissingCertificate { path: ref named } if *named == path),
            "{file}"
        );
        fs.put(&path, saved.as_bytes());
    }

    let failing = ScriptedRunner::always(exited(1, b"", b"down"));
    let error = resolve_gateway(&failing, &fs, &home, &name(GATEWAY), &cancel)
        .await
        .unwrap_err();
    assert!(matches!(error, DiscoveryError::ListFailed { .. }));
    fs.fail_next_read(FsError::AlreadyExists(mtls(GATEWAY, "ca.crt")));
    let error = resolve_gateway(&runner, &fs, &home, &name(GATEWAY), &cancel)
        .await
        .unwrap_err();
    assert!(matches!(error, DiscoveryError::Fs(_)));
}

#[tokio::test]
async fn resolve_gateway_refuses_a_fifo_or_an_oversized_certificate_file() {
    let cancel = CancellationToken::new();
    let runner = ScriptedRunner::always(exited(0, THREE.as_bytes(), b""));
    let dir = tempfile::tempdir().unwrap();
    let home = HostPath::parse(dir.path()).unwrap();
    let mtls = dir.path().join("gateways").join(GATEWAY).join("mtls");
    std::fs::create_dir_all(&mtls).unwrap();
    for file in ["ca.crt", "tls.crt", "tls.key"] {
        std::fs::write(mtls.join(file), file.as_bytes()).unwrap();
    }
    let fs = StdFileSystem::default();
    resolve_gateway(&runner, &fs, &home, &name(GATEWAY), &cancel)
        .await
        .unwrap();

    std::fs::remove_file(mtls.join("tls.key")).unwrap();
    let made = tokio::process::Command::new("mkfifo")
        .arg(mtls.join("tls.key"))
        .status()
        .await
        .unwrap();
    assert!(made.success());
    let error = tokio::time::timeout(
        Duration::from_secs(10),
        resolve_gateway(&runner, &fs, &home, &name(GATEWAY), &cancel),
    )
    .await
    .expect("a FIFO blocked the read")
    .unwrap_err();
    assert!(
        matches!(&error, DiscoveryError::CertificateNotRegular { path } if path.as_path().ends_with("tls.key")),
        "{error:?}"
    );

    std::fs::remove_file(mtls.join("tls.key")).unwrap();
    std::fs::write(mtls.join("tls.key"), b"key").unwrap();
    let huge = std::fs::File::create(mtls.join("ca.crt")).unwrap();
    huge.set_len(u64::try_from(CREDENTIAL_FILE_MAX_BYTES).unwrap() + 1)
        .unwrap();
    let error = resolve_gateway(&runner, &fs, &home, &name(GATEWAY), &cancel)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, DiscoveryError::CertificateTooLarge { path } if path.as_path().ends_with("ca.crt")),
        "{error:?}"
    );
}

fn single(endpoint: &str, auth: &str) -> String {
    format!(r#"[{{"name": "{GATEWAY}", "endpoint": "{endpoint}", "auth": "{auth}"}}]"#)
}

#[tokio::test]
async fn resolve_gateway_refuses_a_plaintext_or_non_mtls_entry() {
    let cancel = CancellationToken::new();
    let home = host("/home/test/.config/openshell");
    let fs = RegistryFs::default();
    for file in ["ca.crt", "tls.crt", "tls.key"] {
        fs.put(
            &host(&format!(
                "/home/test/.config/openshell/gateways/{GATEWAY}/mtls/{file}"
            )),
            file.as_bytes(),
        );
    }
    let resolve = |listing: String| {
        let runner = ScriptedRunner::always(exited(0, listing.as_bytes(), b""));
        let (fs, home, cancel) = (&fs, &home, &cancel);
        async move { resolve_gateway(&runner, fs, home, &name(GATEWAY), cancel).await }
    };

    let resolved = resolve(single("https://localhost:17670", "mtls"))
        .await
        .unwrap();
    assert_eq!(resolved.endpoint(), "https://localhost:17670");

    for (endpoint, scheme) in [
        ("http://localhost:17670", Some("http")),
        ("ftp://localhost:17670", Some("ftp")),
        ("HTTPS://localhost:17670", Some("HTTPS")),
        ("localhost:17670", None),
        (concat!("http://admin:", "hunter2", "@h:1"), Some("http")),
        (concat!("admin:", "hunter2", "@h:1"), None),
        (concat!("admin:", "hunter2", "@h://x"), None),
    ] {
        let error = resolve(single(endpoint, "mtls")).await.unwrap_err();
        match &error {
            DiscoveryError::InsecureEndpoint {
                name: named,
                scheme: found,
            } => {
                assert_eq!(named.as_str(), GATEWAY);
                assert_eq!(found.as_deref(), scheme, "{endpoint}");
            }
            other => panic!("{endpoint}: {other:?}"),
        }
        let shown = format!("{error} {error:?}");
        assert!(
            !shown.contains("hunter2") && !shown.contains("localhost"),
            "{shown}"
        );
    }

    for auth in ["oidc", "edge", "MTLS", ""] {
        let error = resolve(single("https://localhost:17670", auth))
            .await
            .unwrap_err();
        assert!(
            matches!(
                error,
                DiscoveryError::UnsupportedAuth { ref name, auth: ref found }
                    if name.as_str() == GATEWAY && found == auth
            ),
            "{auth}: {error:?}"
        );
    }

    let runner = ScriptedRunner::always(exited(
        0,
        single("http://localhost:17670", "oidc").as_bytes(),
        b"",
    ));
    let listed = registered_gateways(&runner, &cancel).await.unwrap();
    assert_eq!(
        listed,
        vec![entry(
            GATEWAY,
            "http://localhost:17670",
            GatewayAuth::Other("oidc".to_owned())
        )]
    );
}

#[tokio::test]
async fn resolve_gateway_refuses_an_https_endpoint_with_credentials_without_echoing_them() {
    let cancel = CancellationToken::new();
    let home = host("/home/test/.config/openshell");
    let fs = RegistryFs::default();
    for file in ["ca.crt", "tls.crt", "tls.key"] {
        fs.put(
            &host(&format!(
                "/home/test/.config/openshell/gateways/{GATEWAY}/mtls/{file}"
            )),
            file.as_bytes(),
        );
    }
    for endpoint in [
        concat!("https://admin:", "ghp_FAKEf4k3", "@gateway.example:17670"),
        concat!("https://", "ghp_FAKEf4k3", "@gateway.example"),
        concat!(
            "https://admin:",
            "ghp_FAKEf4k3",
            "@gateway.example/path?x=1"
        ),
    ] {
        let runner = ScriptedRunner::always(exited(0, single(endpoint, "mtls").as_bytes(), b""));
        let error = resolve_gateway(&runner, &fs, &home, &name(GATEWAY), &cancel)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, DiscoveryError::EndpointCredentials { name: named } if named.as_str() == GATEWAY),
            "{endpoint}: {error:?}"
        );
        let shown = format!("{error} {error:?}");
        assert!(
            !shown.contains("ghp_FAKE")
                && !shown.contains("admin")
                && !shown.contains("gateway.example"),
            "{shown}"
        );
    }
    let accepted = ScriptedRunner::always(exited(
        0,
        single("https://gateway.example:17670/a@b", "mtls").as_bytes(),
        b"",
    ));
    resolve_gateway(&accepted, &fs, &home, &name(GATEWAY), &cancel)
        .await
        .unwrap();
}
