// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    WorktreeState, assess_worktree, changed_files, parse_porcelain, unpushed_commit_count,
};
use crate::runner::{CommandRunner, RunOptions};
use std::future::Future;
use std::path::Path;
use std::task::{Context, Poll};

const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// Drive an immediately-ready future to completion without an async runtime.
/// The mock runner never yields, so a single poll resolves; the loop is a
/// safety net. Uses the stable no-op waker — no unsafe.
fn block_on<F: Future>(fut: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

/// Scripted git responses keyed on the git subcommand (`args[2]`).
#[derive(Default)]
struct MockGit {
    porcelain: String,
    for_each_ref: String,
    rev_list: String,
    symbolic_ref_ok: bool,
    rev_parse_head: String,
    log_output: String,
    fail_subcommand: Option<&'static str>,
}

impl CommandRunner for MockGit {
    async fn run(
        &mut self,
        _program: &str,
        _args: &[&str],
        _cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn capture(
        &mut self,
        _program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        let sub = args.get(2).copied().unwrap_or("");
        if self.fail_subcommand == Some(sub) {
            anyhow::bail!("mock git failure for {sub}");
        }
        match sub {
            "status" => Ok(self.porcelain.clone()),
            "for-each-ref" => Ok(self.for_each_ref.clone()),
            "rev-list" => Ok(self.rev_list.clone()),
            // symbolic-ref --quiet HEAD: Ok on attached branch, Err on detached.
            "symbolic-ref" => {
                if self.symbolic_ref_ok {
                    Ok(String::new())
                } else {
                    anyhow::bail!("not a symbolic ref")
                }
            }
            "rev-parse" => Ok(self.rev_parse_head.clone()),
            "log" => Ok(self.log_output.clone()),
            other => anyhow::bail!("unexpected git subcommand: {other}"),
        }
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.capture(program, args, cwd).await
    }
}

fn assess(mock: &mut MockGit) -> WorktreeState {
    block_on(assess_worktree("/wt", BASE, mock, |_| {})).expect("assess never errs")
}

#[test]
fn clean_worktree_at_base_is_clean() {
    let mut mock = MockGit {
        porcelain: String::new(),
        // One branch parked at base, attached HEAD.
        for_each_ref: format!("scratch\t{BASE}\t\t"),
        symbolic_ref_ok: true,
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Clean);
}

#[test]
fn uncommitted_changes_are_dirty() {
    let mut mock = MockGit {
        porcelain: " M src/foo.rs\n".to_owned(),
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Dirty);
}

#[test]
fn untracked_files_are_dirty() {
    let mut mock = MockGit {
        porcelain: "?? notes.md\n".to_owned(),
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Dirty);
}

#[test]
fn branch_ahead_with_no_upstream_is_unpushed() {
    let mut mock = MockGit {
        porcelain: String::new(),
        // tip moved past base, no upstream column.
        for_each_ref: "feature\tbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\t\t".to_owned(),
        symbolic_ref_ok: true,
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Unpushed);
}

#[test]
fn branch_ahead_of_upstream_is_unpushed() {
    let mut mock = MockGit {
        porcelain: String::new(),
        for_each_ref:
            "feature\tbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\torigin/feature\t[ahead 1]"
                .to_owned(),
        rev_list: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n".to_owned(),
        symbolic_ref_ok: true,
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Unpushed);
}

#[test]
fn branch_fully_pushed_is_clean() {
    let mut mock = MockGit {
        porcelain: String::new(),
        for_each_ref: "feature\tbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\torigin/feature\t"
            .to_owned(),
        rev_list: String::new(), // nothing ahead of upstream
        symbolic_ref_ok: true,
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Clean);
}

#[test]
fn upstream_gone_is_treated_as_merged_clean() {
    let mut mock = MockGit {
        porcelain: String::new(),
        for_each_ref: "feature\tbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\torigin/feature\t[gone]"
            .to_owned(),
        symbolic_ref_ok: true,
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Clean);
}

#[test]
fn detached_head_off_base_is_unpushed() {
    let mut mock = MockGit {
        porcelain: String::new(),
        for_each_ref: format!("scratch\t{BASE}\t\t"),
        symbolic_ref_ok: false, // detached
        rev_parse_head: "cccccccccccccccccccccccccccccccccccccccc".to_owned(),
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Unpushed);
}

#[test]
fn detached_head_at_base_is_clean() {
    let mut mock = MockGit {
        porcelain: String::new(),
        for_each_ref: format!("scratch\t{BASE}\t\t"),
        symbolic_ref_ok: false,
        rev_parse_head: BASE.to_owned(),
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Clean);
}

#[test]
fn status_failure_fails_closed_to_unpushed() {
    let mut mock = MockGit {
        fail_subcommand: Some("status"),
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Unpushed);
}

#[test]
fn no_local_branches_fails_closed_to_unpushed() {
    let mut mock = MockGit {
        porcelain: String::new(),
        for_each_ref: String::new(),
        ..MockGit::default()
    };
    assert_eq!(assess(&mut mock), WorktreeState::Unpushed);
}

#[test]
fn multiple_repos_assessed_independently() {
    // Repo A dirty, repo B clean — assessed via separate calls (the caller
    // iterates mounts), proving the function is per-path with no shared state.
    let mut dirty = MockGit {
        porcelain: " M a.rs\n".to_owned(),
        ..MockGit::default()
    };
    let mut clean = MockGit {
        porcelain: String::new(),
        for_each_ref: format!("scratch\t{BASE}\t\t"),
        symbolic_ref_ok: true,
        ..MockGit::default()
    };
    assert_eq!(assess(&mut dirty), WorktreeState::Dirty);
    assert_eq!(assess(&mut clean), WorktreeState::Clean);
}

#[test]
fn changed_files_parses_status_via_runner() {
    let mut mock = MockGit {
        porcelain: " M src/a.rs\n?? notes.md\n D old.rs\n".to_owned(),
        ..MockGit::default()
    };
    let files = block_on(changed_files("/wt", &mut mock));
    assert_eq!(files.len(), 3);
    assert_eq!(files[0].status, 'M');
    assert_eq!(files[0].path, "src/a.rs");
    assert_eq!(files[1].status, '?');
    assert_eq!(files[1].path, "notes.md");
    assert_eq!(files[2].status, 'D');
    assert_eq!(files[2].path, "old.rs");
}

#[test]
fn changed_files_empty_on_error() {
    let mut mock = MockGit {
        fail_subcommand: Some("status"),
        ..MockGit::default()
    };
    assert!(block_on(changed_files("/wt", &mut mock)).is_empty());
}

#[test]
fn unpushed_commit_count_counts_log_lines() {
    let mut mock = MockGit {
        log_output: "aaaa\nbbbb\ncccc\n".to_owned(),
        ..MockGit::default()
    };
    assert_eq!(block_on(unpushed_commit_count("/wt", &mut mock)), 3);
}

#[test]
fn unpushed_commit_count_zero_when_pushed() {
    let mut mock = MockGit::default();
    assert_eq!(block_on(unpushed_commit_count("/wt", &mut mock)), 0);
}

#[test]
fn unpushed_commit_count_zero_on_error() {
    let mut mock = MockGit {
        fail_subcommand: Some("log"),
        ..MockGit::default()
    };
    assert_eq!(block_on(unpushed_commit_count("/wt", &mut mock)), 0);
}

#[test]
fn parse_porcelain_skips_blank_lines() {
    let files = parse_porcelain(" M a\n\n A b\n");
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].status, 'M');
    assert_eq!(files[1].status, 'A');
}
