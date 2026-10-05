// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch/attach hot-path benchmark — baseline for the E1/E2 carve perf gate.
//!
//! Measures the in-process CPU-only operations on the `jackin load` / attach
//! critical path that span the new crate boundaries created by E1 (`jackin-isolation`)
//! and E2 (`jackin-instance`) carves from `jackin-runtime`.
//!
//! Run with:
//! ```sh
//! cargo bench -p jackin-runtime --bench launch_attach
//! ```
//! Record the numbers in the E0 PR description as the baseline. Future carve
//! PRs (E1, E2) must show no measurable regression against these numbers.

use jackin_core::WorkspaceName;
use std::path::Path;

use criterion::{Criterion, criterion_group, criterion_main};
use jackin_core::{Agent, RoleSelector};
use jackin_instance::manifest::{DockerResources, InstanceManifest, NewInstanceManifest};
use jackin_instance::naming::container_name_with_id;
use jackin_isolation::materialize::{clone_path_for, worktree_path_for};

// Representative fixtures.
const WORKSPACE: &str = "myworkspace";
const ROLE: &str = "myrole";
const NAMESPACE: &str = "myns";
const INSTANCE_ID: &str = "ab12cd34";
const STATE_DIR: &str = "/home/runner/.jackin/data/jk-ab12cd34-myws-myrole";
const CONTAINER_NAME: &str = "jk-ab12cd34-myws-myrole";
const DST: &str = "/workspace";

fn make_selector() -> RoleSelector {
    RoleSelector {
        name: ROLE.to_owned(),
        namespace: Some(NAMESPACE.to_owned()),
    }
}

fn new_manifest_input() -> NewInstanceManifest<'static> {
    NewInstanceManifest {
        container_base: CONTAINER_NAME,
        workspace_name: Some(WORKSPACE),
        workspace_label: WORKSPACE,
        workdir: DST,
        host_workdir_fingerprint: "abc123fingerprint0000000000000000",
        role_key: "myns/myrole",
        role_display_name: "My Role",
        agent_runtime: Agent::Claude,
        role_source_git: "https://github.com/example/roles.git",
        role_source_ref: Some("main"),
        image_tag: "jk-myns-myrole:abc123",
        docker: DockerResources {
            role_container: CONTAINER_NAME.to_owned(),
            dind_container: None,
            network: "jk-myws".to_owned(),
            certs_volume: None,
        },
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![Agent::Claude],
    }
}

// ── Naming: container_name_with_id (E2 hot path) ─────────────────────────────

fn bench_container_name(c: &mut Criterion) {
    let selector = make_selector();
    c.bench_function("naming/container_name_with_id", |b| {
        b.iter(|| {
            container_name_with_id(
                Some(&WorkspaceName::parse(WORKSPACE).unwrap_or_else(|_| {
                    unreachable!("bench workspace name is a fixed valid fixture")
                })),
                &selector,
                INSTANCE_ID,
            )
        });
    });
}

// ── Persisted role identity scan ────────────────────────────────────────────

/// Scan complete roles from 20 parsed manifest fixtures without filesystem I/O.
fn bench_role_identity_scan(c: &mut Criterion) {
    let manifests: Vec<InstanceManifest> = (0u32..20)
        .map(|i| {
            let mut manifest = InstanceManifest::new(new_manifest_input());
            manifest.container_base = format!("jk-{i:08x}-myworkspace-myrole");
            manifest.docker = DockerResources::from_container_name(&manifest.container_base);
            manifest.role_key = if i % 10 == 7 {
                "myns/myrole".to_owned()
            } else {
                format!("otherns{i}/myrole")
            };
            manifest
        })
        .collect();
    let selector = make_selector();

    c.bench_function("identity/persisted_role_scan_20", |b| {
        b.iter(|| {
            manifests
                .iter()
                .filter(|manifest| {
                    RoleSelector::parse(&manifest.role_key).is_ok_and(|role| role == selector)
                })
                .count()
        });
    });
}

// ── Isolation: mount path computation (E1 hot path) ──────────────────────────

fn bench_mount_paths(c: &mut Criterion) {
    let state_dir = Path::new(STATE_DIR);
    let mut group = c.benchmark_group("isolation");

    group.bench_function("worktree_path_for", |b| {
        b.iter(|| worktree_path_for(state_dir, DST, CONTAINER_NAME));
    });

    group.bench_function("clone_path_for", |b| {
        b.iter(|| clone_path_for(state_dir, DST, CONTAINER_NAME));
    });

    group.finish();
}

// ── Instance: manifest construction + serialization (E2 hot path) ────────────

fn bench_manifest_new(c: &mut Criterion) {
    c.bench_function("instance/manifest_new", |b| {
        b.iter(|| InstanceManifest::new(new_manifest_input()));
    });
}

fn bench_manifest_serialize(c: &mut Criterion) {
    let manifest = InstanceManifest::new(new_manifest_input());

    #[expect(
        clippy::unwrap_used,
        reason = "benchmark: serde_json serialization failure should abort the run immediately"
    )]
    c.bench_function("instance/manifest_serialize", |b| {
        b.iter(|| serde_json::to_string(&manifest).unwrap());
    });
}

criterion_group!(
    benches,
    bench_container_name,
    bench_role_identity_scan,
    bench_mount_paths,
    bench_manifest_new,
    bench_manifest_serialize,
);
criterion_main!(benches);
