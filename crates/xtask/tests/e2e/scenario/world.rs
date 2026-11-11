// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};

use jackin_xtask::E2eArgs;

pub(crate) const ACCESS: &str = concat!(
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9",
    ".",
    "eyJzdWIiOiJ1c2VyLWExLWFjY2Vzcy1wYXlsb2FkIn0",
    ".",
    "c2lnbmF0dXJlLWExLWFjY2Vzcy1zZWNyZXQtYnl0ZXM"
);
pub(crate) const REFRESH: &str = "rt_a1refreshtokenvalue0123456789abcdefghijkl";
pub(crate) const ID_TOKEN: &str = concat!(
    "eyJhbGciOiJSUzI1NiIsImtpZCI6ImExIn0",
    ".",
    "eyJhdWQiOiJhMS1pZC10b2tlbi1wYXlsb2FkLWxvbmcifQ",
    ".",
    "aWQtdG9rZW4tc2lnbmF0dXJlLWExLWJ5dGVzLTAxMjM"
);
pub(crate) const ACCOUNT_ID: &str = "7f6c1f3e-0000-4000-8000-0000000000a1";
pub(crate) const CLIENT_ID: &str = "app_E2eClientId0123456789";

#[derive(Clone, Debug)]
pub(crate) struct World {
    pub root: PathBuf,
    pub home: PathBuf,
    pub registry: PathBuf,
    pub codex_home: PathBuf,
    pub role: PathBuf,
    pub repo_root: PathBuf,
    pub scratch_root: PathBuf,
    pub log_dir: PathBuf,
    pub lock_dir: PathBuf,
    pub engine_certs: PathBuf,
}

pub(crate) const MANIFEST: &str = "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n";
pub(crate) const POLICY: &str =
    "version: 1\nfilesystem_policy:\n  include_workdir: true\nnetwork_policies: {}\n";

fn put(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("mkdir");
    }
    fs::write(path, text).expect("write");
}

pub(crate) fn auth_json(access: &str, refresh: &str, id: &str, account: &str) -> String {
    serde_json::json!({
        "auth_mode": "chatgpt",
        "tokens": {
            "access_token": access,
            "refresh_token": refresh,
            "id_token": id,
            "account_id": account,
            "oauth_provenance": { "client_id": CLIENT_ID },
        },
    })
    .to_string()
}

impl World {
    pub(crate) fn build(root: &Path) -> World {
        let world = World {
            root: root.to_path_buf(),
            home: root.join("home"),
            registry: root.join("home/.config/openshell"),
            codex_home: root.join("codex"),
            role: root.join("role"),
            repo_root: root.join("repo"),
            scratch_root: root.join("scratch"),
            log_dir: root.join("logs"),
            lock_dir: root.join("lock"),
            engine_certs: root.join("engine-certs"),
        };
        put(
            &world.registry.join("gateways/jackin-local/metadata.json"),
            "{\"name\":\"jackin-local\"}\n",
        );
        put(
            &world.codex_home.join("auth.json"),
            &auth_json(ACCESS, REFRESH, ID_TOKEN, ACCOUNT_ID),
        );
        put(&world.role.join("jackin.role.toml"), MANIFEST);
        put(&world.role.join("policy.yaml"), POLICY);
        put(&world.role.join("Dockerfile"), "FROM construct\n");
        put(
            &world.role.join("probe/jackin-sentinel-probe"),
            "#!/bin/sh\necho probe\n",
        );
        put(&world.role.join("hooks/setup.sh"), "#!/bin/sh\n");
        put(&world.role.join("hooks/preflight.sh"), "#!/bin/sh\n");
        fs::create_dir_all(world.repo_root.join("rewrite/blockers")).expect("mkdir");
        fs::create_dir_all(&world.engine_certs).expect("mkdir");
        fs::create_dir_all(&world.lock_dir).expect("mkdir");
        world
    }

    pub(crate) fn probe_digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let bytes = fs::read(self.role.join("probe/jackin-sentinel-probe")).expect("probe file");
        crate::support::hex(&Sha256::digest(bytes))
    }

    pub(crate) fn blocker(&self, name: &str) {
        put(
            &self.repo_root.join("rewrite/blockers").join(name),
            "blocked\n",
        );
    }
}

pub(crate) fn args(world: &World, engine: bool) -> E2eArgs {
    E2eArgs {
        gateway: super::GATEWAY.to_owned(),
        role: Some(world.role.clone()),
        jackin: None,
        codex_home: Some(world.codex_home.clone()),
        agent: None,
        provider: None,
        model: None,
        credential: None,
        builder: None,
        platform: None,
        engine_port: engine.then(|| std::num::NonZeroU16::new(2376).expect("port")),
        engine_certs: engine.then(|| world.engine_certs.clone()),
        log_dir: None,
        sweep: false,
    }
}
