//! Launch-pipeline benches: suite-A helpers + end-to-end orchestration wall-time.
//!
//! The end-to-end scenario drives `load_role` (public entry that includes
//! `run_launch_core`) over `FakeDockerClient` / `FakeRunner` so Criterion
//! measures orchestration logic, not Docker I/O.

use criterion::{Criterion, criterion_group, criterion_main};
use jackin_config::AppConfig;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_runtime::runtime::docker_profile::{
    DockerGrants, dind_enabled, resolve_effective_grants, resolve_profile, validate_grants,
};
use jackin_runtime::runtime::launch::{
    LoadCleanup, LoadOptions, cleanup_after_grant_failure, load_role,
};
use jackin_test_support::{FakeDockerClient, FakeRunner, seed_valid_role_repo};
use std::collections::VecDeque;
use std::hint::black_box;
use std::path::PathBuf;
use tempfile::tempdir;

fn grant_validation_micro(c: &mut Criterion) {
    let grants = DockerGrants::default();

    c.bench_function("launch_pipeline/validate_default_grants", |b| {
        b.iter(|| {
            let errs = validate_grants(black_box(&grants));
            assert!(errs.is_empty());
            let profile = resolve_profile(None, None, None);
            let effective = resolve_effective_grants(profile.0, Some(&grants), None);
            black_box(dind_enabled(&effective));
        });
    });

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(error) => {
            black_box(error);
            return;
        }
    };

    c.bench_function(
        "launch_pipeline/cleanup_after_grant_failure_fakedocker",
        |b| {
            b.iter(|| {
                rt.block_on(async {
                    let docker = FakeDockerClient::default();
                    let cleanup = LoadCleanup::new(
                        "jk-bench-role".into(),
                        "jk-bench-dind".into(),
                        "jk-bench-certs".into(),
                        "jk-bench-net".into(),
                        PathBuf::from("/tmp/jackin-bench-sock"),
                    );
                    // Real suite A helper used by run_launch_core on grant failure.
                    cleanup_after_grant_failure(&cleanup, &docker).await;
                    let recorded = docker.recorded.borrow();
                    assert!(
                        recorded.iter().any(|c| c == "docker rm -f jk-bench-dind"),
                        "LoadCleanup::run must record DinD rm via FakeDocker; got {recorded:?}"
                    );
                    assert!(
                        recorded
                            .iter()
                            .any(|c| c == "docker network rm jk-bench-net"),
                        "LoadCleanup::run must record network rm; got {recorded:?}"
                    );
                    black_box(recorded.len());
                });
            });
        },
    );

    c.bench_function("launch_pipeline/role_selector_key", |b| {
        let selector = RoleSelector::new(Some("ns"), "the-architect");
        b.iter(|| black_box(selector.key()));
    });

    c.bench_function("launch_pipeline/container_state_path", |b| {
        let data = PathBuf::from("/home/runner/.jackin/data");
        let name = "jk-ab12cd34-myws-myrole";
        b.iter(|| black_box(data.join(black_box(name))));
    });
}

/// End-to-end orchestration wall-time: validation → materialize/image choice →
/// run → attach/finalization → cleanup over `FakeDocker` (plan 016 step 4).
#[expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "criterion bench harness: fail-fast fixture setup"
)]
fn pipeline_e2e_orchestration(c: &mut Criterion) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(error) => {
            black_box(error);
            return;
        }
    };

    c.bench_function("launch_pipeline/run_launch_core_e2e_fakedocker", |b| {
        b.iter(|| {
            rt.block_on(async {
                let temp = tempdir().expect("tempdir");
                let paths = JackinPaths::for_tests(temp.path());
                // Install binary stubs so image/agent resolution never hits network.
                jackin_image::agent_binary::install_test_stub(
                    &paths,
                    jackin_core::agent::Agent::Codex,
                )
                .expect("stub");
                jackin_image::capsule_binary::install_test_stub(&paths).expect("capsule stub");
                paths.ensure_base_dirs().unwrap();

                let selector = RoleSelector::new(None, "agent-smith");
                let cached = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
                seed_valid_role_repo(&cached.repo_dir);
                std::fs::write(
                    cached.repo_dir.join("jackin.role.toml"),
                    r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
                )
                .unwrap();

                let mut config = AppConfig::load_or_init(&paths).unwrap();
                config.roles.insert(
                    "agent-smith".to_owned(),
                    jackin_config::RoleSource {
                        git: "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
                        trusted: true,
                        env: std::collections::BTreeMap::new(),
                    },
                );

                let workspace = jackin_config::ResolvedWorkspace {
                    name: String::new(),
                    label: cached.repo_dir.display().to_string(),
                    workdir: "/workspace".to_owned(),
                    mounts: vec![jackin_config::MountConfig {
                        src: cached.repo_dir.display().to_string(),
                        dst: "/workspace".to_owned(),
                        readonly: false,
                        isolation: jackin_config::MountIsolation::Shared,
                    }],
                    default_agent: None,
                    keep_awake_enabled: false,
                    git_pull_on_entry: false,
                };

                let docker = FakeDockerClient {
                    exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
                        String::new(),
                        String::new(),
                        "Sessions: 1\n".to_owned(),
                        "Sessions: 0\n".to_owned(),
                    ])),
                    ..Default::default()
                };
                let mut runner = FakeRunner::for_load_agent([String::new()]);

                let result = load_role(
                    &paths,
                    &mut config,
                    &selector,
                    &workspace,
                    &docker,
                    &mut runner,
                    &LoadOptions {
                        agent: Some(jackin_core::agent::Agent::Codex),
                        ..Default::default()
                    },
                )
                .await;
                black_box(result.is_ok());
            });
        });
    });
}

criterion_group!(benches, grant_validation_micro, pipeline_e2e_orchestration);
criterion_main!(benches);
