use super::usage_cli_error;

#[test]
fn monitor_exit_keeps_json_on_stdout_and_stable_code() {
    let error: anyhow::Error = jackin::cli::usage::UsageCommandExit::new(
        2,
        r#"{"version":1,"result":"blocked"}"#.to_owned(),
    )
    .into();

    assert_eq!(
        usage_cli_error(&error),
        Some((2, r#"{"version":1,"result":"blocked"}"#))
    );
}

#[test]
fn inherited_auth_helper_output_is_not_duplicated() {
    let error: anyhow::Error = jackin::cli::usage::UsageCommandExit::new(2, String::new()).into();

    assert_eq!(usage_cli_error(&error), Some((2, "")));
}
