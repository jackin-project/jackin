// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Git source and staged-index identity checks.

use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::OsStringExt,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use jackin_process::{ExecRequest, StdioMode, exec_sync};

use super::RequestFields;

const OUTPUT_LIMIT: usize = 64 * 1024 * 1024;

pub(super) fn verify_tree_context(
    repository: &Path,
    source: &Path,
    fields: &RequestFields,
    expected_tree: &str,
    index: &[u8],
    inventory: &[u8],
) -> Result<()> {
    ensure!(
        git_output(repository, &["write-tree"])? == expected_tree,
        "live index differs from reviewed tree"
    );
    ensure!(
        index_snapshot(repository)? == index,
        "live index identity changed after bootstrap"
    );
    ensure!(
        git_output(source, &["rev-parse", "HEAD^{tree}"])? == expected_tree,
        "private build commit does not contain the reviewed tree"
    );
    ensure!(
        git_output(source, &["write-tree"])? == expected_tree,
        "private source index differs from the reviewed tree"
    );
    ensure!(
        git_output_bytes(source, &["ls-tree", "-r", "-z", expected_tree])? == inventory,
        "private source inventory differs from reviewed tree evidence"
    );
    ensure!(
        git_output(source, &["rev-parse", "HEAD"])? == fields.build_commit,
        "private build commit differs from bootstrap request"
    );
    ensure!(
        worktree_diff_is_clean(source),
        "private source worktree differs from its reviewed commit"
    );
    verify_private_source_git(source, repository)?;
    Ok(())
}

fn worktree_diff_is_clean(source: &Path) -> bool {
    git_output_bytes(
        source,
        &[
            "--no-pager",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--quiet",
            "HEAD",
            "--",
        ],
    )
    .is_ok()
}

pub(super) fn verify_private_source_git(source: &Path, repository: &Path) -> Result<()> {
    let source_git = git_path_output(
        source,
        &["rev-parse", "--path-format=absolute", "--git-dir"],
        false,
    )?;
    let expected_git = source.join(".git");
    ensure!(
        fs::canonicalize(source_git)? == expected_git,
        "private source Git directory is not local"
    );
    super::files::physical_private_directory(&expected_git, "private source Git directory")?;
    let private_index = git_path_output(
        source,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        false,
    )?;
    ensure!(
        fs::canonicalize(&private_index)? == expected_git.join("index"),
        "private source index is not local"
    );
    let index_metadata = fs::symlink_metadata(&private_index)?;
    ensure!(
        index_metadata.is_file()
            && !index_metadata.file_type().is_symlink()
            && index_metadata.uid() == nix::unistd::geteuid().as_raw()
            && index_metadata.nlink() == 1,
        "private source index has unsafe custody"
    );
    let source_objects = git_path_output(
        source,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ],
        false,
    )?;
    let expected_objects = expected_git.join("objects");
    ensure!(
        fs::canonicalize(source_objects)? == expected_objects,
        "private source object directory is not local"
    );
    let live_objects = git_path_output(
        repository,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ],
        false,
    )?;
    let live_objects =
        fs::canonicalize(live_objects).context("resolving read-only source alternate")?;
    let alternates_path = expected_objects.join("info/alternates");
    let alternates_metadata = fs::symlink_metadata(&alternates_path)?;
    ensure!(
        alternates_metadata.is_file()
            && !alternates_metadata.file_type().is_symlink()
            && alternates_metadata.uid() == nix::unistd::geteuid().as_raw()
            && alternates_metadata.nlink() == 1
            && alternates_metadata.permissions().mode() & 0o777 == 0o600,
        "private source alternate has unsafe custody"
    );
    ensure!(
        super::files::read_physical_file(&alternates_path, 64 * 1024)?
            == format!("{}\n", live_objects.display()).as_bytes(),
        "private source alternate does not point to the live read-only object store"
    );
    ensure!(
        super::files::read_physical_file(&expected_git.join("info/attributes"), 1024)?
            == b"* -filter -ident -text -eol -working-tree-encoding\n",
        "private source checkout attributes do not disable transformations"
    );
    Ok(())
}

pub(super) fn index_snapshot(repository: &Path) -> Result<Vec<u8>> {
    let index_path = git_path_output(
        repository,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        false,
    )?;
    let index_hash = super::files::hash_if_regular(&index_path, OUTPUT_LIMIT)?
        .unwrap_or_else(|| "none".to_owned());
    let shared_path = git_path_output(
        repository,
        &["rev-parse", "--path-format=absolute", "--shared-index-path"],
        true,
    )?;
    let shared_hash = if shared_path.as_os_str().is_empty() {
        "none".to_owned()
    } else {
        super::files::hash_if_regular(&shared_path, OUTPUT_LIMIT)?
            .context("active shared index disappeared")?
    };
    Ok(format!("index={index_hash}\nshared-index={shared_hash}\n").into_bytes())
}

pub(super) fn git_path_output(root: &Path, args: &[&str], allow_empty: bool) -> Result<PathBuf> {
    parse_git_path_record(git_output_bytes(root, args)?, allow_empty)
}

fn parse_git_path_record(mut bytes: Vec<u8>, allow_empty: bool) -> Result<PathBuf> {
    if allow_empty && bytes.is_empty() {
        return Ok(PathBuf::new());
    }
    ensure!(
        bytes.pop() == Some(b'\n'),
        "Git path output is missing its record terminator"
    );
    ensure!(!bytes.contains(&0), "Git path output contains a NUL byte");
    ensure!(allow_empty || !bytes.is_empty(), "Git path output is empty");
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

pub(super) fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let bytes = git_output_bytes(root, args)?;
    let value = std::str::from_utf8(&bytes)
        .context("Git output is not UTF-8")?
        .trim_end_matches('\n');
    ensure!(
        !value.contains('\n') && !value.is_empty(),
        "Git output is not one canonical record"
    );
    Ok(value.to_owned())
}

pub(in crate::precommit_provision) fn hook_config_paths(
    repository: &Path,
) -> Result<(PathBuf, PathBuf)> {
    let worktree_setting = git_result(
        repository,
        &[
            "config",
            "--local",
            "--bool",
            "--get",
            "extensions.worktreeConfig",
        ],
    )?;
    let worktree_config = if worktree_setting.success {
        let value = std::str::from_utf8(&worktree_setting.stdout)
            .context("worktree config setting is not UTF-8")?
            .trim_end_matches('\n');
        ensure!(
            !value.contains('\n') && matches!(value, "true" | "false"),
            "worktree config setting is not canonical"
        );
        value == "true"
    } else {
        ensure!(
            worktree_setting.code == Some(1) && worktree_setting.stderr.is_empty(),
            "cannot inspect worktree config setting"
        );
        false
    };
    let common = git_path_output(
        repository,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config",
        ],
        false,
    )?;
    let selected = if worktree_config {
        git_path_output(
            repository,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "config.worktree",
            ],
            false,
        )?
    } else {
        common.clone()
    };
    Ok((common, selected))
}

pub(super) fn git_output_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let result = git_result(root, args)?;
    ensure!(
        result.success,
        "Git identity query failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(result.stdout)
}

fn git_result(root: &Path, args: &[&str]) -> Result<jackin_process::ExecResult> {
    let mut request = ExecRequest::new("git", args)
        .env_clear()
        .envs([("PATH", std::env::var_os("PATH").unwrap_or_default())])
        .cwd(root.to_path_buf())
        .stdout_mode(StdioMode::Capture)
        .stderr_mode(StdioMode::Capture)
        .output_limits(OUTPUT_LIMIT, 64 * 1024);
    request = request.envs([
        ("GIT_CONFIG_NOSYSTEM", "1"),
        ("GIT_CONFIG_GLOBAL", "/dev/null"),
        ("GIT_ATTR_NOSYSTEM", "1"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_CONFIG_COUNT", "4"),
        ("GIT_CONFIG_KEY_0", "core.fsmonitor"),
        ("GIT_CONFIG_VALUE_0", "false"),
        ("GIT_CONFIG_KEY_1", "core.hooksPath"),
        ("GIT_CONFIG_VALUE_1", "/dev/null"),
        ("GIT_CONFIG_KEY_2", "core.attributesFile"),
        ("GIT_CONFIG_VALUE_2", "/dev/null"),
        ("GIT_CONFIG_KEY_3", "core.autocrlf"),
        ("GIT_CONFIG_VALUE_3", "false"),
    ]);
    exec_sync(&request).context("running fixed Git identity query")
}

#[cfg(test)]
mod tests;
