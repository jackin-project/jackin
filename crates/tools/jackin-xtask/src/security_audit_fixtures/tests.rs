use super::*;
use anyhow::anyhow;
use serde_json::json;

#[cfg(unix)]
#[test]
fn executable_search_uses_subprocess_path_rules() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let cwd = temporary.path().join("first-cwd");
    let other_cwd = temporary.path().join("other-cwd");
    fs::create_dir(&cwd).expect("first cwd");
    fs::create_dir(&other_cwd).expect("other cwd");

    assert_eq!(
        search_directories(None, &cwd),
        [PathBuf::from("/bin"), PathBuf::from("/usr/bin")]
    );
    assert_eq!(
        search_directories(Some(OsStr::new("")), &cwd),
        std::slice::from_ref(&cwd)
    );
    assert_eq!(
        search_directories(Some(OsStr::new("first::/absolute")), &cwd),
        [cwd.join("first"), cwd.clone(), PathBuf::from("/absolute")]
    );

    let first_cwd_tool = cwd.join("mise");
    let other_cwd_tool = other_cwd.join("mise");
    make_executable(&first_cwd_tool, 0o755).expect("first cwd tool");
    make_executable(&other_cwd_tool, 0o755).expect("other cwd tool");
    assert_eq!(
        find_executable(OsStr::new("mise"), Some(OsStr::new("")), &cwd)
            .expect("empty PATH selects cwd executable"),
        fs::canonicalize(&first_cwd_tool).expect("canonical first tool")
    );
    assert_eq!(
        find_executable(OsStr::new("mise"), Some(OsStr::new("")), &other_cwd)
            .expect("empty PATH selects other cwd executable"),
        fs::canonicalize(&other_cwd_tool).expect("canonical other tool")
    );

    let first_directory = cwd.join("not-executable");
    fs::create_dir(&first_directory).expect("non-executable PATH directory");
    let first = first_directory.join("mise");
    let second_directory = cwd.join("later");
    fs::create_dir(&second_directory).expect("later PATH directory");
    let second = second_directory.join("mise");
    #[cfg(unix)]
    let first_mode = u32::from(nix::unistd::geteuid().as_raw() != 0);
    #[cfg(not(unix))]
    let first_mode = 0o000;
    make_executable(&first, first_mode).expect("non-executable first candidate");
    make_executable(&second, 0o755).expect("later executable candidate");
    assert!(
        fs::metadata(&first)
            .expect("first candidate exists")
            .is_file()
    );
    nix::unistd::access(
        &first,
        nix::unistd::AccessFlags::F_OK | nix::unistd::AccessFlags::X_OK,
    )
    .expect_err("first PATH candidate is not executable by the current user");
    nix::unistd::access(
        &second,
        nix::unistd::AccessFlags::F_OK | nix::unistd::AccessFlags::X_OK,
    )
    .expect("later PATH candidate is executable by the current user");
    assert_eq!(
        find_executable(
            OsStr::new("mise"),
            Some(OsStr::new("not-executable:later")),
            &cwd,
        )
        .expect("later executable PATH entry"),
        fs::canonicalize(&second).expect("canonical later tool")
    );

    let empty = BTreeMap::new();
    let request = build_exec_request(&second, &[], &cwd, &empty);
    assert!(!request.env_clear);
    assert!(request.env.is_empty());
    assert_eq!(request.cwd.as_deref(), Some(cwd.as_path()));
    assert_eq!(request.stdin_mode, StdioMode::Inherit);
    assert_eq!(request.stdout_mode, StdioMode::Capture);
    assert_eq!(request.stderr_mode, StdioMode::Capture);
    assert_eq!(request.retry, RetryPolicy::none());
    assert!(request.timeout.is_none());

    let mut empty_path = BTreeMap::new();
    empty_path.insert(OsString::from("PATH"), OsString::new());
    let request = build_exec_request(&second, &[], &cwd, &empty_path);
    assert!(!request.env_clear);
    assert_eq!(request.env, [(OsString::from("PATH"), OsString::new())]);
}

#[test]
fn manifests_are_compact_and_finding_filter_is_specific() {
    assert_eq!(
        manifest("matrix_key", SYNTHETIC_ID).expect("manifest"),
        "{\"matrix_key\":\"m-0123456789abcdef\"}\n"
    );
    let findings = vec![
        json!({"RuleID":"generic-api-key"}),
        json!({"RuleID":"private-key"}),
        json!({"RuleID":"other"}),
    ];
    assert_eq!(generic_findings(&findings).len(), 1);
}

#[test]
fn fixture_cleanup_preserves_body_and_cleanup_failures() {
    let path = Path::new("/private/scanner-fixture");
    assert_eq!(
        finish_with_cleanup(Ok(7), Ok(()), path).expect("both operations succeed"),
        7
    );

    let body_only = finish_with_cleanup::<()>(Err(anyhow!("body failure")), Ok(()), path)
        .expect_err("body failure is retained");
    assert_eq!(body_only.to_string(), "body failure");

    let cleanup_only = finish_with_cleanup(Ok(()), Err(io::Error::other("cleanup failure")), path)
        .expect_err("cleanup failure is reported");
    assert!(format!("{cleanup_only:#}").contains("cleanup failure"));

    let both = finish_with_cleanup::<()>(
        Err(anyhow!("body failure")),
        Err(io::Error::other("cleanup failure")),
        path,
    )
    .expect_err("both failures are retained");
    assert_eq!(both.root_cause().to_string(), "body failure");
    let chain = format!("{both:#}");
    assert!(chain.contains("body failure"));
    assert!(chain.contains("cleanup failure"));
}
