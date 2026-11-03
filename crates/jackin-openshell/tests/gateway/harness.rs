// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{GatewayName, HostPath, OpenShellWorkspace};
use jackin_openshell::{GrpcGateway, GrpcGatewayConfig, resolve_gateway};
use tokio_util::sync::CancellationToken;

use super::fakes::{RegistryFs, ScriptedRunner, exited};
use super::pki::Pki;
use super::server::{ServerHandle, start};

pub(crate) const WORKSPACE: &str = "jackin-test";
pub(crate) const GATEWAY: &str = "jackin-local";
pub(crate) const HANG_GUARD: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub(crate) struct Env {
    pub(crate) server: ServerHandle,
    pub(crate) fs: Arc<RegistryFs>,
    pub(crate) runner: ScriptedRunner,
    pub(crate) home: HostPath,
}

pub(crate) async fn guard<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(HANG_GUARD, future)
        .await
        .expect("the operation hung")
}

pub(crate) fn host(path: &str) -> HostPath {
    HostPath::parse(Path::new(path)).unwrap()
}

pub(crate) fn list_json(endpoint: &str) -> String {
    format!(
        r#"[{{"active":true,"auth":"mtls","endpoint":"{endpoint}","is_remote":false,"name":"{GATEWAY}","remote_host":null,"resolved_host":null,"source":"user","type":"local"}}]"#
    )
}

pub(crate) fn install_pki(fs: &RegistryFs, home: &HostPath, pki: &Pki) {
    for (file, bytes) in [
        ("ca.crt", pki.ca_pem.clone()),
        ("tls.crt", pki.client_cert_pem.clone()),
        ("tls.key", pki.key_file()),
    ] {
        let path = host(&format!(
            "{}/gateways/{GATEWAY}/mtls/{file}",
            home.as_path().display()
        ));
        fs.put(&path, bytes.as_bytes());
    }
}

pub(crate) async fn env() -> Env {
    let pki = Pki::new();
    let server = start(&pki, &pki.ca_pem).await;
    let fs = Arc::new(RegistryFs::default());
    let home = host("/home/test/.config/openshell");
    install_pki(&fs, &home, &pki);
    let list = list_json(&server.endpoint());
    let runner = ScriptedRunner::always(exited(0, list.as_bytes(), b""));
    Env {
        server,
        fs,
        runner,
        home,
    }
}

pub(crate) fn name(text: &str) -> GatewayName {
    GatewayName::parse(text).unwrap()
}

pub(crate) async fn resolved(env: &Env) -> jackin_openshell::ResolvedGateway {
    resolve_gateway(
        &env.runner,
        env.fs.as_ref(),
        &env.home,
        &name(GATEWAY),
        &CancellationToken::new(),
    )
    .await
    .unwrap()
}

pub(crate) fn config(
    env: &Env,
    gateway: jackin_openshell::ResolvedGateway,
    workspace: &str,
    runner: ScriptedRunner,
) -> GrpcGatewayConfig {
    let _ = env;
    GrpcGatewayConfig {
        gateway,
        workspace: OpenShellWorkspace::parse(workspace).unwrap(),
        runner: Arc::new(runner),
        transfer_timeout: Duration::from_secs(123),
        call_timeout: Duration::from_secs(60),
    }
}

pub(crate) async fn connect(env: &Env, workspace: &str) -> GrpcGateway {
    let gateway = resolved(env).await;
    guard(GrpcGateway::connect(config(
        env,
        gateway,
        workspace,
        env.runner.clone(),
    )))
    .await
    .unwrap()
}

pub(crate) async fn wait_until(mut condition: impl FnMut() -> bool) {
    guard(async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
}
