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

#[test]
fn op_read_args_include_account_when_pinned() {
    assert_eq!(
        op_read_args("op://vault/item/field", Some("acct-a")),
        vec!["--account", "acct-a", "read", "--", "op://vault/item/field"]
    );
}

#[test]
fn op_read_args_omit_account_when_unpinned() {
    assert_eq!(
        op_read_args("op://vault/item/field", None),
        vec!["read", "--", "op://vault/item/field"]
    );
}

#[test]
fn text_file_busy_retry_eventually_succeeds() {
    let attempts = std::cell::Cell::new(0);

    let result = retry_text_file_busy_result(|| {
        let next = attempts.get() + 1;
        attempts.set(next);
        if next < 3 {
            return Err(std::io::Error::from_raw_os_error(TEXT_FILE_BUSY_OS_ERROR));
        }
        Ok("ok")
    })
    .expect("retry should recover");

    assert_eq!(result, "ok");
    assert_eq!(attempts.get(), 3);
}
