// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

#[cfg(unix)]
use super::*;

#[cfg(unix)]
#[test]
fn reviewed_snapshot_command_accepts_only_the_closed_argument_order() {
    let reviewed_tree = "a".repeat(40);
    let mut args = [
        "--bootstrap-root",
        "/private/bootstrap",
        "--repository",
        "/work/repository",
        "--reviewed-tree",
        reviewed_tree.as_str(),
    ]
    .into_iter()
    .map(OsString::from);
    let (bootstrap, repository, tree) = parse_provision_arguments(&mut args).unwrap();
    assert_eq!(bootstrap, PathBuf::from("/private/bootstrap"));
    assert_eq!(repository, PathBuf::from("/work/repository"));
    assert_eq!(tree, "a".repeat(40));
}

#[cfg(unix)]
#[test]
fn reviewed_snapshot_command_rejects_reordered_extra_and_noncanonical_values() {
    let reviewed_tree = "a".repeat(40);
    let mut wrong_order = [
        "--repository",
        "/work/repository",
        "--bootstrap-root",
        "/private/bootstrap",
        "--reviewed-tree",
        reviewed_tree.as_str(),
    ]
    .into_iter()
    .map(OsString::from);
    parse_provision_arguments(&mut wrong_order).expect_err("option order must be closed");

    let mut extra = [
        "--bootstrap-root",
        "/private/bootstrap",
        "--repository",
        "/work/repository",
        "--reviewed-tree",
        reviewed_tree.as_str(),
        "--force",
    ]
    .into_iter()
    .map(OsString::from);
    parse_provision_arguments(&mut extra).expect_err("extra options must be rejected");

    for tree in ["A".repeat(40), "a".repeat(39), "z".repeat(40)] {
        let args = [
            OsString::from("--bootstrap-root"),
            OsString::from("/private/bootstrap"),
            OsString::from("--repository"),
            OsString::from("/work/repository"),
            OsString::from("--reviewed-tree"),
            OsString::from(tree),
        ];
        let mut args = args.into_iter();
        parse_provision_arguments(&mut args).expect_err("invalid tree ids must be rejected");
    }
}

#[cfg(unix)]
#[test]
fn config_custody_command_accepts_only_one_absolute_repository() {
    let mut valid = ["--repository", "/work/repository"]
        .into_iter()
        .map(OsString::from);
    assert_eq!(
        parse_hook_config_arguments(&mut valid).unwrap(),
        PathBuf::from("/work/repository")
    );

    for values in [
        vec!["/work/repository", "--repository"],
        vec!["--repository", "relative"],
        vec!["--repository", "/work/repository", "--force"],
    ] {
        let mut args = values.into_iter().map(OsString::from);
        parse_hook_config_arguments(&mut args)
            .expect_err("invalid config arguments must be rejected");
    }
}
