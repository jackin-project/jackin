//! Launch-pipeline micro-bench over pure phase helpers + FakeDocker records.
//!
//! Covers the grant-validation path extracted for suite A
//! (R-014-launch-pipeline-bench) without needing a full Docker host.

use criterion::{Criterion, criterion_group, criterion_main};
use jackin_core::selector::RoleSelector;
use jackin_runtime::runtime::docker_profile::{
    DockerGrants, dind_enabled, resolve_effective_grants, resolve_profile, validate_grants,
};
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

    c.bench_function("launch_pipeline/fake_docker_cleanup_record", |b| {
        b.iter(|| {
            let docker = FakeDockerClient::default();
            // Simulate the call pattern of grant-failure cleanup: rm dind + net + volume.
            let dind = "jk-bench-dind";
            let net = "jk-bench-net";
            let certs = "jk-bench-certs";
            docker.recorded.borrow_mut().push(format!("docker rm -f {dind}"));
            docker
                .recorded
                .borrow_mut()
                .push(format!("docker network rm {net}"));
            docker
                .recorded
                .borrow_mut()
                .push(format!("docker volume rm {certs}"));
            assert_eq!(docker.recorded.borrow().len(), 3);
            black_box(docker.recorded.borrow().len());
        });
    });

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
