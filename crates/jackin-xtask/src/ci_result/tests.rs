use super::{FindArgs, Toggle, result_names};

fn args(package: &str) -> FindArgs {
    FindArgs {
        package: package.to_owned(),
        cache_key: "crate-key".to_owned(),
        all_features: Toggle::True,
        docker_e2e: Toggle::True,
        construct_image_changed: Toggle::True,
        common_contract_key: "common".to_owned(),
        docker_contract_key: "docker".to_owned(),
        runner_os: "Linux".to_owned(),
        runner_arch: "X64".to_owned(),
        source_sha: "abc123".to_owned(),
        repository: "jackin-project/jackin".to_owned(),
        refresh_package: String::new(),
        github_output: false,
    }
}

#[test]
fn non_product_crates_ignore_docker_inputs() {
    let names = result_names(&args("jackin-process"));

    assert_eq!(
        names.name,
        "ci-crate-result-v1-Linux-X64-jackin-process-crate-key-common-aftrue-e2efalse-constructfalse"
    );
    assert_eq!(
        names.sha_name,
        "ci-crate-result-sha-v1-Linux-X64-jackin-process-abc123-common-aftrue-e2efalse-constructfalse"
    );
}

#[test]
fn product_result_includes_docker_contract() {
    let names = result_names(&args("jackin"));

    assert!(
        names
            .name
            .starts_with("ci-crate-result-v1-Linux-X64-jackin-crate-key-")
    );
    assert!(names.name.ends_with("-aftrue-e2etrue-constructtrue"));
    assert!(!names.name.contains("-common-"));
}
