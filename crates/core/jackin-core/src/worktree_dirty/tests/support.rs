// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const BASE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub(super) fn block_on<F: Future>(fut: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

#[derive(Default)]
pub(super) struct MockGit {
    pub(super) porcelain: String,
    pub(super) for_each_ref: String,
    pub(super) rev_list: String,
    pub(super) symbolic_ref_ok: bool,
    pub(super) rev_parse_head: String,
    pub(super) log_output: String,
    pub(super) fail_subcommand: Option<&'static str>,
}

impl CommandRunner for MockGit {
    async fn run(
        &mut self,
        _program: &str,
        _args: &[&str],
        _cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        std::future::ready(()).await;
        Ok(())
    }

    async fn capture(
        &mut self,
        _program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        std::future::ready(()).await;
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

pub(super) fn assess(mock: &mut MockGit) -> WorktreeState {
    block_on(assess_worktree("/wt", BASE, mock, |_| {})).expect("assess never errs")
}
