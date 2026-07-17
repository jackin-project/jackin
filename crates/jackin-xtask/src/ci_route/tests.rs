use super::*;

fn args() -> CiRouteArgs {
    CiRouteArgs {
        metadata: PathBuf::from("metadata.json"),
        base_ref: String::new(),
        before_sha: String::new(),
        event_name: "push".to_owned(),
        force_package: String::new(),
        common_contract_key: String::new(),
        construct_image_changed: "false".to_owned(),
        docker_contract_key: String::new(),
        docker_e2e: "false".to_owned(),
        source_sha: "abc".to_owned(),
        repository: "owner/repo".to_owned(),
    }
}

#[test]
fn dispatch_selects_every_crate() {
    let mut args = args();
    args.event_name = "workflow_dispatch".to_owned();
    assert_eq!(selection_args(&args).unwrap(), ["--all"]);
}

#[test]
fn zero_before_sha_selects_every_crate() {
    let mut args = args();
    args.before_sha = "000000".to_owned();
    assert_eq!(selection_args(&args).unwrap(), ["--all"]);
}
