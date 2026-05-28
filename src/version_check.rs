use crate::paths::JackinPaths;
use std::path::PathBuf;

/// File that records the Claude Code version baked into a given Docker image.
fn image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths
        .cache_dir
        .join(format!("image-claude-version/{image}"))
}

/// File that records the `OpenCode` version baked into a given `Docker` image.
fn opencode_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths
        .cache_dir
        .join(format!("image-opencode-version/{image}"))
}

/// File that records the Kimi version baked into a given Docker image.
fn kimi_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-kimi-version/{image}"))
}

fn amp_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-amp-version/{image}"))
}

fn codex_image_version_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-codex-version/{image}"))
}

/// Read the Claude Code version we stored for a previously-built image.
pub fn stored_image_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the Claude Code version that was just installed into an image.
pub fn store_image_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

/// Read the `OpenCode` version we stored for a previously-built image.
pub fn stored_opencode_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = opencode_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the `OpenCode` version that was just installed into an image.
pub fn store_opencode_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = opencode_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

/// Read the Kimi version we stored for a previously-built image.
pub fn stored_kimi_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = kimi_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the Kimi version that was just installed into an image.
pub fn store_kimi_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = kimi_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

pub fn stored_amp_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = amp_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn store_amp_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = amp_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

pub fn stored_codex_version(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = codex_image_version_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

pub fn store_codex_version(paths: &JackinPaths, image: &str, version: &str) {
    let path = codex_image_version_path(paths, image);
    let _ = write_cached(&path, version);
}

pub async fn needs_agent_update(
    paths: &JackinPaths,
    image: &str,
    agent: crate::agent::Agent,
) -> bool {
    let installed = match agent {
        crate::agent::Agent::Claude => stored_image_version(paths, image),
        crate::agent::Agent::Codex => stored_codex_version(paths, image),
        crate::agent::Agent::Amp => stored_amp_version(paths, image),
        crate::agent::Agent::Kimi => stored_kimi_version(paths, image),
        crate::agent::Agent::Opencode => stored_opencode_version(paths, image),
    };
    let Some(installed) = installed else {
        return false;
    };
    let Some(latest) = crate::agent_binary::latest_release(paths, agent).await else {
        return false;
    };
    installed != latest.version
}

/// File that records the last `JACKIN_CACHE_BUST` value used to build an image.
fn cache_bust_path(paths: &JackinPaths, image: &str) -> PathBuf {
    paths.cache_dir.join(format!("image-cache-bust/{image}"))
}

/// Read the last `JACKIN_CACHE_BUST` value used for an image build.
pub fn stored_cache_bust(paths: &JackinPaths, image: &str) -> Option<String> {
    let path = cache_bust_path(paths, image);
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Persist the `JACKIN_CACHE_BUST` value used for a build so that
/// subsequent non-rebuild launches replay the same value and hit the
/// same Docker cache layer.
pub fn store_cache_bust(paths: &JackinPaths, image: &str, value: &str) {
    let path = cache_bust_path(paths, image);
    if let Err(e) = write_cached(&path, value) {
        eprintln!(
            "warning: failed to persist JACKIN_CACHE_BUST for {image}: {e}; \
             subsequent non-rebuild launches may replay the wrong cache layer"
        );
    }
}

/// Extract a bare semver string from `claude --version` output.
///
/// The command returns e.g. `"2.1.96 (Claude Code)"` but we only need the
/// `"2.1.96"` portion to compare against the npm registry.  Returns `None`
/// when the output doesn't look like a version string.
pub fn parse_claude_version(raw: &str) -> Option<&str> {
    let token = raw.split_whitespace().next()?;
    if token.split('.').count() < 2 || !token.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(token)
}

/// Extract a bare semver string from `kimi --version` output.
///
/// The command returns e.g. `"kimi 1.2.3"` but we only need the `"1.2.3"`
/// portion. Returns `None` when the output doesn't look like a version string.
pub fn parse_kimi_version(raw: &str) -> Option<&str> {
    let mut tokens = raw.split_whitespace();
    let first = tokens.next()?;
    if first.split('.').count() >= 2 && first.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(first);
    }
    let second = tokens.next()?;
    if second.split('.').count() >= 2 && second.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(second);
    }
    None
}

/// Extract a bare semver string from `opencode --version` output.
///
/// The command returns e.g. `"1.14.48"` or `"v1.14.48"`. Strip a leading `v`
/// if present, then validate it looks like a semver.
pub fn parse_opencode_version(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let token = trimmed.strip_prefix('v').unwrap_or(trimmed);
    if token.split('.').count() < 2 || !token.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(token)
}

pub fn parse_amp_version(raw: &str) -> Option<&str> {
    raw.split_whitespace().find(|token| {
        token.split('.').count() >= 2 && token.starts_with(|c: char| c.is_ascii_digit())
    })
}

pub fn parse_codex_version(raw: &str) -> Option<&str> {
    raw.split_whitespace().find(|token| {
        token.split('.').count() >= 2 && token.starts_with(|c: char| c.is_ascii_digit())
    })
}

/// Write content to a cache file, creating parent directories as needed.
fn write_cached(path: &PathBuf, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;
    use crate::paths::JackinPaths;
    use tempfile::tempdir;

    fn seed_latest(paths: &JackinPaths, agent: Agent, version: &str) {
        let release = crate::agent_binary::AgentRelease {
            agent,
            version: version.to_string(),
            url: "https://example.invalid/agent".to_string(),
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

        store_image_version(&paths, "jk_agent-smith", "2.1.91");

        assert_eq!(
            stored_image_version(&paths, "jk_agent-smith"),
            Some("2.1.91".to_string())
        );
    }

    #[tokio::test]
    async fn needs_update_when_versions_differ() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_image_version(&paths, "jk_agent-smith", "2.1.91");
        seed_latest(&paths, Agent::Claude, "2.1.92");

        assert!(needs_agent_update(&paths, "jk_agent-smith", Agent::Claude).await);
    }

    #[tokio::test]
    async fn no_update_when_versions_match() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_image_version(&paths, "jk_agent-smith", "2.1.92");
        seed_latest(&paths, Agent::Claude, "2.1.92");

        assert!(!needs_agent_update(&paths, "jk_agent-smith", Agent::Claude).await);
    }

    #[tokio::test]
    async fn no_update_on_first_build() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        seed_latest(&paths, Agent::Claude, "2.1.92");
        assert!(!needs_agent_update(&paths, "jk_agent-smith", Agent::Claude).await);
    }

    #[test]
    fn parse_claude_version_strips_suffix() {
        assert_eq!(parse_claude_version("2.1.96 (Claude Code)"), Some("2.1.96"));
    }

    #[test]
    fn parse_claude_version_bare_semver() {
        assert_eq!(parse_claude_version("2.1.96"), Some("2.1.96"));
    }

    #[test]
    fn parse_claude_version_two_part() {
        assert_eq!(parse_claude_version("1.0"), Some("1.0"));
    }

    #[test]
    fn parse_claude_version_rejects_garbage() {
        assert_eq!(parse_claude_version("not-a-version"), None);
    }

    #[test]
    fn parse_claude_version_rejects_empty() {
        assert_eq!(parse_claude_version(""), None);
    }

    #[test]
    fn stores_and_reads_opencode_image_version() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_opencode_version(&paths, "jk_the-architect", "1.14.48");

        assert_eq!(
            stored_opencode_version(&paths, "jk_the-architect"),
            Some("1.14.48".to_string())
        );
    }

    #[tokio::test]
    async fn opencode_needs_update_when_versions_differ() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_opencode_version(&paths, "jk_the-architect", "1.14.47");
        seed_latest(&paths, Agent::Opencode, "1.14.48");

        assert!(needs_agent_update(&paths, "jk_the-architect", Agent::Opencode).await);
    }

    #[tokio::test]
    async fn opencode_no_update_when_versions_match() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_opencode_version(&paths, "jk_the-architect", "1.14.48");
        seed_latest(&paths, Agent::Opencode, "1.14.48");

        assert!(!needs_agent_update(&paths, "jk_the-architect", Agent::Opencode).await);
    }

    #[test]
    fn stores_and_reads_kimi_image_version() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_kimi_version(&paths, "jk_the-architect", "1.2.3");

        assert_eq!(
            stored_kimi_version(&paths, "jk_the-architect"),
            Some("1.2.3".to_string())
        );
    }

    #[test]
    fn kimi_version_stored_separately_from_claude_version() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        paths.ensure_base_dirs().unwrap();

        store_image_version(&paths, "jk_test", "2.0.0");
        store_kimi_version(&paths, "jk_test", "1.0.0");

        assert_eq!(
            stored_image_version(&paths, "jk_test"),
            Some("2.0.0".to_string())
        );
        assert_eq!(
            stored_kimi_version(&paths, "jk_test"),
            Some("1.0.0".to_string())
        );
    }

    #[test]
    fn parse_kimi_version_prefixed_with_kimi() {
        assert_eq!(parse_kimi_version("kimi 1.2.3"), Some("1.2.3"));
    }

    #[test]
    fn parse_kimi_version_bare_semver() {
        assert_eq!(parse_kimi_version("1.2.3"), Some("1.2.3"));
    }

    #[test]
    fn parse_kimi_version_two_part() {
        assert_eq!(parse_kimi_version("kimi 1.0"), Some("1.0"));
    }

    #[test]
    fn parse_kimi_version_rejects_v_prefix() {
        assert_eq!(parse_kimi_version("kimi v1.2.3"), None);
    }

    #[test]
    fn parse_kimi_version_rejects_garbage() {
        assert_eq!(parse_kimi_version("not-a-version"), None);
    }

    #[test]
    fn parse_kimi_version_rejects_empty() {
        assert_eq!(parse_kimi_version(""), None);
    }

    #[test]
    fn parse_opencode_version_bare_semver() {
        assert_eq!(parse_opencode_version("1.14.48"), Some("1.14.48"));
    }

    #[test]
    fn parse_opencode_version_strips_v_prefix() {
        assert_eq!(parse_opencode_version("v1.14.48"), Some("1.14.48"));
    }

    #[test]
    fn parse_opencode_version_rejects_garbage() {
        assert_eq!(parse_opencode_version("not-a-version"), None);
    }

    #[test]
    fn parse_opencode_version_rejects_empty() {
        assert_eq!(parse_opencode_version(""), None);
    }

    #[test]
    fn parse_amp_version_finds_semver_token() {
        assert_eq!(
            parse_amp_version("amp 0.0.1779945647-g362e01"),
            Some("0.0.1779945647-g362e01")
        );
    }

    #[test]
    fn parse_codex_version_finds_semver_token() {
        assert_eq!(parse_codex_version("codex 0.134.0"), Some("0.134.0"));
    }
}
