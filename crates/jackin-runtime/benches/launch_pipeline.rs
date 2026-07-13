//! Launch-pipeline micro-bench over real phase helpers + `FakeDocker`.
//!
//! Drives `cleanup_after_grant_failure` / `LoadCleanup::run` (the suite A
//! helpers used by `run_launch_core`) — not string bookkeeping theater.

use criterion::{Criterion, criterion_group, criterion_main};
use jackin_core::selector::RoleSelector;
use jackin_runtime::runtime::docker_profile::{
    DockerGrants, dind_enabled, resolve_effective_grants, resolve_profile, validate_grants,
};
use jackin_runtime::runtime::launch::{LoadCleanup, cleanup_after_grant_failure};
use jackin_test_support::FakeDockerClient;
use std::hint::black_box;
use std::path::PathBuf;

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

criterion_group!(benches, grant_validation_micro);
criterion_main!(benches);
