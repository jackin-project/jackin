#![cfg(feature = "e2e")]
//! End-to-end smoke tests covering each (agent, mode) pair against a real
//! container. Skipped on machines without a docker daemon.
//!
//! Run with `cargo nextest run --features e2e` on a host with Docker.
//!
//! These skeletons document the expected end-to-end contract for each
//! (agent, mode) pair. A future task fills in the real docker-driving
//! logic — for now the bodies `panic!("smoke test stub")` so that anyone
//! who runs the e2e feature on a docker-equipped host gets a clear
//! "implement me" signal rather than a silent pass.

use std::process::Command;

fn skip_if_no_docker() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
}

#[test]
fn claude_sync_copies_host_credentials() {
    if skip_if_no_docker() {
        return;
    }
    // TODO: Implement smoke test:
    //   - Pre-seed config with [workspaces.e2e.claude].auth_forward = "sync"
    //   - Run jackin in docker with claude agent + e2e workspace + smith role
    //   - Assert container has /home/agent/.claude/.credentials.json matching host
    panic!("smoke test stub — implement when e2e infra is in place");
}

#[test]
fn claude_api_key_wipes_state_and_injects_env() {
    if skip_if_no_docker() {
        return;
    }
    // TODO: same skeleton, mode = ApiKey
    //   - Assert /home/agent/.claude/.credentials.json absent inside container
    //   - Assert ANTHROPIC_API_KEY is present in the agent process env
    panic!("smoke test stub — implement when e2e infra is in place");
}

#[test]
fn claude_oauth_token_wipes_state_and_injects_env() {
    if skip_if_no_docker() {
        return;
    }
    // TODO: mode = OAuthToken
    //   - Assert /home/agent/.claude state files absent
    //   - Assert CLAUDE_CODE_OAUTH_TOKEN is present in the agent process env
    panic!("smoke test stub — implement when e2e infra is in place");
}

#[test]
fn claude_ignore_wipes_state() {
    if skip_if_no_docker() {
        return;
    }
    // TODO: mode = Ignore
    //   - Assert /home/agent/.claude state files absent
    //   - Assert no ANTHROPIC_API_KEY / CLAUDE_CODE_OAUTH_TOKEN in env
    panic!("smoke test stub — implement when e2e infra is in place");
}

#[test]
fn codex_sync_copies_host_auth_json() {
    if skip_if_no_docker() {
        return;
    }
    // TODO: mode = Sync
    //   - Assert container has /home/agent/.codex/auth.json matching host
    panic!("smoke test stub — implement when e2e infra is in place");
}

#[test]
fn codex_api_key_wipes_auth_json() {
    if skip_if_no_docker() {
        return;
    }
    // TODO: mode = ApiKey
    //   - Assert /home/agent/.codex/auth.json absent inside container
    //   - Assert OPENAI_API_KEY is present in the agent process env
    panic!("smoke test stub — implement when e2e infra is in place");
}

#[test]
fn codex_ignore_wipes_auth_json() {
    if skip_if_no_docker() {
        return;
    }
    // TODO: mode = Ignore
    //   - Assert /home/agent/.codex/auth.json absent
    //   - Assert no OPENAI_API_KEY in env
    panic!("smoke test stub — implement when e2e infra is in place");
}
