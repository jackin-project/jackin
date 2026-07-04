use super::*;

#[test]
fn launch_env_runner_uses_wider_bounded_timeout() {
    let runner = OpCli::new_launch_env();

    assert_eq!(runner.binary, OP_DEFAULT_BIN);
    assert_eq!(runner.timeout, std::time::Duration::from_mins(2));
    assert_eq!(runner.account, None);
}

#[test]
fn read_rejects_non_op_reference_before_spawn() {
    let runner = OpCli::with_binary("definitely-not-run".into());

    let result = runner.read("not-op://vault/item/field");

    let Err(err) = result else {
        panic!("expected non-op reference to fail");
    };
    assert!(err.to_string().contains("must start with op://"), "{err:#}");
}

#[test]
fn read_rejects_flag_like_reference_segments_before_spawn() {
    let runner = OpCli::with_binary("definitely-not-run".into());

    let result = runner.read("op://vault/-item/field");

    let Err(err) = result else {
        panic!("expected flag-like segment to fail");
    };
    assert!(
        err.to_string().contains("segment looks like a flag"),
        "{err:#}"
    );
}
