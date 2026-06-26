//! Tests for `version_check`.
use super::*;
use jackin_core::Agent;
use tempfile::tempdir;

fn seed_latest(paths: &JackinPaths, agent: Agent, version: &str) {
    let release = crate::agent_binary::AgentRelease {
        agent,
        version: version.to_owned(),
        url: "https://example.invalid/agent".to_owned(),
        checksum: None,
        archive_member: None,
    };
    let path = paths
        .cache_dir
        .join("agent-binaries")
        .join(agent.slug())
        .join("latest.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string(&release).unwrap()).unwrap();
}

#[test]
fn stores_and_reads_image_version() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    store_version(&paths, Agent::Claude, "jk_agent-smith", "2.1.91");

    assert_eq!(
        stored_version(&paths, Agent::Claude, "jk_agent-smith"),
        Some("2.1.91".to_owned())
    );
}

#[tokio::test]
async fn needs_update_when_versions_differ() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    store_version(&paths, Agent::Claude, "jk_agent-smith", "2.1.91");
    seed_latest(&paths, Agent::Claude, "2.1.92");

    assert_eq!(
        needs_agent_update(&paths, "jk_agent-smith", Agent::Claude).await,
        AgentVersionCheck::Stale
    );
}

#[tokio::test]
async fn no_update_when_versions_match() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    store_version(&paths, Agent::Claude, "jk_agent-smith", "2.1.92");
    seed_latest(&paths, Agent::Claude, "2.1.92");

    assert_eq!(
        needs_agent_update(&paths, "jk_agent-smith", Agent::Claude).await,
        AgentVersionCheck::UpToDate
    );
}

#[tokio::test]
async fn no_update_on_first_build() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    // No stored baseline → first build → UpToDate (never warns/rebuilds).
    seed_latest(&paths, Agent::Claude, "2.1.92");
    assert_eq!(
        needs_agent_update(&paths, "jk_agent-smith", Agent::Claude).await,
        AgentVersionCheck::UpToDate
    );
}

#[test]
fn parse_claude_version_strips_suffix() {
    assert_eq!(
        Agent::Claude
            .runtime()
            .parse_version("2.1.96 (Claude Code)"),
        Some("2.1.96")
    );
}

#[test]
fn parse_claude_version_bare_semver() {
    assert_eq!(
        Agent::Claude.runtime().parse_version("2.1.96"),
        Some("2.1.96")
    );
}

#[test]
fn parse_claude_version_two_part() {
    assert_eq!(Agent::Claude.runtime().parse_version("1.0"), Some("1.0"));
}

#[test]
fn parse_claude_version_rejects_garbage() {
    assert_eq!(Agent::Claude.runtime().parse_version("not-a-version"), None);
}

#[test]
fn parse_claude_version_rejects_empty() {
    assert_eq!(Agent::Claude.runtime().parse_version(""), None);
}

#[test]
fn stores_and_reads_opencode_image_version() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    store_version(&paths, Agent::Opencode, "jk_the-architect", "1.14.48");

    assert_eq!(
        stored_version(&paths, Agent::Opencode, "jk_the-architect"),
        Some("1.14.48".to_owned())
    );
}

#[tokio::test]
async fn opencode_needs_update_when_versions_differ() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    store_version(&paths, Agent::Opencode, "jk_the-architect", "1.14.47");
    seed_latest(&paths, Agent::Opencode, "1.14.48");

    assert_eq!(
        needs_agent_update(&paths, "jk_the-architect", Agent::Opencode).await,
        AgentVersionCheck::Stale
    );
}

#[tokio::test]
async fn opencode_no_update_when_versions_match() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    store_version(&paths, Agent::Opencode, "jk_the-architect", "1.14.48");
    seed_latest(&paths, Agent::Opencode, "1.14.48");

    assert_eq!(
        needs_agent_update(&paths, "jk_the-architect", Agent::Opencode).await,
        AgentVersionCheck::UpToDate
    );
}

#[test]
fn stores_and_reads_kimi_image_version() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    store_version(&paths, Agent::Kimi, "jk_the-architect", "1.2.3");

    assert_eq!(
        stored_version(&paths, Agent::Kimi, "jk_the-architect"),
        Some("1.2.3".to_owned())
    );
}

#[test]
fn kimi_version_stored_separately_from_claude_version() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    store_version(&paths, Agent::Claude, "jk_test", "2.0.0");
    store_version(&paths, Agent::Kimi, "jk_test", "1.0.0");

    assert_eq!(
        stored_version(&paths, Agent::Claude, "jk_test"),
        Some("2.0.0".to_owned())
    );
    assert_eq!(
        stored_version(&paths, Agent::Kimi, "jk_test"),
        Some("1.0.0".to_owned())
    );
}

#[test]
fn parse_kimi_version_prefixed_with_kimi() {
    assert_eq!(
        Agent::Kimi.runtime().parse_version("kimi 1.2.3"),
        Some("1.2.3")
    );
}

#[test]
fn parse_kimi_version_bare_semver() {
    assert_eq!(Agent::Kimi.runtime().parse_version("1.2.3"), Some("1.2.3"));
}

#[test]
fn parse_kimi_version_two_part() {
    assert_eq!(Agent::Kimi.runtime().parse_version("kimi 1.0"), Some("1.0"));
}

#[test]
fn parse_kimi_version_rejects_v_prefix() {
    assert_eq!(Agent::Kimi.runtime().parse_version("kimi v1.2.3"), None);
}

#[test]
fn parse_kimi_version_rejects_garbage() {
    assert_eq!(Agent::Kimi.runtime().parse_version("not-a-version"), None);
}

#[test]
fn parse_kimi_version_rejects_empty() {
    assert_eq!(Agent::Kimi.runtime().parse_version(""), None);
}

#[test]
fn parse_opencode_version_bare_semver() {
    assert_eq!(
        Agent::Opencode.runtime().parse_version("1.14.48"),
        Some("1.14.48")
    );
}

#[test]
fn parse_opencode_version_strips_v_prefix() {
    assert_eq!(
        Agent::Opencode.runtime().parse_version("v1.14.48"),
        Some("1.14.48")
    );
}

#[test]
fn parse_opencode_version_rejects_garbage() {
    assert_eq!(
        Agent::Opencode.runtime().parse_version("not-a-version"),
        None
    );
}

#[test]
fn parse_opencode_version_rejects_empty() {
    assert_eq!(Agent::Opencode.runtime().parse_version(""), None);
}

#[test]
fn parse_amp_version_finds_semver_token() {
    assert_eq!(
        Agent::Amp
            .runtime()
            .parse_version("amp 0.0.1779945647-g362e01"),
        Some("0.0.1779945647-g362e01")
    );
}

#[test]
fn parse_codex_version_finds_semver_token() {
    assert_eq!(
        Agent::Codex.runtime().parse_version("codex 0.134.0"),
        Some("0.134.0")
    );
}
