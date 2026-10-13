// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Session, worktree, and multiplexer fixtures for daemon tests.

use super::*;

pub(crate) fn make_worktree_layout(temp: &Path, worktree_name: &str) -> (PathBuf, PathBuf) {
    let workdir = temp.join("workdir");
    let common_git = temp.join("repo/.git");
    let wt_git = common_git.join(format!("worktrees/{worktree_name}"));
    std::fs::create_dir_all(&workdir).unwrap();
    std::fs::create_dir_all(&wt_git).unwrap();
    std::fs::write(
        workdir.join(".git"),
        format!("gitdir: {}\n", wt_git.display()),
    )
    .unwrap();
    (workdir, common_git)
}

pub(crate) fn arm_pending_pr_lookup(mux: &mut Multiplexer, branch_name: &str, request_id: u64) {
    mux.pr_watch.pull_request_lookup.request_id = request_id;
    mux.pr_watch.pull_request_lookup.in_flight = true;
    mux.pr_watch.pull_request_context_branch = Some(branch(branch_name));
    mux.open_github_context_dialog(Instant::now());
}

pub(crate) fn test_session(rows: u16, cols: u16) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, Some("codex".to_owned()))
}

pub(crate) fn test_shell_session(
    rows: u16,
    cols: u16,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, None)
}

pub(crate) fn pane_kind_cases() -> [(Option<&'static str>, &'static str); 2] {
    [(Some("codex"), "agent"), (None, "shell")]
}

pub(crate) fn test_pane_session(
    rows: u16,
    cols: u16,
    agent: Option<&str>,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    test_session_with_agent(rows, cols, agent.map(str::to_owned))
}

pub(crate) fn test_session_with_agent(
    rows: u16,
    cols: u16,
    agent: Option<String>,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (input_tx, input_rx) = mpsc::unbounded_channel();
    let mut session = Session::new_for_test(
        "Test".to_owned(),
        agent.clone(),
        None,
        (rows, cols),
        100,
        input_tx,
        Arc::new(Mutex::new(Box::new(NullMasterPty))),
        Arc::new(Mutex::new(Box::new(NullChildKiller))),
    );
    session.usage_capability =
        agent.map(
            |agent| jackin_protocol::usage_broker::UsageAccountCapability {
                account_id: format!("test-{agent}"),
                surface_id: agent,
            },
        );
    (session, input_rx)
}

pub(crate) fn test_provider_session(
    provider: jackin_protocol::Provider,
) -> (Session, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (mut session, input_rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: provider.label().to_owned(),
        env_overrides: vec![("ANTHROPIC_AUTH_TOKEN".into(), "zai-test-token".into())],
    });
    (session, input_rx)
}

pub(crate) fn split_tab_mux() -> Multiplexer {
    let mut mux = test_mux(24, 80);
    let mut tab = Tab::new_single("Shell", 1, "test");
    assert!(tab.tree.split_h(1, 2, SplitPosition::After));
    mux.session_supervisor.tabs.push(tab);
    drop(mux.compose_pending_frame());
    mux
}
