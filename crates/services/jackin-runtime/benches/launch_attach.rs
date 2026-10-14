// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::WorkspaceName;
use std::path::Path;

use criterion::{Criterion, criterion_group, criterion_main};
use jackin_core::{Agent, RoleSelector};
use jackin_instance::manifest::{DockerResources, InstanceManifest, NewInstanceManifest};
use jackin_instance::naming::{
    class_family_matches_with_slug, compact_component, container_name_with_id,
};
use jackin_isolation::materialize::{clone_path_for, worktree_path_for};

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

fn bench_class_family_scan(c: &mut Criterion) {
    let containers: Vec<String> = (0u32..20)
        .map(|i| {
            if i % 10 == 7 {
                format!("jk-{i:08x}-myworkspace-myrole")
            } else {
                format!("jk-{i:08x}-myworkspace-otherrole{i}")
            }
        })
        .collect();
    let slug = compact_component(ROLE, "role");

    c.bench_function("naming/class_family_scan_20", |b| {
        b.iter(|| {
            containers
                .iter()
                .filter(|name| class_family_matches_with_slug(&slug, name))
                .count()
        });
    });
}

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
    bench_class_family_scan,
    bench_mount_paths,
    bench_manifest_new,
    bench_manifest_serialize,
);
criterion_main!(benches);
